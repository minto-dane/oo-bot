#![forbid(unsafe_code)]

use discord_oo_bot::{
    app::analyze_message::{BotConfig, ReactionConfig},
    generated::kanji_oo_db::KANJI_OO_DB,
    infra::discord_handler::Handler,
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::core_governor::{RuntimeProtectionConfig, TrustedCore},
};
use serenity::all::{Client, GatewayIntents};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Error)]
enum StartupError {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid environment variable: {0}")]
    InvalidEnv(&'static str),
    #[error("sandbox init failed: {0}")]
    SandboxInit(String),
    #[error("failed to create discord client: {0}")]
    ClientBuild(String),
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    init_tracing();
    dotenvy::dotenv().ok();

    let token =
        std::env::var("DISCORD_TOKEN").map_err(|_| StartupError::MissingEnv("DISCORD_TOKEN"))?;
    validate_discord_token(&token)?;

    let config = load_bot_config()?;
    let runtime_cfg = load_runtime_config()?;
    let sandbox_cfg = load_sandbox_config()?;

    let analyzer = WasmtimeSandboxAnalyzer::new(sandbox_cfg).map_err(StartupError::SandboxInit)?;
    let mut core = TrustedCore::new(Box::new(analyzer), config, runtime_cfg, &KANJI_OO_DB);

    let (budget_total, budget_remaining, budget_reset_after) = load_session_budget()?;
    core.update_session_budget(budget_total, budget_remaining, budget_reset_after);

    if core.session_budget_low() {
        info!(remaining = budget_remaining, "session budget low: starting in degraded posture");
    }

    let shared_core = Arc::new(Mutex::new(core));

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler { core: shared_core })
        .await
        .map_err(|err| StartupError::ClientBuild(err.to_string()))?;

    info!("starting bot");
    if let Err(err) = client.start().await {
        error!(error = %err, "discord client stopped with error");
    }
    Ok(())
}

fn load_runtime_config() -> Result<RuntimeProtectionConfig, StartupError> {
    let mut cfg = RuntimeProtectionConfig::default();
    cfg.duplicate_ttl_ms = read_env_u64("OO_DUPLICATE_TTL_MS", cfg.duplicate_ttl_ms)?;
    cfg.duplicate_cache_cap = read_env_usize("OO_DUPLICATE_CACHE_CAP", cfg.duplicate_cache_cap)?;
    cfg.per_user_cooldown_ms = read_env_u64("OO_COOLDOWN_USER_MS", cfg.per_user_cooldown_ms)?;
    cfg.per_channel_cooldown_ms =
        read_env_u64("OO_COOLDOWN_CHANNEL_MS", cfg.per_channel_cooldown_ms)?;
    cfg.per_guild_cooldown_ms = read_env_u64("OO_COOLDOWN_GUILD_MS", cfg.per_guild_cooldown_ms)?;
    cfg.global_cooldown_ms = read_env_u64("OO_COOLDOWN_GLOBAL_MS", cfg.global_cooldown_ms)?;
    cfg.global_rate_per_sec = read_env_f64("OO_GLOBAL_RATE_PER_SEC", cfg.global_rate_per_sec)?;
    cfg.global_rate_burst = read_env_u32("OO_GLOBAL_RATE_BURST", cfg.global_rate_burst)?;
    cfg.max_actions_per_message =
        read_env_u8("OO_MAX_ACTIONS_PER_MESSAGE", cfg.max_actions_per_message)?;
    cfg.max_send_chars = read_env_usize("OO_MAX_SEND_CHARS", cfg.max_send_chars)?;
    cfg.long_message_soft_chars =
        read_env_usize("OO_LONG_MESSAGE_SOFT_CHARS", cfg.long_message_soft_chars)?;
    cfg.long_message_hard_chars =
        read_env_usize("OO_LONG_MESSAGE_HARD_CHARS", cfg.long_message_hard_chars)?;
    cfg.suspicious_repetition_threshold =
        read_env_usize("OO_SUSPICIOUS_REPETITION_THRESHOLD", cfg.suspicious_repetition_threshold)?;
    cfg.breaker_window_ms = read_env_u64("OO_BREAKER_WINDOW_MS", cfg.breaker_window_ms)?;
    cfg.breaker_threshold = read_env_usize("OO_BREAKER_THRESHOLD", cfg.breaker_threshold)?;
    cfg.breaker_open_ms = read_env_u64("OO_BREAKER_OPEN_MS", cfg.breaker_open_ms)?;
    cfg.sandbox_failure_window_ms =
        read_env_u64("OO_SANDBOX_FAILURE_WINDOW_MS", cfg.sandbox_failure_window_ms)?;
    cfg.sandbox_failure_threshold =
        read_env_usize("OO_SANDBOX_FAILURE_THRESHOLD", cfg.sandbox_failure_threshold)?;
    cfg.session_budget_low_watermark =
        read_env_u32("OO_SESSION_BUDGET_LOW_WATERMARK", cfg.session_budget_low_watermark)?;
    cfg.emergency_kill_switch = read_env_bool("OO_EMERGENCY_KILL_SWITCH", false)?;
    cfg.allow_guild_ids = read_env_id_list("OO_ALLOW_GUILD_IDS")?;
    cfg.deny_guild_ids = read_env_id_list("OO_DENY_GUILD_IDS")?;
    cfg.allow_channel_ids = read_env_id_list("OO_ALLOW_CHANNEL_IDS")?;
    cfg.deny_channel_ids = read_env_id_list("OO_DENY_CHANNEL_IDS")?;
    cfg.mode_override = read_env_mode_override("OO_MODE_OVERRIDE")?;
    Ok(cfg)
}

