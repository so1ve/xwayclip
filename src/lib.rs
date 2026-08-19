mod snapshot;
mod wayland;
mod x11;

use std::cmp::min;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Error, Result, anyhow, ensure};
use blake3::Hash;
use tracing::{debug, info, warn};

use crate::snapshot::Snapshot;

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub struct Config {
    max_target_bytes: usize,
    max_total_bytes: usize,
    transfer_timeout: Duration,
}

impl Config {
    pub fn new(
        max_target_bytes: usize,
        max_total_bytes: usize,
        transfer_timeout: Duration,
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
        })
    }
}

enum ClipboardUpdate {
    Set(Snapshot),
    Cleared,
}

enum WorkerEvent {
    Ready,
    X11(ClipboardUpdate),
    Wayland(ClipboardUpdate),
    Failed(Error),
}

enum X11Command {
    Set(Snapshot),
    Clear,
}

struct X11Worker {
    commands: Sender<X11Command>,
    wake: UnixStream,
}

impl X11Worker {
    fn send(&mut self, command: X11Command) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|_| anyhow!("clipboard worker stopped"))?;

        self.wake
            .write_all(&[1])
            .context("failed to wake clipboard worker")
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ClipboardState {
    Unknown,
    Cleared,
    Content(Hash),
}

pub fn run(config: Config) -> ! {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        if let Err(error) = run_session(config, &mut reconnect_delay) {
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

fn join_worker(thread: JoinHandle<()>, side: &str) -> Result<()> {
    thread
        .join()
        .map_err(|_| anyhow!("{side} clipboard worker panicked"))
}

fn run_session(config: Config, reconnect_delay: &mut Duration) -> Result<()> {
    let (events, worker_events) = mpsc::channel();
    let (mut x11, x11_thread) = spawn_x11(config, events.clone())?;
    let (wayland_shutdown, wayland_thread) = match spawn_wayland(config, events) {
        Ok(worker) => worker,
        Err(error) => {
            drop(x11);
            join_worker(x11_thread, "X11")?;

            return Err(error);
        }
    };

    let result = coordinate(&worker_events, &mut x11, reconnect_delay);

    drop(x11);
    drop(wayland_shutdown);
    join_worker(x11_thread, "X11")?;
    join_worker(wayland_thread, "Wayland")?;

    result
}

fn spawn_x11(config: Config, events: Sender<WorkerEvent>) -> Result<(X11Worker, JoinHandle<()>)> {
    let (commands, command_rx) = mpsc::channel();
    let (wake_worker, wake_coordinator) =
        UnixStream::pair().context("failed to create X11 worker wake pipe")?;
    let thread = thread::Builder::new()
        .name("xwayclip-x11".to_owned())
        .spawn(move || {
            if let Err(error) = x11::run(config, events.clone(), &command_rx, wake_worker) {
                events
                    .send(WorkerEvent::Failed(error.context("X11 worker failed")))
                    .unwrap();
            }
        })
        .context("failed to start X11 clipboard worker")?;

    Ok((
        X11Worker {
            commands,
            wake: wake_coordinator,
        },
        thread,
    ))
}

fn spawn_wayland(
    config: Config,
    events: Sender<WorkerEvent>,
) -> Result<(Sender<()>, JoinHandle<()>)> {
    let (shutdown, shutdown_rx) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("xwayclip-wl".to_owned())
        .spawn(move || {
            if let Err(error) = wayland::run(config, &events, &shutdown_rx) {
                events
                    .send(WorkerEvent::Failed(error.context("Wayland worker failed")))
                    .unwrap();
            }
        })
        .context("failed to start Wayland clipboard worker")?;

    Ok((shutdown, thread))
}

fn coordinate(
    events: &Receiver<WorkerEvent>,
    x11: &mut X11Worker,
    reconnect_delay: &mut Duration,
) -> Result<()> {
    let mut x11_state = ClipboardState::Unknown;
    let mut wayland_state = ClipboardState::Unknown;
    let mut ready_workers = 0;

    loop {
        match events
            .recv()
            .context("all clipboard workers stopped unexpectedly")?
        {
            WorkerEvent::Ready => {
                ready_workers += 1;
                if ready_workers == 2 {
                    *reconnect_delay = INITIAL_RECONNECT_DELAY;
                    info!("synchronizing CLIPBOARD bidirectionally between X11 and Wayland");
                }
            }
            WorkerEvent::X11(update) => {
                sync_from_x11(update, &mut x11_state, &mut wayland_state)?;
            }
            WorkerEvent::Wayland(update) => {
                sync_from_wayland(update, x11, &mut x11_state, &mut wayland_state)?;
            }
            WorkerEvent::Failed(error) => return Err(error),
        }
    }
}

fn sync_from_x11(
    update: ClipboardUpdate,
    x11_state: &mut ClipboardState,
    wayland_state: &mut ClipboardState,
) -> Result<()> {
    match update {
        ClipboardUpdate::Cleared => {
            if *x11_state == ClipboardState::Cleared {
                return Ok(());
            }
            *x11_state = ClipboardState::Cleared;

            wayland::clear_clipboard().context("failed to clear the Wayland clipboard")?;
            *wayland_state = ClipboardState::Cleared;
            info!("cleared Wayland clipboard from X11");
        }
        ClipboardUpdate::Set(snapshot) => {
            let source_fingerprint = snapshot.fingerprint();
            if *x11_state == ClipboardState::Content(source_fingerprint) {
                debug!("ignoring X11 clipboard content already known to the bridge");

                return Ok(());
            }
            *x11_state = ClipboardState::Content(source_fingerprint);

            let destination_fingerprint = snapshot.wayland_fingerprint();
            let total_bytes = snapshot.total_bytes();
            let mime_types = snapshot.mime_types().collect::<Vec<_>>();
            let offer_count = mime_types.len();
            let mime_types = mime_types.join(", ");
            wayland::publish(snapshot).context("failed to publish the clipboard to Wayland")?;
            *wayland_state = ClipboardState::Content(destination_fingerprint);
            info!(
                offer_count,
                total_bytes,
                %mime_types,
                "published X11 clipboard to Wayland"
            );
        }
    }

    Ok(())
}

fn sync_from_wayland(
    update: ClipboardUpdate,
    x11: &mut X11Worker,
    x11_state: &mut ClipboardState,
    wayland_state: &mut ClipboardState,
) -> Result<()> {
    match update {
        ClipboardUpdate::Cleared => {
            if *wayland_state == ClipboardState::Cleared {
                return Ok(());
            }
            *wayland_state = ClipboardState::Cleared;

            x11.send(X11Command::Clear)
                .context("failed to request X11 clipboard clearing")?;
            *x11_state = ClipboardState::Cleared;
            info!("cleared X11 clipboard from Wayland");
        }
        ClipboardUpdate::Set(snapshot) => {
            let source_fingerprint = snapshot.fingerprint();
            if *wayland_state == ClipboardState::Content(source_fingerprint) {
                debug!("ignoring Wayland clipboard content already known to the bridge");

                return Ok(());
            }
            *wayland_state = ClipboardState::Content(source_fingerprint);

            let x11_targets = snapshot.x11_targets();
            if x11_targets.is_empty() {
                debug!("Wayland clipboard has no transferable X11 targets");

                return Ok(());
            }
            let offer_count = x11_targets.len();
            let total_bytes = snapshot.total_bytes();
            let mime_types = x11_targets
                .into_iter()
                .map(|(mime_type, _)| mime_type)
                .collect::<Vec<_>>()
                .join(", ");
            x11.send(X11Command::Set(snapshot))
                .context("failed to publish the clipboard to X11")?;
            *x11_state = ClipboardState::Content(source_fingerprint);
            info!(
                offer_count,
                total_bytes,
                %mime_types,
                "published Wayland clipboard to X11"
            );
        }
    }

    Ok(())
}
