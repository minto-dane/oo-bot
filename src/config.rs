use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use hmac::{Hmac, Mac};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::Builder;
use thiserror::Error;
use tracing::warn;

use crate::app::analyze_message::{ActionPolicy, BotConfig, ReactionConfig};
use crate::domain::detector::{DetectorBackendKind, DetectorPolicy};
use crate::security::core_governor::RuntimeProtectionConfig;

pub const DEFAULT_CONFIG_PATH: &str = "config/oo-bot.yaml";
pub const CONFIG_SCHEMA_VERSION: &str = "oo-bot.strict-v1";

const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_DETECTOR_ITEMS: usize = 64;
const MAX_SPECIAL_PHRASES: usize = 64;
const MAX_PATTERN_LEN: usize = 32;
const MAX_READING_LEN: usize = 32;
const MAX_TEMPLATE_LEN: usize = 512;
const MAX_STAMP_LEN: usize = 128;
const MAX_EMOJI_NAME_LEN: usize = 64;

const EMBEDDED_DEFAULT_CONFIG_YAML: &str = include_str!("../config/oo-bot.yaml");

#[derive(Debug, Error, Clone)]
pub enum StartupConfigError {
    #[error("failed to read config: {0}")]
    ReadConfig(String),
    #[error("failed to create config directory: {0}")]
    CreateConfigDir(String),
    #[error("failed to write config: {0}")]
    WriteConfig(String),
    #[error("config too large")]
    TooLarge,
    #[error("failed to parse config: {0}")]
    ParseConfig(String),
    #[error("invalid config: {0}")]
    Validate(String),
    #[error("failed to verify config signature: {0}")]
    Signature(String),
    #[error("embedded default config is invalid: {0}")]
    EmbeddedDefault(String),
}

