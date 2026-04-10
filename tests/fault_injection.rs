use discord_oo_bot::{
    app::analyze_message::{BotAction, BotConfig},
    sandbox::abi::{
        ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer, SANDBOX_ABI_VERSION,
    },
    security::{
        core_governor::{MessageContext, RuntimeProtectionConfig, TrustedCore},
        mode::RuntimeMode,
    },
};

#[derive(Debug, Clone, Copy)]
enum FailureKind {
    Trap,
    Timeout,
    AbiMismatch,
}

struct FailingAnalyzer {
    kind: FailureKind,
}

impl ProposalAnalyzer for FailingAnalyzer {
    fn abi_version(&self) -> u32 {
        if matches!(self.kind, FailureKind::AbiMismatch) {
            SANDBOX_ABI_VERSION + 1
        } else {
            SANDBOX_ABI_VERSION
        }
    }

    fn propose(&mut self, _req: &AnalyzerRequest<'_>) -> Result<ActionProposal, AnalyzerError> {
        match self.kind {
            FailureKind::Trap => Err(AnalyzerError::Trap("guest trap".to_string())),
            FailureKind::Timeout => Err(AnalyzerError::Timeout),
            FailureKind::AbiMismatch => Err(AnalyzerError::AbiMismatch {
                expected: SANDBOX_ABI_VERSION,
                actual: SANDBOX_ABI_VERSION + 1,
            }),
        }
    }
}

fn ctx(id: u64) -> MessageContext {
    MessageContext {
        message_id: id,
        author_id: 1,
        channel_id: 2,
        guild_id: Some(3),
        author_is_bot: false,
    }
}

fn runtime_cfg() -> RuntimeProtectionConfig {
    RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        sandbox_failure_threshold: 2,
        ..RuntimeProtectionConfig::default()
    }
}

#[test]
fn analyzer_trap_does_not_produce_outbound_action() {
    let mut core = TrustedCore::new(
        Box::new(FailingAnalyzer { kind: FailureKind::Trap }),
        BotConfig::default(),
        runtime_cfg(),
    );

    let decision = core.decide_message(ctx(1), "oo");
    assert_eq!(decision.action, BotAction::Noop);
}

#[test]
fn repeated_analyzer_failures_degrade_mode_to_audit_only() {
    let mut core = TrustedCore::new(
        Box::new(FailingAnalyzer { kind: FailureKind::Timeout }),
        BotConfig::default(),
        runtime_cfg(),
    );

    let first = core.decide_message(ctx(10), "oo");
    assert_eq!(first.action, BotAction::Noop);

    let second = core.decide_message(ctx(11), "oo");
    assert_eq!(second.action, BotAction::Noop);

    let third = core.decide_message(ctx(12), "oo");
    assert_eq!(third.mode, RuntimeMode::AuditOnly);
}

#[test]
fn invalid_config_kill_switch_forces_full_disable() {
    let mut cfg = runtime_cfg();
    cfg.emergency_kill_switch = true;

    let mut core = TrustedCore::new(
        Box::new(FailingAnalyzer { kind: FailureKind::AbiMismatch }),
        BotConfig::default(),
        cfg,
    );

    let decision = core.decide_message(ctx(99), "oo");
    assert_eq!(decision.mode, RuntimeMode::FullDisable);
    assert_eq!(decision.action, BotAction::Noop);
}
