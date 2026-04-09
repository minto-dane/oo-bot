use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    app::analyze_message::{analyze_message, BotAction, BotConfig},
    domain::kanji_matcher::KanjiOoDb,
    sandbox::{
        abi::{ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer},
        host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    },
    security::{
        core_governor::{MessageContext, RuntimeProtectionConfig, SuppressReason, TrustedCore},
        mode::RuntimeMode,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReplayCase {
    pub name: String,
    pub content: String,
    #[serde(default = "default_message_id")]
    pub message_id: u64,
    #[serde(default = "default_author_id")]
    pub author_id: u64,
    #[serde(default = "default_channel_id")]
    pub channel_id: u64,
    #[serde(default)]
    pub guild_id: Option<u64>,
    #[serde(default)]
    pub author_is_bot: bool,
    pub expected: ExpectedAction,
    #[serde(default)]
    pub expected_mode: Option<RuntimeMode>,
    #[serde(default)]
    pub expected_suppress_reason: Option<SuppressReason>,
    #[serde(default)]
    pub runtime: ReplayRuntimeOverrides,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct ReplayRuntimeOverrides {
    #[serde(default)]
    pub mode_override: Option<RuntimeMode>,
    #[serde(default)]
    pub emergency_kill_switch: bool,
    #[serde(default)]
    pub allow_guild_ids: Vec<u64>,
    #[serde(default)]
    pub deny_guild_ids: Vec<u64>,
    #[serde(default)]
    pub allow_channel_ids: Vec<u64>,
    #[serde(default)]
    pub deny_channel_ids: Vec<u64>,
    #[serde(default)]
    pub inject_statuses: Vec<u16>,
    #[serde(default)]
    pub soft_char_limit: Option<usize>,
    #[serde(default)]
    pub hard_char_limit: Option<usize>,
    #[serde(default)]
    pub repetition_threshold: Option<usize>,
    #[serde(default)]
    pub preserve_state: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExpectedAction {
    Noop,
    React { emoji_id: u64, emoji_name: String, animated: bool },
    SendMessage { content: String },
}

impl From<BotAction> for ExpectedAction {
    fn from(value: BotAction) -> Self {
        match value {
            BotAction::Noop => Self::Noop,
            BotAction::React { emoji_id, emoji_name, animated } => {
                Self::React { emoji_id, emoji_name, animated }
            }
            BotAction::SendMessage { content } => Self::SendMessage { content },
        }
    }
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("failed to read replay fixture: {0}")]
    ReadFixture(String),
    #[error("failed to parse replay fixture: {0}")]
    ParseFixture(String),
}

pub fn load_replay_cases(path: &Path) -> Result<Vec<ReplayCase>, ReplayError> {
    if path.is_file() {
        return load_replay_cases_from_file(path);
    }
    if path.is_dir() {
        let mut all = Vec::new();
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|e| ReplayError::ReadFixture(e.to_string()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();

        for entry_path in entries {
            if !entry_path.is_file() || !is_supported_fixture(&entry_path) {
                continue;
            }
            all.extend(load_replay_cases_from_file(&entry_path)?);
        }
        return Ok(all);
    }
    Err(ReplayError::ReadFixture(format!("path not found: {}", path.display())))
}

pub fn run_replay_case(
    case: &ReplayCase,
    config: &BotConfig,
    db: &KanjiOoDb,
) -> Result<(), String> {
    let actual = analyze_message(&case.content, case.author_is_bot, config, db);
    let expected = case.expected.clone();
    let actual_as_expected: ExpectedAction = actual.into();

    if actual_as_expected == expected {
        Ok(())
    } else {
        Err(format!("case={} expected={:?} actual={:?}", case.name, expected, actual_as_expected))
    }
}

pub fn run_replay_case_with_core(case: &ReplayCase, core: &mut TrustedCore) -> Result<(), String> {
    core.set_mode_override(case.runtime.mode_override);
    core.set_emergency_kill_switch(case.runtime.emergency_kill_switch);
    core.set_access_lists(
        case.runtime.allow_guild_ids.clone(),
        case.runtime.deny_guild_ids.clone(),
        case.runtime.allow_channel_ids.clone(),
        case.runtime.deny_channel_ids.clone(),
    );
    if let (Some(soft), Some(hard)) = (case.runtime.soft_char_limit, case.runtime.hard_char_limit) {
        let repetition = case.runtime.repetition_threshold.unwrap_or(256);
        core.set_suspicious_thresholds(soft, hard, repetition);
    } else {
        core.reset_suspicious_thresholds();
    }
    for status in &case.runtime.inject_statuses {
        core.record_http_status(*status);
    }

    let message_id =
        if case.message_id == 0 { stable_id_from_name(&case.name) } else { case.message_id };

    let message = MessageContext {
        message_id,
        author_id: case.author_id,
        channel_id: case.channel_id,
        guild_id: case.guild_id,
        author_is_bot: case.author_is_bot,
    };
    let decision = core.decide_message(message, &case.content);
    let actual_as_expected: ExpectedAction = decision.action.into();

    if actual_as_expected != case.expected {
        return Err(format!(
            "case={} expected={:?} actual={:?} mode={:?} proposal={:?} suspicion={:?} suppress_reason={:?}",
            case.name,
            case.expected,
            actual_as_expected,
            decision.mode,
            decision.proposal,
            decision.suspicion,
            decision.suppress_reason
        ));
    }

    if let Some(expected_mode) = case.expected_mode {
        if decision.mode != expected_mode {
            return Err(format!(
                "case={} expected_mode={:?} actual_mode={:?}",
                case.name, expected_mode, decision.mode
            ));
        }
    }

    if case.expected_suppress_reason != decision.suppress_reason {
        return Err(format!(
            "case={} expected_suppress_reason={:?} actual_suppress_reason={:?}",
            case.name, case.expected_suppress_reason, decision.suppress_reason
        ));
    }

    Ok(())
}

pub fn build_replay_core(config: BotConfig, db: &'static KanjiOoDb) -> Result<TrustedCore, String> {
    let analyzer = WasmtimeSandboxAnalyzer::new(SandboxConfig {
        fuel_limit: 800_000,
        ..SandboxConfig::default()
    })?;

    let runtime_cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        breaker_threshold: 2,
        // Keep baseline replay compatibility with pure analyzer behavior.
        // Runtime-sensitive fixtures override these thresholds explicitly.
        long_message_soft_chars: 20_000,
        long_message_hard_chars: 30_000,
        ..RuntimeProtectionConfig::default()
    };

    Ok(TrustedCore::new(
        Box::new(ReplayHarnessAnalyzer { inner: analyzer }),
        config,
        runtime_cfg,
        db,
    ))
}

fn load_replay_cases_from_file(path: &Path) -> Result<Vec<ReplayCase>, ReplayError> {
    let content = fs::read_to_string(path).map_err(|e| ReplayError::ReadFixture(e.to_string()))?;

    let ext = path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or_default();
    match ext {
        "yml" | "yaml" => parse_yaml_cases(&content),
        "json" => parse_json_cases(&content),
        _ => Err(ReplayError::ParseFixture(format!("unsupported extension: {}", path.display()))),
    }
}

fn parse_yaml_cases(content: &str) -> Result<Vec<ReplayCase>, ReplayError> {
    serde_yaml::from_str::<Vec<ReplayCase>>(content)
        .or_else(|_| serde_yaml::from_str::<ReplayCase>(content).map(|single| vec![single]))
        .map_err(|e| ReplayError::ParseFixture(e.to_string()))
}

fn parse_json_cases(content: &str) -> Result<Vec<ReplayCase>, ReplayError> {
    serde_json::from_str::<Vec<ReplayCase>>(content)
        .or_else(|_| serde_json::from_str::<ReplayCase>(content).map(|single| vec![single]))
        .map_err(|e| ReplayError::ParseFixture(e.to_string()))
}

fn is_supported_fixture(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| matches!(ext, "yaml" | "yml" | "json"))
}

fn default_message_id() -> u64 {
    0
}

fn default_author_id() -> u64 {
    100
}

fn default_channel_id() -> u64 {
    200
}

fn stable_id_from_name(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

struct ReplayHarnessAnalyzer {
    inner: WasmtimeSandboxAnalyzer,
}

impl ProposalAnalyzer for ReplayHarnessAnalyzer {
    fn abi_version(&self) -> u32 {
        self.inner.abi_version()
    }

    fn propose(&mut self, req: &AnalyzerRequest<'_>) -> Result<ActionProposal, AnalyzerError> {
        if req.content.contains("[[sandbox_trap]]") {
            return Err(AnalyzerError::Trap("injected trap".to_string()));
        }
        if req.content.contains("[[sandbox_timeout]]") {
            return Err(AnalyzerError::Timeout);
        }
        self.inner.propose(req)
    }
}