#[derive(Debug, Clone)]
pub struct LoadedStartupConfig {
    pub app: StartupConfig,
    pub config_path: PathBuf,
    pub config_fingerprint: String,
    pub pseudo_id_hmac_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupConfig {
    #[serde(default)]
    pub detector: DetectorConfig,
    #[serde(default)]
    pub bot: BotPolicyConfig,
    #[serde(default)]
    pub runtime: RuntimeProtectionConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    #[serde(default)]
    pub integrity: IntegrityConfig,
}

impl Default for StartupConfig {
    fn default() -> Self {
        canonical_startup_config()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectorConfig {
    #[serde(default = "default_detector_backend")]
    pub backend: DetectorBackendKind,
    #[serde(default = "default_target_readings")]
    pub target_readings: Vec<String>,
    #[serde(default = "default_literal_sequence_patterns")]
    pub literal_sequence_patterns: Vec<String>,
    #[serde(default = "default_special_phrases")]
    pub special_phrases: Vec<String>,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        canonical_detector_config()
    }
}

impl DetectorConfig {
    #[must_use]
    pub fn as_policy(&self) -> DetectorPolicy {
        DetectorPolicy {
            target_readings: self.target_readings.clone(),
            literal_sequence_patterns: self.literal_sequence_patterns.clone(),
            special_phrases: self.special_phrases.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotPolicyConfig {
    #[serde(default = "default_stamp_text")]
    pub stamp_text: String,
    #[serde(default = "default_send_template")]
    pub send_template: String,
    #[serde(default = "default_reaction_config")]
    pub reaction: ReactionConfig,
    #[serde(default = "default_max_count_cap")]
    pub max_count_cap: usize,
    #[serde(default = "default_max_send_chars")]
    pub max_send_chars: usize,
    #[serde(default = "default_action_policy")]
    pub action_policy: ActionPolicy,
}

impl Default for BotPolicyConfig {
    fn default() -> Self {
        canonical_bot_policy_config()
    }
}

impl BotPolicyConfig {
    #[must_use]
    pub fn as_bot_config(&self, detector: &DetectorConfig) -> BotConfig {
        BotConfig {
            special_phrase: detector.special_phrases.first().cloned().unwrap_or_default(),
            stamp_text: self.stamp_text.clone(),
            reaction: self.reaction.clone(),
            max_count_cap: self.max_count_cap,
            max_send_chars: self.max_send_chars,
            send_template: self.send_template.clone(),
            action_policy: self.action_policy.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    #[serde(default = "default_audit_path")]
    pub sqlite_path: PathBuf,
    #[serde(default = "default_audit_export_max_rows")]
    pub export_max_rows: usize,
    #[serde(default = "default_audit_query_max_rows")]
    pub query_max_rows: usize,
    #[serde(default = "default_audit_busy_timeout_ms")]
    pub busy_timeout_ms: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        canonical_audit_config()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrityConfig {
    #[serde(default = "default_config_signature")]
    pub config_signature: Option<ConfigSignatureConfig>,
    #[serde(default = "default_pseudo_id_hmac_key_env")]
    pub pseudo_id_hmac_key_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencySecurityCheckMode {
    Disabled,
    OfflineSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsConfig {
    #[serde(default = "default_local_self_check_on_startup")]
    pub local_self_check_on_startup: bool,
    #[serde(default = "default_verify_hardening_artifacts")]
    pub verify_hardening_artifacts: bool,
    #[serde(default = "default_verify_generated_artifacts")]
    pub verify_generated_artifacts: bool,
    #[serde(default = "default_audit_verify_rows")]
    pub audit_verify_max_rows: usize,
    #[serde(default = "default_security_snapshot_path")]
    pub security_snapshot_path: PathBuf,
    #[serde(default = "default_dependency_security_check_mode")]
    pub dependency_security_check_mode: DependencySecurityCheckMode,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        canonical_diagnostics_config()
    }
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        canonical_integrity_config()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSignatureConfig {
    pub detached_hmac_sha256_path: PathBuf,
    pub hmac_key_env: String,
}

fn embedded_default_startup_config() -> Result<&'static StartupConfig, StartupConfigError> {
    static EMBEDDED: OnceLock<Result<StartupConfig, StartupConfigError>> = OnceLock::new();

    EMBEDDED
        .get_or_init(|| {
            let cfg: StartupConfig = serde_yaml::from_str(EMBEDDED_DEFAULT_CONFIG_YAML)
                .map_err(|err| StartupConfigError::EmbeddedDefault(err.to_string()))?;
            validate_config(&cfg)?;
            Ok(cfg)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn embedded_default_yaml() -> Result<&'static serde_yaml::Value, StartupConfigError> {
    static EMBEDDED: OnceLock<Result<serde_yaml::Value, StartupConfigError>> = OnceLock::new();

    EMBEDDED
        .get_or_init(|| {
            serde_yaml::from_str(EMBEDDED_DEFAULT_CONFIG_YAML)
                .map_err(|err| StartupConfigError::EmbeddedDefault(err.to_string()))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn embedded_yaml_lookup(path: &[&str]) -> Result<&'static serde_yaml::Value, StartupConfigError> {
    let mut current = embedded_default_yaml()?;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| StartupConfigError::EmbeddedDefault(format!("missing key: {}", path.join("."))))?;
    }
    Ok(current)
}

fn embedded_default_value<T: DeserializeOwned>(path: &[&str]) -> Result<T, StartupConfigError> {
    let value = embedded_yaml_lookup(path)?.clone();
    serde_yaml::from_value(value).map_err(|err| StartupConfigError::EmbeddedDefault(err.to_string()))
}

pub fn startup_config_path() -> PathBuf {
    std::env::var("OO_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn default_detector_backend() -> DetectorBackendKind {
    embedded_default_value(&["detector", "backend"]).expect("embedded default config")
}

fn default_stamp_text() -> String {
    embedded_default_value(&["bot", "stamp_text"]).expect("embedded default config")
}

fn default_send_template() -> String {
    embedded_default_value(&["bot", "send_template"]).expect("embedded default config")
}

fn default_max_count_cap() -> usize {
    embedded_default_value(&["bot", "max_count_cap"]).expect("embedded default config")
}

fn default_max_send_chars() -> usize {
    embedded_default_value(&["bot", "max_send_chars"]).expect("embedded default config")
}

fn default_audit_path() -> PathBuf {
    embedded_default_value(&["audit", "sqlite_path"]).expect("embedded default config")
}

fn default_local_self_check_on_startup() -> bool {
    embedded_default_value(&["diagnostics", "local_self_check_on_startup"])
        .expect("embedded default config")
}

fn default_verify_hardening_artifacts() -> bool {
    embedded_default_value(&["diagnostics", "verify_hardening_artifacts"])
        .expect("embedded default config")
}

fn default_verify_generated_artifacts() -> bool {
    embedded_default_value(&["diagnostics", "verify_generated_artifacts"])
        .expect("embedded default config")
}

fn default_config_signature() -> Option<ConfigSignatureConfig> {
    embedded_default_value(&["integrity", "config_signature"]).expect("embedded default config")
}

fn default_pseudo_id_hmac_key_env() -> Option<String> {
    embedded_default_value(&["integrity", "pseudo_id_hmac_key_env"]).expect("embedded default config")
}

fn default_audit_verify_rows() -> usize {
    embedded_default_value(&["diagnostics", "audit_verify_max_rows"]).expect("embedded default config")
}

fn default_security_snapshot_path() -> PathBuf {
    embedded_default_value(&["diagnostics", "security_snapshot_path"]).expect("embedded default config")
}

fn default_dependency_security_check_mode() -> DependencySecurityCheckMode {
    embedded_default_value(&["diagnostics", "dependency_security_check_mode"]).expect("embedded default config")
}

pub fn load_startup_config() -> Result<LoadedStartupConfig, StartupConfigError> {
    load_startup_config_from_path(&startup_config_path())
}

pub fn canonical_startup_config() -> StartupConfig {
    embedded_default_startup_config()
        .expect("embedded default config")
        .clone()
}

pub fn render_canonical_sample_config_yaml() -> Result<String, StartupConfigError> {
    render_startup_config_yaml(
        embedded_default_startup_config()?
    )
}

pub fn render_startup_config_yaml(config: &StartupConfig) -> Result<String, StartupConfigError> {
    validate_config(config)?;

    let mut rendered = serde_yaml::to_string(config)
        .map_err(|err| StartupConfigError::Validate(err.to_string()))?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

pub fn validate_startup_config(config: &StartupConfig) -> Result<(), StartupConfigError> {
    validate_config(config)
}

pub fn ensure_startup_config_exists(path: &Path) -> Result<bool, StartupConfigError> {
    if path.exists() {
        return Ok(false);
    }

    write_startup_config_to_path(path, &canonical_startup_config())?;
    Ok(true)
}

pub fn write_startup_config_to_path(
    path: &Path,
    config: &StartupConfig,
) -> Result<(), StartupConfigError> {
    let rendered = render_startup_config_yaml(config)?;
    let signature_hex = config_signature_hex(path, config, rendered.as_bytes())?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| StartupConfigError::CreateConfigDir(err.to_string()))?;
    }

    let config_parent = path.parent().unwrap_or_else(|| Path::new("."));
    let config_file_name = path.file_name().ok_or_else(|| {
        StartupConfigError::WriteConfig("config path must include file name".to_string())
    })?;

    let temp_dir = Builder::new()
        .prefix(".oo-bot-config-")
        .tempdir_in(config_parent)
        .map_err(|err| StartupConfigError::CreateConfigDir(err.to_string()))?;

    let temp_config_path = temp_dir.path().join(config_file_name);
    write_file_synced(&temp_config_path, rendered.as_bytes())
        .map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;

    let mut signature_rename: Option<(PathBuf, PathBuf)> = None;
    if let Some((signature_path, hex)) = signature_hex {
        if let Some(parent) = signature_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| StartupConfigError::CreateConfigDir(err.to_string()))?;
        }

        let signature_parent = signature_path.parent().unwrap_or_else(|| Path::new("."));
        if signature_parent != config_parent {
            return Err(StartupConfigError::WriteConfig(
                "signature path must share config parent directory for atomic install".to_string(),
            ));
        }

        let signature_file_name = signature_path.file_name().ok_or_else(|| {
            StartupConfigError::WriteConfig("signature path must include file name".to_string())
        })?;
        let temp_signature_path = temp_dir.path().join(signature_file_name);
        write_file_synced(&temp_signature_path, format!("{hex}\n").as_bytes())
            .map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;
        signature_rename = Some((temp_signature_path, signature_path));
    }

    fsync_directory(temp_dir.path())
        .map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;

    fs::rename(&temp_config_path, path).map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;

    if let Some((temp_signature_path, signature_path)) = signature_rename {
        fs::rename(&temp_signature_path, &signature_path)
            .map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;
    }

    fsync_directory(config_parent)
        .map_err(|err| StartupConfigError::WriteConfig(err.to_string()))?;

    Ok(())
}

pub fn load_startup_config_from_path(path: &Path) -> Result<LoadedStartupConfig, StartupConfigError> {
    let bytes = fs::read(path).map_err(|err| StartupConfigError::ReadConfig(err.to_string()))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(StartupConfigError::TooLarge);
    }

    let cfg: StartupConfig =
        serde_yaml::from_slice(&bytes).map_err(|err| StartupConfigError::ParseConfig(err.to_string()))?;

    validate_config(&cfg)?;

    if let Some(signature) = &cfg.integrity.config_signature {
        verify_detached_hmac(path, &bytes, signature)?;
    } else {
        warn!("config signature is not configured; proceeding in unsigned mode");
    }

    let pseudo_key = resolve_optional_hmac_key(cfg.integrity.pseudo_id_hmac_key_env.as_deref());

    let config_fingerprint = fingerprint_config(&cfg)?;

    Ok(LoadedStartupConfig {
        app: cfg,
        config_path: path.to_path_buf(),
        config_fingerprint,
        pseudo_id_hmac_key: pseudo_key,
    })
}

fn validate_config(cfg: &StartupConfig) -> Result<(), StartupConfigError> {
    if cfg.detector.target_readings.len() > MAX_DETECTOR_ITEMS {
        return Err(StartupConfigError::Validate("target_readings exceeds max entries".into()));
    }
    if cfg.detector.literal_sequence_patterns.len() > MAX_DETECTOR_ITEMS {
        return Err(StartupConfigError::Validate(
            "literal_sequence_patterns exceeds max entries".into(),
        ));
    }
    if cfg.detector.special_phrases.len() > MAX_SPECIAL_PHRASES {
        return Err(StartupConfigError::Validate("special_phrases exceeds max entries".into()));
    }

    if cfg.detector.special_phrases.is_empty() {
        return Err(StartupConfigError::Validate(
            "special_phrases must not be empty".into(),
        ));
    }

    match cfg.detector.backend {
        DetectorBackendKind::MorphologicalReading => {
            if cfg.detector.target_readings.is_empty() {
                return Err(StartupConfigError::Validate(
                    "target_readings must not be empty for morphological_reading backend".into(),
                ));
            }
        }
        DetectorBackendKind::Fallback => {
            return Err(StartupConfigError::Validate(
                "detector.backend=fallback is internal-only and cannot be configured".into(),
            ));
        }
    }

    if cfg.detector.target_readings.iter().any(|entry| entry.len() > MAX_READING_LEN) {
        return Err(StartupConfigError::Validate("target_readings entry too long".into()));
    }
    if cfg
        .detector
        .literal_sequence_patterns
        .iter()
        .any(|entry| entry.len() > MAX_PATTERN_LEN)
    {
        return Err(StartupConfigError::Validate(
            "literal_sequence_patterns entry too long".into(),
        ));
    }

    if cfg.bot.stamp_text.is_empty() || cfg.bot.stamp_text.len() > MAX_STAMP_LEN {
        return Err(StartupConfigError::Validate("stamp_text length is invalid".into()));
    }
    if cfg.bot.send_template.is_empty() || cfg.bot.send_template.len() > MAX_TEMPLATE_LEN {
        return Err(StartupConfigError::Validate("send_template length is invalid".into()));
    }
    if cfg.bot.reaction.emoji_name.is_empty() || cfg.bot.reaction.emoji_name.len() > MAX_EMOJI_NAME_LEN {
        return Err(StartupConfigError::Validate("reaction.emoji_name length is invalid".into()));
    }
    if cfg.bot.reaction.emoji_id == 0 {
        return Err(StartupConfigError::Validate("reaction.emoji_id must be non-zero".into()));
    }
    if cfg.bot.max_count_cap == 0 || cfg.bot.max_count_cap > 4_096 {
        return Err(StartupConfigError::Validate("max_count_cap must be 1..=4096".into()));
    }
    if cfg.bot.max_send_chars == 0 || cfg.bot.max_send_chars > 8_000 {
        return Err(StartupConfigError::Validate("max_send_chars must be 1..=8000".into()));
    }

    if cfg.audit.export_max_rows == 0 || cfg.audit.export_max_rows > 1_000_000 {
        return Err(StartupConfigError::Validate("audit.export_max_rows is invalid".into()));
    }
    if cfg.audit.query_max_rows == 0 || cfg.audit.query_max_rows > 1_000_000 {
        return Err(StartupConfigError::Validate("audit.query_max_rows is invalid".into()));
    }

    if cfg.diagnostics.audit_verify_max_rows == 0 || cfg.diagnostics.audit_verify_max_rows > 100_000 {
        return Err(StartupConfigError::Validate(
            "diagnostics.audit_verify_max_rows is invalid".into(),
        ));
    }

    validate_template(&cfg.bot.send_template).map_err(StartupConfigError::Validate)?;

    Ok(())
}

fn validate_template(template: &str) -> Result<(), String> {
    let mut cursor = 0usize;
    while let Some(start) = template[cursor..].find("${") {
        let marker_start = cursor + start;
        let after_open = marker_start + 2;
        let Some(end_rel) = template[after_open..].find('}') else {
            return Err("template placeholder is not closed".into());
        };
        let end = after_open + end_rel;
        let key = &template[after_open..end];
        if !is_allowed_placeholder(key) {
            return Err(format!("template contains undefined placeholder: {key}"));
        }
        cursor = end + 1;
    }

    Ok(())
}

fn is_allowed_placeholder(value: &str) -> bool {
    matches!(
        value,
        "count" | "stamp" | "matched_backend" | "matched_reading" | "action_kind"
    )
}

pub fn default_target_readings() -> Vec<String> {
    embedded_default_value(&["detector", "target_readings"]).expect("embedded default config")
}

pub fn default_literal_sequence_patterns() -> Vec<String> {
    embedded_default_value(&["detector", "literal_sequence_patterns"]).expect("embedded default config")
}

pub fn default_special_phrases() -> Vec<String> {
    embedded_default_value(&["detector", "special_phrases"]).expect("embedded default config")
}

pub fn canonical_detector_policy() -> DetectorPolicy {
    DetectorPolicy {
        target_readings: default_target_readings(),
        literal_sequence_patterns: default_literal_sequence_patterns(),
        special_phrases: default_special_phrases(),
    }
}

pub fn canonical_detector_config() -> DetectorConfig {
    embedded_default_value(&["detector"]).expect("embedded default config")
}

pub fn default_action_policy() -> ActionPolicy {
    embedded_default_value(&["bot", "action_policy"]).expect("embedded default config")
}

pub fn default_reaction_config() -> ReactionConfig {
    embedded_default_value(&["bot", "reaction"]).expect("embedded default config")
}

pub fn canonical_reaction_config() -> ReactionConfig {
    default_reaction_config()
}

pub fn canonical_bot_policy_config() -> BotPolicyConfig {
    BotPolicyConfig {
        stamp_text: default_stamp_text(),
        send_template: default_send_template(),
        reaction: canonical_reaction_config(),
        max_count_cap: default_max_count_cap(),
        max_send_chars: default_max_send_chars(),
        action_policy: default_action_policy(),
    }
}

pub fn canonical_bot_config() -> BotConfig {
    BotConfig {
        special_phrase: default_special_phrases().first().cloned().unwrap_or_default(),
        stamp_text: default_stamp_text(),
        reaction: canonical_reaction_config(),
        max_count_cap: default_max_count_cap(),
        max_send_chars: default_max_send_chars(),
        send_template: default_send_template(),
        action_policy: default_action_policy(),
    }
}

fn default_audit_export_max_rows() -> usize {
    embedded_default_value(&["audit", "export_max_rows"]).expect("embedded default config")
}

fn default_audit_query_max_rows() -> usize {
    embedded_default_value(&["audit", "query_max_rows"]).expect("embedded default config")
}

fn default_audit_busy_timeout_ms() -> u64 {
    embedded_default_value(&["audit", "busy_timeout_ms"]).expect("embedded default config")
}

pub fn canonical_audit_config() -> AuditConfig {
    embedded_default_value(&["audit"]).expect("embedded default config")
}

pub fn canonical_integrity_config() -> IntegrityConfig {
    embedded_default_value(&["integrity"]).expect("embedded default config")
}

pub fn canonical_diagnostics_config() -> DiagnosticsConfig {
    embedded_default_value(&["diagnostics"]).expect("embedded default config")
}

pub fn default_runtime_protection_config() -> RuntimeProtectionConfig {
    RuntimeProtectionConfig {
        duplicate_ttl_ms: embedded_default_value(&["runtime", "duplicate_ttl_ms"])
            .expect("embedded default config"),
        duplicate_cache_cap: embedded_default_value(&["runtime", "duplicate_cache_cap"])
            .expect("embedded default config"),
        per_user_cooldown_ms: embedded_default_value(&["runtime", "per_user_cooldown_ms"])
            .expect("embedded default config"),
        per_channel_cooldown_ms: embedded_default_value(&["runtime", "per_channel_cooldown_ms"])
            .expect("embedded default config"),
        per_guild_cooldown_ms: embedded_default_value(&["runtime", "per_guild_cooldown_ms"])
            .expect("embedded default config"),
        global_cooldown_ms: embedded_default_value(&["runtime", "global_cooldown_ms"])
            .expect("embedded default config"),
        global_rate_per_sec: embedded_default_value(&["runtime", "global_rate_per_sec"])
            .expect("embedded default config"),
        global_rate_burst: embedded_default_value(&["runtime", "global_rate_burst"])
            .expect("embedded default config"),
        max_actions_per_message: embedded_default_value(&["runtime", "max_actions_per_message"])
            .expect("embedded default config"),
        max_send_chars: embedded_default_value(&["runtime", "max_send_chars"])
            .expect("embedded default config"),
        long_message_soft_chars: embedded_default_value(&["runtime", "long_message_soft_chars"])
            .expect("embedded default config"),
        long_message_hard_chars: embedded_default_value(&["runtime", "long_message_hard_chars"])
            .expect("embedded default config"),
        suspicious_repetition_threshold: embedded_default_value(&[
            "runtime",
            "suspicious_repetition_threshold",
        ])
        .expect("embedded default config"),
        breaker_window_ms: embedded_default_value(&["runtime", "breaker_window_ms"])
            .expect("embedded default config"),
        breaker_threshold: embedded_default_value(&["runtime", "breaker_threshold"])
            .expect("embedded default config"),
        breaker_open_ms: embedded_default_value(&["runtime", "breaker_open_ms"])
            .expect("embedded default config"),
        sandbox_failure_window_ms: embedded_default_value(&["runtime", "sandbox_failure_window_ms"])
            .expect("embedded default config"),
        sandbox_failure_threshold: embedded_default_value(&["runtime", "sandbox_failure_threshold"])
            .expect("embedded default config"),
        allow_guild_ids: embedded_default_value(&["runtime", "allow_guild_ids"])
            .expect("embedded default config"),
        deny_guild_ids: embedded_default_value(&["runtime", "deny_guild_ids"])
            .expect("embedded default config"),
        allow_channel_ids: embedded_default_value(&["runtime", "allow_channel_ids"])
            .expect("embedded default config"),
        deny_channel_ids: embedded_default_value(&["runtime", "deny_channel_ids"])
            .expect("embedded default config"),
        mode_override: embedded_default_value(&["runtime", "mode_override"])
            .expect("embedded default config"),
        emergency_kill_switch: embedded_default_value(&["runtime", "emergency_kill_switch"])
            .expect("embedded default config"),
        session_budget_low_watermark: embedded_default_value(&[
            "runtime",
            "session_budget_low_watermark",
        ])
        .expect("embedded default config"),
    }
}

fn verify_detached_hmac(
    config_path: &Path,
    config_bytes: &[u8],
    signature: &ConfigSignatureConfig,
) -> Result<(), StartupConfigError> {
    let signature_path = resolve_config_relative_path(config_path, &signature.detached_hmac_sha256_path);
    let signature_hex = fs::read_to_string(&signature_path)
        .map_err(|err| StartupConfigError::Signature(err.to_string()))?;
    let supplied = hex::decode(signature_hex.trim())
        .map_err(|err| StartupConfigError::Signature(err.to_string()))?;

    let mut mac = build_hmac_from_env(&signature.hmac_key_env)?;
    mac.update(config_bytes);
    mac.verify_slice(&supplied).map_err(|_| {
        StartupConfigError::Signature("detached config signature mismatch".into())
    })?;

    Ok(())
}

fn config_signature_hex(
    config_path: &Path,
    config: &StartupConfig,
    config_bytes: &[u8],
) -> Result<Option<(PathBuf, String)>, StartupConfigError> {
    let Some(signature) = &config.integrity.config_signature else {
        return Ok(None);
    };

    let digest = compute_signature_digest(config_bytes, &signature.hmac_key_env)?;
    Ok(Some((
        resolve_config_relative_path(config_path, &signature.detached_hmac_sha256_path),
        hex::encode(digest),
    )))
}

fn compute_signature_digest(
    config_bytes: &[u8],
    hmac_key_env: &str,
) -> Result<Vec<u8>, StartupConfigError> {
    let mut mac = build_hmac_from_env(hmac_key_env)?;
    mac.update(config_bytes);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn build_hmac_from_env(hmac_key_env: &str) -> Result<Hmac<Sha256>, StartupConfigError> {
    let key_value = std::env::var(hmac_key_env)
        .map_err(|_| StartupConfigError::Signature("missing signature HMAC key env var".into()))?;

    Hmac::<Sha256>::new_from_slice(key_value.as_bytes())
        .map_err(|err| StartupConfigError::Signature(err.to_string()))
}

fn resolve_config_relative_path(config_path: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        return target.to_path_buf();
    }

    config_path
        .parent()
        .map(|parent| parent.join(target))
        .unwrap_or_else(|| target.to_path_buf())
}

fn write_file_synced(path: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content)?;
    file.sync_all()
}

fn fsync_directory(path: &Path) -> Result<(), std::io::Error> {
    let dir = fs::File::open(path)?;
    dir.sync_all()
}

fn resolve_optional_hmac_key(env_name: Option<&str>) -> Option<Vec<u8>> {
    let Some(env_name) = env_name else {
        warn!("pseudo-id hmac key is not configured; pseudo identifiers will be disabled");
        return None;
    };

    match std::env::var(env_name) {
        Ok(value) if !value.trim().is_empty() => Some(value.into_bytes()),
        _ => {
            warn!(key_env = env_name, "pseudo-id hmac key is not set; pseudo identifiers will be disabled");
            None
        }
    }
}

fn fingerprint_config(cfg: &StartupConfig) -> Result<String, StartupConfigError> {
    let mut fingerprint_src = cfg.clone();
    if let Some(signature) = fingerprint_src.integrity.config_signature.as_mut() {
        signature.hmac_key_env = "<redacted>".to_string();
    }
    if let Some(env_name) = fingerprint_src.integrity.pseudo_id_hmac_key_env.as_mut() {
        *env_name = "<redacted>".to_string();
    }

    let encoded = serde_json::to_vec(&fingerprint_src)
        .map_err(|err| StartupConfigError::Validate(err.to_string()))?;
    let digest = Sha256::digest(&encoded);
    Ok(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{load_startup_config_from_path, validate_template, StartupConfig};

    #[test]
    fn template_rejects_unknown_placeholder() {
        assert!(validate_template("${unknown}").is_err());
    }

    #[test]
    fn config_rejects_unknown_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("cfg.yaml");

        let mut file = std::fs::File::create(&file_path).expect("create config file");
        writeln!(
            file,
            "detector:\n  backend: morphological_reading\n  target_readings: [\"おお\"]\n  literal_sequence_patterns: [\"oo\"]\n  special_phrases: [\"これはおお\"]\nunknown_field: true"
        )
        .expect("write");

        assert!(load_startup_config_from_path(&file_path).is_err());
    }

    #[test]
    fn default_config_is_valid() {
        let cfg = StartupConfig::default();
        let serialized = serde_yaml::to_string(&cfg).expect("serialize default config");
        assert!(!serialized.is_empty());
    }
}
