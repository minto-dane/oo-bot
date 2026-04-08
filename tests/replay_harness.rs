use std::path::Path;

use discord_ooh_bot::{
    app::{
        analyze_message::BotConfig,
        replay::{load_replay_cases, run_replay_case, run_replay_case_with_core},
    },
    generated::kanji_oo_db::KANJI_OO_DB,
    sandbox::abi::{ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer},
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::core_governor::{RuntimeProtectionConfig, TrustedCore},
};

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

#[test]
fn replay_fixtures_match_expected_actions() {
    let fixtures =
        load_replay_cases(Path::new("tests/fixtures/replay")).expect("fixtures must load");
    assert!(!fixtures.is_empty(), "fixtures must not be empty");

    let cfg = BotConfig::default();
    let mut core = build_core(cfg.clone());

    for case in &fixtures {
        if !case.runtime.preserve_state {
            core = build_core(cfg.clone());
        }

        let runtime_sensitive = case.runtime != Default::default() || case.expected_mode.is_some();
        if !runtime_sensitive {
            run_replay_case(case, &cfg, &KANJI_OO_DB)
                .unwrap_or_else(|diff| panic!("replay mismatch: {diff}"));
        }
        run_replay_case_with_core(case, &mut core)
            .unwrap_or_else(|diff| panic!("governed replay mismatch: {diff}"));
    }
}

fn build_core(cfg: BotConfig) -> TrustedCore {
    let analyzer = WasmtimeSandboxAnalyzer::new(SandboxConfig {
        fuel_limit: 800_000,
        ..SandboxConfig::default()
    })
    .expect("sandbox should initialize");

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

    TrustedCore::new(
        Box::new(ReplayHarnessAnalyzer { inner: analyzer }),
        cfg,
        runtime_cfg,
        &KANJI_OO_DB,
    )
}