fn load_sandbox_config() -> Result<SandboxConfig, StartupError> {
    Ok(SandboxConfig {
        fuel_limit: read_env_u64("OO_SANDBOX_FUEL_LIMIT", 50_000)?,
        memory_limit_bytes: read_env_usize("OO_SANDBOX_MEMORY_BYTES", 65_536)?,
        table_elements_limit: read_env_usize("OO_SANDBOX_TABLE_ELEMENTS", 64)?,
        store_instance_limit: read_env_usize("OO_SANDBOX_INSTANCE_LIMIT", 4)?,
    })
}

fn load_session_budget() -> Result<(u32, u32, u64), StartupError> {
    let budget_total = read_env_u32("OO_SESSION_BUDGET_TOTAL", 1000)?;
    let budget_remaining = read_env_u32("OO_SESSION_BUDGET_REMAINING", budget_total)?;
    let budget_reset_after = read_env_u64("OO_SESSION_BUDGET_RESET_AFTER", 86_400)?;
    if budget_remaining > budget_total {
        return Err(StartupError::InvalidEnv("OO_SESSION_BUDGET_REMAINING"));
    }
    Ok((budget_total, budget_remaining, budget_reset_after))
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).compact().init();
}

fn load_bot_config() -> Result<BotConfig, StartupError> {
    let emoji_id = read_env_u64("OO_EMOJI_ID", 1489695886773587978)?;
    let emoji_name = std::env::var("OO_EMOJI_NAME").unwrap_or_else(|_| "Omilfy".to_string());
    let animated = read_env_bool("OO_EMOJI_ANIMATED", false)?;
    let stamp_text =
        std::env::var("OO_STAMP").unwrap_or_else(|_| format!("<:{emoji_name}:{emoji_id}>"));

    let special_phrase =
        std::env::var("OO_SPECIAL_PHRASE").unwrap_or_else(|_| "これはおお".to_string());
    let max_count_cap = read_env_usize("OO_MAX_COUNT_CAP", 48)?;
    let max_send_chars = read_env_usize("OO_MAX_SEND_CHARS", 1_900)?;

    Ok(BotConfig {
        special_phrase,
        stamp_text,
        reaction: ReactionConfig { emoji_id, emoji_name, animated },
        max_count_cap,
        max_send_chars,
    })
}

