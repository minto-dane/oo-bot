set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just ci-local

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace --all-features

integration:
    cargo test --test analyze_message_integration --test replay_harness --test replay_suppress_reason_regression --test generated_db --test runtime_protection_integration --all-features

runtime-smoke:
    cargo test --test runtime_protection_integration --test replay_harness --test replay_suppress_reason_regression --all-features

fault-inject:
    cargo test --test fault_injection --all-features

nextest:
    cargo nextest run --workspace --all-features --config-file nextest.toml

coverage:
    cargo llvm-cov --workspace --all-features --lcov --output-path target/coverage.lcov

audit:
    cargo audit

deny:
    cargo deny check

geiger:
    cargo geiger --all-features

semgrep:
    semgrep --config semgrep.yml --error .

hack:
    cargo hack check --workspace --feature-powerset --depth 1

minimal-features:
    cargo hack check --workspace --no-dev-deps --each-feature

all-features:
    cargo check --workspace --all-features

generate-db:
    cargo xtask generate

verify-generated:
    cargo xtask verify

docs:
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps

replay:
    cargo run --bin replay -- tests/fixtures/replay

fuzz-smoke:
    cargo fuzz run analyze_message -- -max_total_time=10
    cargo fuzz run sandbox_abi -- -max_total_time=10
    cargo fuzz run replay_parser -- -max_total_time=10

bench-sanity:
    cargo bench --bench oo_count_bench -- --warm-up-time 0.1 --measurement-time 0.2
    cargo bench --bench runtime_protection_bench -- --warm-up-time 0.1 --measurement-time 0.2

miri:
    cargo +nightly miri test --workspace

udeps:
    cargo +nightly udeps --workspace --all-targets

security-local: audit deny geiger semgrep

ci-local: fmt-check clippy test integration runtime-smoke fault-inject nextest coverage audit deny geiger semgrep minimal-features all-features verify-generated docs replay
