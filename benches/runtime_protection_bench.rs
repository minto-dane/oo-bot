use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use discord_oo_bot::{
    app::analyze_message::{analyze_message, BotConfig},
    sandbox::host::{SandboxConfig, WasmtimeSandboxAnalyzer},
    security::{
        core_governor::{MessageContext, RuntimeProtectionConfig, TrustedCore},
        duplicate_guard::DuplicateGuard,
    },
};

fn bench_runtime(c: &mut Criterion) {
    let content = "おおooオオ大".repeat(64);
    let cfg = BotConfig::default();

    let mut group = c.benchmark_group("runtime_protection");
    group.measurement_time(Duration::from_secs(4));

    group.bench_with_input(BenchmarkId::new("pure_analyze", "mixed"), &content, |b, input| {
        b.iter(|| {
            let _ = analyze_message(input, false, &cfg);
        });
    });

    let analyzer =
        WasmtimeSandboxAnalyzer::new(SandboxConfig::default()).expect("sandbox should initialize");
    let runtime_cfg = RuntimeProtectionConfig {
        per_user_cooldown_ms: 0,
        per_channel_cooldown_ms: 0,
        per_guild_cooldown_ms: 0,
        global_cooldown_ms: 0,
        ..RuntimeProtectionConfig::default()
    };
    let mut core = TrustedCore::new(Box::new(analyzer), cfg.clone(), runtime_cfg);

    let mut message_id = 0u64;
    group.bench_with_input(BenchmarkId::new("sandbox_governor", "mixed"), &content, |b, input| {
        b.iter(|| {
            message_id = message_id.saturating_add(1);
            let _ = core.decide_message(
                MessageContext {
                    message_id,
                    author_id: 1,
                    channel_id: 2,
                    guild_id: Some(3),
                    author_is_bot: false,
                },
                input,
            );
        });
    });

    let mut duplicate = DuplicateGuard::new(Duration::from_secs(60), 32_768);
    let now = Instant::now();
    group.bench_function("duplicate_guard", |b| {
        let mut id = 0u64;
        b.iter(|| {
            id = id.saturating_add(1);
            let _ = duplicate.is_duplicate_and_mark(id, now);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_runtime);
criterion_main!(benches);