fn validate_discord_token(token: &str) -> Result<(), StartupError> {
    // Token must be present and structurally similar to Discord bot token format.
    if token.trim().is_empty() || !token.contains('.') || token.len() < 50 {
        return Err(StartupError::InvalidEnv("DISCORD_TOKEN"));
    }
    Ok(())
}

fn read_env_u64(name: &'static str, default: u64) -> Result<u64, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<u64>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_usize(name: &'static str, default: usize) -> Result<usize, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<usize>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_bool(name: &'static str, default: bool) -> Result<bool, StartupError> {
    match std::env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(StartupError::InvalidEnv(name)),
        },
        Err(_) => Ok(default),
    }
}

fn read_env_f64(name: &'static str, default: f64) -> Result<f64, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<f64>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_id_list(name: &'static str) -> Result<Vec<u64>, StartupError> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(vec![]);
    };

    if raw.trim().is_empty() {
        return Ok(vec![]);
    }

    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().map_err(|_| StartupError::InvalidEnv(name)))
        .collect()
}

fn read_env_mode_override(
    name: &'static str,
) -> Result<Option<discord_oo_bot::security::mode::RuntimeMode>, StartupError> {
    use discord_oo_bot::security::mode::RuntimeMode;

    let Ok(raw) = std::env::var(name) else {
        return Ok(None);
    };

    let mode = match raw.to_ascii_lowercase().as_str() {
        "normal" => RuntimeMode::Normal,
        "observe-only" | "observe_only" => RuntimeMode::ObserveOnly,
        "react-only" | "react_only" => RuntimeMode::ReactOnly,
        "audit-only" | "audit_only" => RuntimeMode::AuditOnly,
        "full-disable" | "full_disable" => RuntimeMode::FullDisable,
        _ => return Err(StartupError::InvalidEnv(name)),
    };
    Ok(Some(mode))
}

fn read_env_u32(name: &'static str, default: u32) -> Result<u32, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<u32>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

fn read_env_u8(name: &'static str, default: u8) -> Result<u8, StartupError> {
    match std::env::var(name) {
        Ok(value) => value.parse::<u8>().map_err(|_| StartupError::InvalidEnv(name)),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::{load_session_budget, read_env_u32, read_env_u8, StartupError};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn rejects_u8_overflow_in_env() {
        let _guard = match env_lock().lock() {
            Ok(guard) => guard,
            Err(err) => panic!("env lock poisoned: {err}"),
        };
        let key = "OO_MAX_ACTIONS_PER_MESSAGE_TEST";
        std::env::set_var(key, "256");
        let parsed = read_env_u8(key, 1);
        std::env::remove_var(key);

        assert!(matches!(parsed, Err(StartupError::InvalidEnv("OO_MAX_ACTIONS_PER_MESSAGE_TEST"))));
    }

    #[test]
    fn rejects_u32_overflow_in_env() {
        let _guard = match env_lock().lock() {
            Ok(guard) => guard,
            Err(err) => panic!("env lock poisoned: {err}"),
        };
        let key = "OO_GLOBAL_RATE_BURST_TEST";
        std::env::set_var(key, "4294967296");
        let parsed = read_env_u32(key, 1);
        std::env::remove_var(key);

        assert!(matches!(parsed, Err(StartupError::InvalidEnv("OO_GLOBAL_RATE_BURST_TEST"))));
    }

    #[test]
    fn rejects_budget_remaining_above_total() {
        let _guard = match env_lock().lock() {
            Ok(guard) => guard,
            Err(err) => panic!("env lock poisoned: {err}"),
        };
        std::env::set_var("OO_SESSION_BUDGET_TOTAL", "5");
        std::env::set_var("OO_SESSION_BUDGET_REMAINING", "6");

        let budget = load_session_budget();

        std::env::remove_var("OO_SESSION_BUDGET_TOTAL");
        std::env::remove_var("OO_SESSION_BUDGET_REMAINING");

        assert!(matches!(budget, Err(StartupError::InvalidEnv("OO_SESSION_BUDGET_REMAINING"))));
    }
}
