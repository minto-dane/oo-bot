# CLI Reference

## 目的

このリポジトリで利用する主要 CLI を定義します。

## 対象読者

- Bot を実行する運用者
- 品質確認や監査を行う開発者 / CI

Nix は後者向けの任意手段です。Bot を動かすだけなら必須ではありません。

## cargo commands

- build/check
  - `cargo check --workspace --all-features`
- test
  - `cargo test --workspace --all-features`
- replay
  - `cargo run --bin replay -- tests/fixtures/replay`
  - replay CLI は state reset / `preserve_state` / sandbox trap/timeout injection を含めて test harness と同じ挙動を取る
- clippy
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## 推奨の確認順

1. fixture 回帰だけを見たい:
   - `cargo run --bin replay -- tests/fixtures/replay`
2. replay 固定回帰を test として回したい:
   - `cargo test --test replay_harness --test replay_suppress_reason_regression --all-features`
3. runtime guardrail を追加確認したい:
   - `cargo test --test runtime_protection_integration --test fault_injection --all-features`
4. 起動設定の unit test を見たい:
   - `cargo test --bin discord-oo-bot --all-features`

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

## ツール導入の考え方

- 運用者
  - `cargo xtask generate`
  - `cargo run`
- 開発者 / CI
  - Nix を使う場合は `nix develop`
  - 非 Nix 環境では `./scripts/bootstrap_security_tools.sh`
  - Nix を使う場合は、監査ツールの再現可能な配布手段として使う
  - end user に Nix を要求するためではなく、contributor / CI の toolchain 管理のために使う
