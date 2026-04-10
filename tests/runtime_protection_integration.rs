use discord_oo_bot::{
    app::analyze_message::{BotAction, BotConfig},
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::{
        core_governor::{MessageContext, RuntimeProtectionConfig, TrustedCore},
        mode::RuntimeMode,
    },
};

fn test_context(message_id: u64) -> MessageContext {
    MessageContext {
        message_id,
        author_id: 10,
        channel_id: 20,
        guild_id: Some(30),
        author_is_bot: false,
    }
}

fn core_with_default() -> TrustedCore {
    let analyzer =
        WasmtimeSandboxAnalyzer::new(SandboxConfig::default()).expect("sandbox should initialize");

    let cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        ..RuntimeProtectionConfig::default()
    };

    TrustedCore::new(Box::new(analyzer), BotConfig::default(), cfg)
}

#[test]
fn governed_path_keeps_existing_behavior_for_mixed_input() {
    let mut core = core_with_default();
    let decision = core.decide_message(test_context(1), "おおoo大");
    assert!(matches!(decision.action, BotAction::SendMessage { .. }));
}

#[test]
fn duplicate_message_is_suppressed() {
    let mut core = core_with_default();

    let first = core.decide_message(test_context(100), "oo");
    assert!(matches!(
        first.action,
        BotAction::React { .. } | BotAction::SendMessage { .. }
    ));

    let second = core.decide_message(test_context(100), "oo");
    assert_eq!(second.action, BotAction::Noop);
}

#[test]
fn breaker_forces_observe_only() {
    let analyzer =
        WasmtimeSandboxAnalyzer::new(SandboxConfig::default()).expect("sandbox should initialize");

    let cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        breaker_threshold: 2,
        ..RuntimeProtectionConfig::default()
    };

    let mut core = TrustedCore::new(Box::new(analyzer), BotConfig::default(), cfg);
    core.record_http_status(429);
    core.record_http_status(429);

    let decision = core.decide_message(test_context(2), "oooo");
    assert_eq!(decision.mode, RuntimeMode::ObserveOnly);
    assert_eq!(decision.action, BotAction::Noop);
}

#[test]
fn react_only_mode_is_honored() {
    let analyzer =
        WasmtimeSandboxAnalyzer::new(SandboxConfig::default()).expect("sandbox should initialize");

    let cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        mode_override: Some(RuntimeMode::ReactOnly),
        ..RuntimeProtectionConfig::default()
    };

    let mut core = TrustedCore::new(Box::new(analyzer), BotConfig::default(), cfg);

    let decision = core.decide_message(test_context(3), "oooo");
    assert_eq!(decision.mode, RuntimeMode::ReactOnly);
    assert!(matches!(decision.action, BotAction::React { .. }));
}
