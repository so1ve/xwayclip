mod snapshot;
mod wayland;
mod x11;

use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tracing::{debug, info};

use crate::x11::ClipboardWatcher;

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
    let mut watcher = ClipboardWatcher::new(config)?;
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

        info!(offer_count, total_bytes, %mime_types, "published clipboard to Wayland");
    }
}
