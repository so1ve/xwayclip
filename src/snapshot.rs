use blake3::{Hash, Hasher};

pub const PLAIN_TEXT_MIME_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "TEXT",
    "STRING",
];

const X11_CONTROL_TARGETS: &[&str] = &[
    "ATOM",
    "ATOM_PAIR",
    "DELETE",
    "INCR",
    "INSERT_PROPERTY",
    "INSERT_SELECTION",
    "MULTIPLE",
    "SAVE_TARGETS",
    "TARGETS",
    "TIMESTAMP",
];

pub struct Offer {
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub struct Snapshot {
    offers: Vec<Offer>,
}

impl Snapshot {
    pub fn new(mut offers: Vec<Offer>) -> Self {
        offers.sort_by(|left, right| left.mime_type.cmp(&right.mime_type));
        offers.dedup_by(|left, right| left.mime_type == right.mime_type);

        Self { offers }
    }

    pub fn fingerprint(&self) -> Hash {
        fingerprint(
            self.offers
                .iter()
                .map(|offer| (offer.mime_type.as_str(), offer.data.as_slice())),
        )
    }

    pub fn wayland_fingerprint(&self) -> Hash {
        let mut targets = self
            .offers
            .iter()
            .map(|offer| (offer.mime_type.as_str(), offer.data.as_slice()))
            .collect::<Vec<_>>();

        if let Some(text_index) = self.preferred_text_index() {
            let text = self.offers[text_index].data.as_slice();
            for mime_type in PLAIN_TEXT_MIME_TYPES {
                if !self.offers.iter().any(|offer| offer.mime_type == mime_type) {
                    targets.push((mime_type, text));
                }
            }
        }

        fingerprint(targets)
    }

    pub fn preferred_text_index(&self) -> Option<usize> {
        PLAIN_TEXT_MIME_TYPES.iter().find_map(|mime_type| {
            self.offers
                .iter()
                .position(|offer| offer.mime_type == *mime_type)
        })
    }

    pub fn x11_targets(&self) -> Vec<(&str, usize)> {
        let mut targets = self
            .offers
            .iter()
            .enumerate()
            .filter(|(_, offer)| is_x11_target(&offer.mime_type))
            .map(|(index, offer)| (offer.mime_type.as_str(), index))
            .collect::<Vec<_>>();

        let utf8_text = self
            .offers
            .iter()
            .position(|offer| offer.mime_type == "text/plain;charset=utf-8");
        let has_utf8_string = targets
            .iter()
            .any(|(mime_type, _)| *mime_type == "UTF8_STRING");
        if let Some(source) = utf8_text
            && !has_utf8_string
        {
            targets.push(("UTF8_STRING", source));
        }

        targets.sort_unstable_by(|left, right| left.0.cmp(right.0));

        targets
    }

    pub fn offers(&self) -> &[Offer] {
        &self.offers
    }

    pub fn total_bytes(&self) -> usize {
        self.offers.iter().map(|offer| offer.data.len()).sum()
    }

    pub fn mime_types(&self) -> impl Iterator<Item = &str> {
        self.offers.iter().map(|offer| offer.mime_type.as_str())
    }

    pub fn into_offers(self) -> Vec<Offer> {
        self.offers
    }
}

fn fingerprint<'a>(targets: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Hash {
    let mut targets = targets.into_iter().collect::<Vec<_>>();
    targets.sort_unstable_by(|left, right| left.0.cmp(right.0));

    let mut hasher = Hasher::new();
    for (mime_type, data) in targets {
        hasher.update(&mime_type.len().to_le_bytes());
        hasher.update(mime_type.as_bytes());
        hasher.update(&data.len().to_le_bytes());
        hasher.update(data);
    }

    hasher.finalize()
}

pub fn is_x11_target(name: &str) -> bool {
    !name.is_empty() && !name.contains('\0') && !X11_CONTROL_TARGETS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::{Offer, Snapshot};

    fn offer(mime_type: &str, data: &[u8]) -> Offer {
        Offer {
            mime_type: mime_type.to_owned(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn fingerprint_is_independent_of_target_order() {
        let left = Snapshot::new(vec![
            offer("text/html", b"html"),
            offer("UTF8_STRING", b"text"),
        ]);
        let right = Snapshot::new(vec![
            offer("UTF8_STRING", b"text"),
            offer("text/html", b"html"),
        ]);

        assert_eq!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn fingerprint_includes_mime_type_and_data() {
        let original = Snapshot::new(vec![offer("text/html", b"same")]);
        let other_mime = Snapshot::new(vec![offer("UTF8_STRING", b"same")]);
        let other_data = Snapshot::new(vec![offer("text/html", b"different")]);

        assert_ne!(original.fingerprint(), other_mime.fingerprint());
        assert_ne!(original.fingerprint(), other_data.fingerprint());
    }

    #[test]
    fn wayland_fingerprint_includes_missing_text_aliases() {
        let source = Snapshot::new(vec![offer("UTF8_STRING", b"text")]);
        let expanded = Snapshot::new(
            super::PLAIN_TEXT_MIME_TYPES
                .into_iter()
                .map(|mime_type| offer(mime_type, b"text"))
                .collect(),
        );

        assert_eq!(source.wayland_fingerprint(), expanded.fingerprint());
    }
}
