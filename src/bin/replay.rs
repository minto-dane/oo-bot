#![forbid(unsafe_code)]

use std::{path::PathBuf, process::ExitCode};

use discord_oo_bot::app::{
    analyze_message::BotConfig,
    replay::{build_replay_core, load_replay_cases, run_replay_case_with_core},
};

fn main() -> ExitCode {
    let input = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures/replay"));

    let config = BotConfig::default();
    let mut core = match build_replay_core(config.clone()) {
        Ok(core) => core,
        Err(err) => {
            eprintln!("failed to initialize sandbox: {err}");
            return ExitCode::FAILURE;
        }
    };

    let cases = match load_replay_cases(&input) {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!("failed to load fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };

    if cases.is_empty() {
        eprintln!("no replay cases found under {}", input.display());
        return ExitCode::FAILURE;
    }

    let mut failures = 0usize;
    for case in &cases {
        if !case.runtime.preserve_state {
            core = match build_replay_core(config.clone()) {
                Ok(core) => core,
                Err(err) => {
                    eprintln!("failed to initialize sandbox: {err}");
                    return ExitCode::FAILURE;
                }
            };
        }
        if let Err(diff) = run_replay_case_with_core(case, &mut core) {
            failures += 1;
            eprintln!("{diff}");
        }
    }

    if failures > 0 {
        eprintln!("replay failed: {failures}/{} case(s)", cases.len());
        ExitCode::FAILURE
    } else {
        println!("replay succeeded: {} case(s)", cases.len());
        ExitCode::SUCCESS
    }
}
