#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KanjiOoDbMetadata {
    pub source_name: &'static str,
    pub source_sha256: &'static str,
    pub total_chars: usize,
    pub ja_kun_hits: usize,
    pub nanori_hits: usize,
    pub ja_on_hits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KanjiOoDb {
    codepoints: &'static [u32],
    metadata: KanjiOoDbMetadata,
}

impl KanjiOoDb {
    pub const fn new(codepoints: &'static [u32], metadata: KanjiOoDbMetadata) -> Self {
        Self { codepoints, metadata }
    }

    #[must_use]
    pub fn contains_char(&self, ch: char) -> bool {
        let target = ch as u32;
        self.codepoints.binary_search(&target).is_ok()
    }

    #[must_use]
    pub fn metadata(&self) -> KanjiOoDbMetadata {
        self.metadata
    }

    #[must_use]
    pub fn codepoints(&self) -> &'static [u32] {
        self.codepoints
    }
}

#[must_use]
pub fn kanji_has_oo_reading(ch: char, db: &KanjiOoDb) -> bool {
    db.contains_char(ch)
}

#[must_use]
pub fn count_oo_kanji(text: &str, db: &KanjiOoDb) -> usize {
    text.chars().filter(|&ch| db.contains_char(ch)).count()
}

#[cfg(test)]
mod tests {
    use super::{count_oo_kanji, kanji_has_oo_reading, KanjiOoDb, KanjiOoDbMetadata};

    const TEST_DB: KanjiOoDb = KanjiOoDb::new(
        &['大' as u32, '狼' as u32],
        KanjiOoDbMetadata {
            source_name: "test",
            source_sha256: "test",
            total_chars: 2,
            ja_kun_hits: 2,
            nanori_hits: 0,
            ja_on_hits: 0,
        },
    );

    #[test]
    fn finds_oo_kanji() {
        assert!(kanji_has_oo_reading('大', &TEST_DB));
        assert!(!kanji_has_oo_reading('小', &TEST_DB));
    }

    #[test]
    fn counts_per_character() {
        assert_eq!(count_oo_kanji("大小大狼", &TEST_DB), 3);
    }
}
