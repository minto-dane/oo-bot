use std::path::Path;

use discord_oo_bot::app::{
    analyze_message::BotConfig,
    replay::{build_replay_core, load_replay_cases, run_replay_case, run_replay_case_with_core},
};

#[test]
fn replay_fixtures_match_expected_actions() {
    let fixtures =
        load_replay_cases(Path::new("tests/fixtures/replay")).expect("fixtures must load");
    assert!(!fixtures.is_empty(), "fixtures must not be empty");

    let cfg = BotConfig::default();
    let mut core = build_replay_core(cfg.clone()).expect("sandbox should initialize");

    for case in &fixtures {
        if !case.runtime.preserve_state {
            core = build_replay_core(cfg.clone()).expect("sandbox should initialize");
        }

        let runtime_sensitive = case.runtime != Default::default() || case.expected_mode.is_some();
        if !runtime_sensitive {
            run_replay_case(case, &cfg).unwrap_or_else(|diff| panic!("replay mismatch: {diff}"));
        }
        run_replay_case_with_core(case, &mut core)
            .unwrap_or_else(|diff| panic!("governed replay mismatch: {diff}"));
    }
}
