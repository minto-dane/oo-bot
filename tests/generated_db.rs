use discord_ooh_bot::{
    domain::kanji_matcher::kanji_has_oo_reading, generated::kanji_oo_db::KANJI_OO_DB,
};

#[test]
fn generated_db_is_sorted_unique_and_non_empty() {
    let cps = KANJI_OO_DB.codepoints();
    assert!(!cps.is_empty(), "generated DB should not be empty");
    assert!(cps.windows(2).all(|w| w[0] < w[1]), "codepoints must be sorted and unique");
}

#[test]
fn generated_db_contains_known_character() {
    assert!(kanji_has_oo_reading('大', &KANJI_OO_DB));
}
