mod snapshot;
mod wayland;
mod x11;

use std::cmp::min;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tracing::{debug, info, warn};

use crate::x11::ClipboardWatcher;

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub struct Config {
    max_target_bytes: usize,
    max_total_bytes: usize,
    transfer_timeout: Duration,
    initial_sync: bool,
}

impl Config {
    pub fn new(
        max_target_bytes: usize,
        max_total_bytes: usize,
        transfer_timeout: Duration,
        initial_sync: bool,
    ) -> Result<Self> {
        ensure!(
            max_target_bytes > 0,
            "max target size must be greater than zero"
        );
        ensure!(
            max_total_bytes > 0,
            "max total size must be greater than zero"
        );
        ensure!(
            !transfer_timeout.is_zero(),
            "transfer timeout must be greater than zero"
        );

        Ok(Self {
            max_target_bytes,
            max_total_bytes,
            transfer_timeout,
            initial_sync,
        })
    }
}

pub fn run(config: Config) -> Result<()> {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    let mut sync_current_owner = config.initial_sync;

    loop {
        let result = match ClipboardWatcher::new(config, sync_current_owner) {
            Ok(watcher) => {
                sync_current_owner = true;
                run_session(watcher, &mut reconnect_delay)
            }
            Err(error) => Err(error),
        };

        if let Err(error) = result {
            warn!(
                %error,
                retry_in = ?reconnect_delay,
                "clipboard session failed; reconnecting"
            );
        }

        thread::sleep(reconnect_delay);
        reconnect_delay = min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
    }
}

fn run_session(mut watcher: ClipboardWatcher, reconnect_delay: &mut Duration) -> Result<()> {
    let mut last_fingerprint = None;

    info!("watching X11 CLIPBOARD selection");

    loop {
        let snapshot = watcher
            .next_snapshot()
            .context("failed while watching X11 clipboard")?;
        let fingerprint = snapshot.fingerprint();

        if last_fingerprint == Some(fingerprint) {
            debug!("ignoring a clipboard snapshot already published to Wayland");
            continue;
        }

        let offer_count = snapshot.len();
        let total_bytes = snapshot.total_bytes();
        let mime_types = snapshot.mime_types().collect::<Vec<_>>().join(", ");

        wayland::publish(snapshot).context("failed to publish the clipboard to Wayland")?;
        last_fingerprint = Some(fingerprint);
        *reconnect_delay = INITIAL_RECONNECT_DELAY;

        info!(offer_count, total_bytes, %mime_types, "published clipboard to Wayland");
    }
}
