use discord_oo_bot::{
    config::{load_startup_config_from_path, write_startup_config_to_path, StartupConfig},
    domain::detector::build_detector,
};
use tempfile::tempdir;

#[test]
fn yaml_target_readings_can_customize_kanji_reading_match() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");

    let mut cfg = StartupConfig::default();
    cfg.detector.target_readings = vec!["おおき".to_string()];
    cfg.detector.literal_sequence_patterns = vec![];

    write_startup_config_to_path(&path, &cfg).expect("write yaml config");
    let loaded = load_startup_config_from_path(&path).expect("load yaml config");

    let detector = build_detector(loaded.app.detector.backend, loaded.app.detector.as_policy())
        .expect("build detector from config");

    let kanji_word = detector.detect("大きい");
    assert!(
        kanji_word.kanji_hits >= 1,
        "expected kanji reading hit from custom target_readings, got: {kanji_word:?}"
    );

    let plain_kana = detector.detect("おお");
    assert_eq!(
        plain_kana.total_count, 0,
        "unexpected hit for non-matching kana content: {plain_kana:?}"
    );
}
