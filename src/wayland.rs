use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tracing::{debug, trace, warn};
use wl_clipboard_rs::copy::{
    ClipboardType, Error, MimeSource, MimeType, Options, Seat, Source, clear,
};
use wl_clipboard_watch::{Config as WatcherConfig, Event, Selection, Transfer, Watcher};

use crate::snapshot::{Offer, Snapshot};
use crate::{ClipboardUpdate, Config, WorkerEvent};

const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub fn run(config: Config, events: &Sender<WorkerEvent>, shutdown: &Receiver<()>) -> Result<()> {
    let watcher_config = WatcherConfig::new(
        config.max_target_bytes.min(config.max_total_bytes),
        config.transfer_timeout,
    )?;
    let mut watcher = Watcher::connect_with(watcher_config)?;
    events.send(WorkerEvent::Ready).unwrap();

    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }

        let Some(event) = watcher
            .next_event_timeout(SHUTDOWN_POLL_INTERVAL)
            .context("failed while watching the Wayland clipboard")?
        else {
            continue;
        };

        match event {
            Event::Cleared => events
                .send(WorkerEvent::Wayland(ClipboardUpdate::Cleared))
                .unwrap(),
            Event::Selection(selection) => match capture(&mut watcher, &selection, config) {
                Ok(Some(snapshot)) => events
                    .send(WorkerEvent::Wayland(ClipboardUpdate::Set(snapshot)))
                    .unwrap(),
                Ok(None) => trace!("discarded a stale or empty Wayland clipboard snapshot"),
                Err(error) => {
                    warn!(%error, "could not capture the complete Wayland clipboard snapshot");
                }
            },
        }
    }
}

fn capture(
    watcher: &mut Watcher,
    selection: &Selection,
    config: Config,
) -> Result<Option<Snapshot>> {
    let mut mime_types = selection.mime_types().to_vec();
    mime_types.sort_unstable();
    mime_types.dedup();

    let mut offers = Vec::with_capacity(mime_types.len());
    let mut total_bytes = 0_usize;

    for mime_type in mime_types {
        if mime_type.is_empty() || mime_type.contains('\0') {
            debug!(?mime_type, "skipping invalid Wayland MIME type");
            continue;
        }

        let remaining_total = config.max_total_bytes - total_bytes;
        ensure!(
            remaining_total > 0,
            "Wayland clipboard exceeds its total size limit"
        );

        let transfer = watcher
            .receive(selection, &mime_type)
            .with_context(|| format!("failed to receive Wayland MIME type {mime_type:?}"))?;
        let Transfer::Complete(data) = transfer else {
            return Ok(None);
        };
        ensure!(
            data.len() <= remaining_total,
            "Wayland clipboard exceeds its total size limit"
        );

        total_bytes += data.len();
        trace!(%mime_type, bytes = data.len(), "captured Wayland clipboard target");
        offers.push(Offer { mime_type, data });
    }

    Ok((!offers.is_empty()).then(|| Snapshot::new(offers)))
}

pub fn publish(snapshot: Snapshot) -> Result<(), Error> {
    let preferred_text = snapshot.preferred_text_index();
    let mut offers = snapshot.into_offers();
    if let Some(index) = preferred_text {
        let offer = offers.remove(index);
        offers.insert(0, offer);
    }

    let sources = offers
        .into_iter()
        .map(|offer| MimeSource {
            mime_type: MimeType::Specific(offer.mime_type),
            source: Source::Bytes(offer.data.into_boxed_slice()),
        })
        .collect();

    let mut options = Options::new();
    options.omit_additional_text_mime_types(preferred_text.is_none());
    options.copy_multi(sources)
}

pub fn clear_clipboard() -> Result<(), Error> {
    clear(ClipboardType::Regular, Seat::All)
}
