use std::path::Path;

use discord_oo_bot::{
    app::{
        analyze_message::BotConfig,
        replay::{build_replay_core, load_replay_cases, run_replay_case_with_core},
    },
    generated::kanji_oo_db::KANJI_OO_DB,
};

#[test]
fn replay_suppress_reason_expectations_match() {
    let fixtures =
        load_replay_cases(Path::new("tests/fixtures/replay")).expect("fixtures must load");
    assert!(!fixtures.is_empty(), "fixtures must not be empty");

    let cfg = BotConfig::default();
    let mut core = build_replay_core(cfg.clone(), &KANJI_OO_DB).expect("sandbox should initialize");

    let mut suppress_reason_expectations = 0usize;

    for case in &fixtures {
        if !case.runtime.preserve_state {
            core = build_replay_core(cfg.clone(), &KANJI_OO_DB).expect("sandbox should initialize");
        }

        if case.expected_suppress_reason.is_some() {
            suppress_reason_expectations += 1;
        }

        run_replay_case_with_core(case, &mut core)
            .unwrap_or_else(|diff| panic!("suppress-reason replay mismatch: {diff}"));
    }

    assert!(
        suppress_reason_expectations >= 8,
        "expected at least 8 suppress_reason-tagged fixtures, got {suppress_reason_expectations}"
    );
}
