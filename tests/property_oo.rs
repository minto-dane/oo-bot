use discord_oo_bot::{
    app::analyze_message::{analyze_message, BotConfig},
    domain::{oo_counter::count_oo_sequences, reading_normalizer::normalize_reading},
    generated::kanji_oo_db::KANJI_OO_DB,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn non_overlapping_ascii_o_invariant(bits in proptest::collection::vec(any::<bool>(), 0..400)) {
        let s: String = bits.into_iter().map(|b| if b { 'o' } else { 'O' }).collect();
        let count = count_oo_sequences(&s);
        prop_assert_eq!(count, s.chars().count() / 2);
    }

    #[test]
    fn count_is_bounded_by_half_of_chars(s in any::<String>()) {
        let count = count_oo_sequences(&s);
        prop_assert!(count <= s.chars().count() / 2);
    }

    #[test]
    fn normalizer_is_idempotent(s in any::<String>()) {
        let once = normalize_reading(&s);
        let twice = normalize_reading(&once);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn analyze_message_never_panics_on_random_unicode(s in any::<String>(), is_bot in any::<bool>()) {
        let cfg = BotConfig::default();
        let _ = analyze_message(&s, is_bot, &cfg, &KANJI_OO_DB);
    }
}
