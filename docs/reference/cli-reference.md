# CLI Reference

## 目的

このリポジトリで利用する主要 CLI を定義します。

## cargo commands

- build/check
  - `cargo check --workspace --all-features`
- test
  - `cargo test --workspace --all-features`
- replay
  - `cargo run --bin replay -- tests/fixtures/replay`
- clippy
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## xtask

- `cargo xtask generate`
- `cargo xtask verify`

## just

- `just ci-local`
- `just runtime-smoke`
- `just fault-inject`
- `just fuzz-smoke`
- `just bench-sanity`

## CI heavy tooling

- `cargo nextest run --workspace --all-features --config-file nextest.toml`
- `cargo llvm-cov --workspace --all-features --lcov --output-path target/coverage.lcov`
- `cargo audit`
- `cargo deny check`
- `cargo geiger --all-features`
- `semgrep --config semgrep.yml --error .`
