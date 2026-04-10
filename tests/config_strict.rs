use std::io::Write;
use std::sync::{Mutex, OnceLock};

use discord_oo_bot::config::{
    load_startup_config_from_path, write_startup_config_to_path, StartupConfig,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tempfile::tempdir;

type HmacSha256 = Hmac<Sha256>;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn rejects_unknown_key() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "detector:\n  backend: morphological_reading\n  target_readings: [\"おお\"]\n  literal_sequence_patterns: [\"oo\"]\n  special_phrases: [\"これはおお\"]\nbot:\n  stamp_text: \"x\"\n  send_template: \"${stamp}\"\n  reaction:\n    emoji_id: 1\n    emoji_name: \"x\"\n    animated: false\n  max_count_cap: 1\n  max_send_chars: 10\nruntime: {}\naudit:\n  sqlite_path: \"state/audit.sqlite3\"\n  export_max_rows: 10\n  query_max_rows: 10\n  busy_timeout_ms: 100\nintegrity:\n  config_signature: null\n  pseudo_id_hmac_key_env: null\nunknown_key: true\n",
    )
    .expect("write");

    assert!(load_startup_config_from_path(&path).is_err());
}

#[test]
fn rejects_template_with_undefined_placeholder() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "detector:\n  backend: morphological_reading\n  target_readings: [\"おお\"]\n  literal_sequence_patterns: [\"oo\"]\n  special_phrases: [\"これはおお\"]\nbot:\n  stamp_text: \"x\"\n  send_template: \"${unknown}\"\n  reaction:\n    emoji_id: 1\n    emoji_name: \"x\"\n    animated: false\n  max_count_cap: 1\n  max_send_chars: 10\nruntime: {}\naudit:\n  sqlite_path: \"state/audit.sqlite3\"\n  export_max_rows: 10\n  query_max_rows: 10\n  busy_timeout_ms: 100\nintegrity:\n  config_signature: null\n  pseudo_id_hmac_key_env: null\n",
    )
    .expect("write");

    assert!(load_startup_config_from_path(&path).is_err());
}

#[test]
fn signature_verification_success_and_failure() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = tempdir().expect("tempdir");
    let cfg_path = dir.path().join("config.yaml");
    let sig_path = dir.path().join("config.sig");

    let cfg_body = format!(
        "detector:\n  backend: morphological_reading\n  target_readings: [\"おお\"]\n  literal_sequence_patterns: [\"oo\"]\n  special_phrases: [\"これはおお\"]\nbot:\n  stamp_text: \"x\"\n  send_template: \"${{stamp}}\"\n  reaction:\n    emoji_id: 1\n    emoji_name: \"x\"\n    animated: false\n  max_count_cap: 1\n  max_send_chars: 10\nruntime: {{}}\naudit:\n  sqlite_path: \"state/audit.sqlite3\"\n  export_max_rows: 10\n  query_max_rows: 10\n  busy_timeout_ms: 100\nintegrity:\n  config_signature:\n    detached_hmac_sha256_path: \"{}\"\n    hmac_key_env: \"OO_TEST_CFG_HMAC\"\n  pseudo_id_hmac_key_env: null\n",
        sig_path.display()
    );

    std::fs::write(&cfg_path, &cfg_body).expect("write cfg");

    let mut mac = HmacSha256::new_from_slice(b"k-test").expect("hmac init");
    mac.update(cfg_body.as_bytes());
    let digest = mac.finalize().into_bytes();

    let mut sig_file = std::fs::File::create(&sig_path).expect("create sig");
    writeln!(sig_file, "{}", hex::encode(digest)).expect("write sig");

    std::env::set_var("OO_TEST_CFG_HMAC", "k-test");
    let ok_result = load_startup_config_from_path(&cfg_path);
    assert!(ok_result.is_ok(), "expected success, got: {ok_result:?}");

    std::env::set_var("OO_TEST_CFG_HMAC", "wrong-key");
    let err_result = load_startup_config_from_path(&cfg_path);
    assert!(err_result.is_err(), "expected failure, got: {err_result:?}");

    std::env::remove_var("OO_TEST_CFG_HMAC");
}

#[test]
fn write_updates_relative_signature_file() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = tempdir().expect("tempdir");
    let cfg_path = dir.path().join("config.yaml");
    let sig_path = dir.path().join("config.sig");

    std::env::set_var("OO_TEST_CFG_HMAC", "k-test");

    let mut cfg = StartupConfig::default();
    cfg.integrity.config_signature = Some(discord_oo_bot::config::ConfigSignatureConfig {
        detached_hmac_sha256_path: "config.sig".into(),
        hmac_key_env: "OO_TEST_CFG_HMAC".to_string(),
    });
    cfg.bot.stamp_text = "stamp!".to_string();

    write_startup_config_to_path(&cfg_path, &cfg).expect("write config with signature");

    assert!(cfg_path.exists(), "config file should exist");
    assert!(sig_path.exists(), "signature file should exist");

    let loaded = load_startup_config_from_path(&cfg_path).expect("load signed config");
    assert_eq!(loaded.app.bot.stamp_text, "stamp!");

    std::env::remove_var("OO_TEST_CFG_HMAC");
}
