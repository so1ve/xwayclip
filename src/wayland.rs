use wl_clipboard_rs::copy::{Error, MimeSource, MimeType, Options, Source};

use crate::snapshot::{Offer, Snapshot};

const PLAIN_TEXT_MIME_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "TEXT",
    "STRING",
];

pub fn publish(snapshot: Snapshot) -> Result<(), Error> {
    let (sources, has_plain_text) = sources(snapshot);
    let mut options = Options::new();
    options.omit_additional_text_mime_types(!has_plain_text);

    options.copy_multi(sources)
}

fn sources(snapshot: Snapshot) -> (Vec<MimeSource>, bool) {
    let mut offers = snapshot.into_offers();
    let plain_text = take_plain_text(&mut offers);
    let has_plain_text = plain_text.is_some();
    let mut sources = Vec::with_capacity(offers.len() + usize::from(plain_text.is_some()));

    if let Some(offer) = plain_text {
        sources.push(MimeSource {
            mime_type: MimeType::Text,
            source: Source::Bytes(offer.data.into_boxed_slice()),
        });
    }

    sources.extend(offers.into_iter().map(|offer| MimeSource {
        mime_type: MimeType::Specific(offer.mime_type),
        source: Source::Bytes(offer.data.into_boxed_slice()),
    }));

    (sources, has_plain_text)
}

fn take_plain_text(offers: &mut Vec<Offer>) -> Option<Offer> {
    for mime_type in PLAIN_TEXT_MIME_TYPES {
        if let Some(index) = offers
            .iter()
            .position(|offer| offer.mime_type == mime_type)
        {
            return Some(offers.remove(index));
        }
    }

    None
}
