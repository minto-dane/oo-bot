use discord_ooh_bot::{
    app::analyze_message::{analyze_message, BotAction, BotConfig},
    generated::kanji_oo_db::KANJI_OO_DB,
};

fn stamp_count(content: &str) -> usize {
    content.split_whitespace().count()
}

#[test]
fn mixed_kana_romaji_kanji_counts_as_expected() {
    let cfg = BotConfig::default();
    let action = analyze_message("おおoo大", false, &cfg, &KANJI_OO_DB);

    match action {
        BotAction::SendMessage { content } => {
            assert_eq!(stamp_count(&content), 3);
        }
        other => panic!("expected send message, got: {other:?}"),
    }
}

#[test]
fn special_phrase_takes_priority() {
    let cfg = BotConfig::default();
    let action = analyze_message("これはおお oo 大", false, &cfg, &KANJI_OO_DB);

    assert_eq!(action, BotAction::SendMessage { content: cfg.stamp_text });
}

#[test]
fn single_hit_reacts() {
    let cfg = BotConfig::default();
    let action = analyze_message("オオ", false, &cfg, &KANJI_OO_DB);
    assert!(matches!(action, BotAction::React { .. }));
}
