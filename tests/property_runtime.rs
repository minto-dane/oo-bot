use discord_oo_bot::{
    app::analyze_message::{BotAction, BotConfig},
    generated::kanji_oo_db::KANJI_OO_DB,
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::core_governor::{MessageContext, RuntimeProtectionConfig, TrustedCore},
};
use proptest::prelude::*;

fn build_core(max_send: usize) -> TrustedCore {
    let analyzer =
        WasmtimeSandboxAnalyzer::new(SandboxConfig::default()).expect("sandbox should initialize");
    let cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        max_send_chars: max_send,
        ..RuntimeProtectionConfig::default()
    };
    TrustedCore::new(Box::new(analyzer), BotConfig::default(), cfg, &KANJI_OO_DB)
}

proptest! {
    #[test]
    fn governor_never_exceeds_send_char_cap(s in any::<String>()) {
        let max_send = 128usize;
        let mut core = build_core(max_send);
        let decision = core.decide_message(
            MessageContext {
                message_id: 1,
                author_id: 2,
                channel_id: 3,
                guild_id: Some(4),
                author_is_bot: false,
            },
            &s,
        );

        if let BotAction::SendMessage { content } = decision.action {
            prop_assert!(content.chars().count() <= max_send);
        }
    }

    #[test]
    fn duplicate_suppression_is_idempotent(s in any::<String>()) {
        let mut core = build_core(256);
        let ctx = MessageContext {
            message_id: 42,
            author_id: 7,
            channel_id: 8,
            guild_id: Some(9),
            author_is_bot: false,
        };

        let _ = core.decide_message(ctx, &s);
        let second = core.decide_message(ctx, &s);
        prop_assert_eq!(second.action, BotAction::Noop);
    }

    #[test]
    fn suspicious_handling_never_panics(s in any::<String>()) {
        let mut core = build_core(512);
        let _ = core.decide_message(
            MessageContext {
                message_id: 100,
                author_id: 200,
                channel_id: 300,
                guild_id: None,
                author_is_bot: false,
            },
            &s,
        );
    }
}
