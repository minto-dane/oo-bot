use std::path::Path;

use discord_oo_bot::{
    app::analyze_message::BotConfig,
    config::{
        canonical_bot_config, canonical_detector_policy, canonical_startup_config,
        ensure_startup_config_exists, load_startup_config_from_path,
        render_canonical_sample_config_yaml, StartupConfig,
    },
    domain::detector::DetectorPolicy,
};
use tempfile::tempdir;

#[test]
fn bot_and_detector_defaults_delegate_to_config_canonical_source() {
    assert_eq!(BotConfig::default(), canonical_bot_config());
    assert_eq!(DetectorPolicy::default(), canonical_detector_policy());
}

#[test]
fn canonical_renderer_roundtrips_to_canonical_struct() {
    let rendered = render_canonical_sample_config_yaml().expect("render canonical yaml");
    let parsed: StartupConfig = serde_yaml::from_str(&rendered).expect("parse rendered yaml");

    let expected = canonical_startup_config();
    let parsed_json = serde_json::to_value(parsed).expect("json value parsed");
    let expected_json = serde_json::to_value(expected).expect("json value expected");
    assert_eq!(parsed_json, expected_json);
}

#[test]
fn checked_in_sample_config_matches_canonical_defaults() {
    let loaded = load_startup_config_from_path(Path::new("config/oo-bot.yaml"))
        .expect("load sample config");
    let expected = canonical_startup_config();

    let loaded_json = serde_json::to_value(loaded.app).expect("json value loaded");
    let expected_json = serde_json::to_value(expected).expect("json value expected");
    assert_eq!(loaded_json, expected_json);
}

#[test]
fn ensure_startup_config_exists_bootstraps_yaml_file() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("oo-bot.yaml");

    let created = ensure_startup_config_exists(&path).expect("bootstrap config");
    assert!(created, "expected config bootstrap");

    let loaded = load_startup_config_from_path(&path).expect("load bootstrapped config");
    let expected = canonical_startup_config();

    let loaded_json = serde_json::to_value(loaded.app).expect("json value loaded");
    let expected_json = serde_json::to_value(expected).expect("json value expected");
    assert_eq!(loaded_json, expected_json);
}
