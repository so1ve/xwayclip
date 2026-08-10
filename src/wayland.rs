use wl_clipboard_rs::copy::{Error, MimeSource, MimeType, Options, Source};

use crate::snapshot::Snapshot;

pub fn publish(snapshot: Snapshot) -> Result<(), Error> {
    let sources = snapshot
        .into_offers()
        .into_iter()
        .map(|offer| MimeSource {
            source: Source::Bytes(offer.data.into_boxed_slice()),
            mime_type: MimeType::Specific(offer.mime_type),
        })
        .collect();

    Options::new().copy_multi(sources)
}
