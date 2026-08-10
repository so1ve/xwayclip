use blake3::{Hash, Hasher};

pub struct Offer {
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub struct Snapshot {
    offers: Vec<Offer>,
}

impl Snapshot {
    pub fn new(mut offers: Vec<Offer>) -> Self {
        offers.sort_unstable_by(|left, right| left.mime_type.cmp(&right.mime_type));
        offers.dedup_by(|left, right| left.mime_type == right.mime_type);
        Self { offers }
    }

    pub fn fingerprint(&self) -> Hash {
        let mut hasher = Hasher::new();

        for offer in &self.offers {
            hasher.update(&offer.mime_type.len().to_le_bytes());
            hasher.update(offer.mime_type.as_bytes());
            hasher.update(&offer.data.len().to_le_bytes());
            hasher.update(&offer.data);
        }

        hasher.finalize()
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
}
