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
- bot run
  - `cargo run --bin oo-bot -- run`
- runtime control
  - `cargo run --bin oo-bot -- control status`
  - `cargo run --bin oo-bot -- control stop`
- dashboard / operator tui
  - `cargo run --bin oo-bot -- tui`
  - `cargo run --bin oo-bot -- tui --page setup|dashboard|diagnostics|audit`
- config
  - `cargo run --bin oo-bot -- config init`
  - `cargo run --bin oo-bot -- config init --force`
  - `cargo run --bin oo-bot -- config setup`
  - `cargo run --bin oo-bot -- config edit`
- audit
  - `cargo run --bin oo-bot -- audit tail --limit 100`
  - `cargo run --bin oo-bot -- audit stats`
  - `cargo run --bin oo-bot -- audit inspect <event_id>`
  - `cargo run --bin oo-bot -- audit verify`
  - `cargo run --bin oo-bot -- audit export --format jsonl|csv|parquet --out <path>`
  - `cargo run --bin oo-bot -- audit tui`
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
  - `cargo test --bin oo-bot --all-features`
5. 初期設定を作る / 変更したい:
  - `cargo run --bin oo-bot -- config init`
  - `cargo run --bin oo-bot -- config setup`
  - `cargo run --bin oo-bot -- tui --page setup`
6. 実行中 bot の状態確認 / 停止:
  - `cargo run --bin oo-bot -- control status`
  - `cargo run --bin oo-bot -- control stop`

## just

- `just ci-local`
- `just runtime-smoke`
- `just fault-inject`
- `just fuzz-smoke`
- `just bench-sanity`
- `just hardened-x64`
- `just verify-hardening`

## CI heavy tooling

- `cargo nextest run --workspace --all-features --config-file nextest.toml`
- `cargo llvm-cov --workspace --all-features --lcov --output-path target/coverage.lcov`
- `cargo audit`
- `cargo deny check`
- `cargo geiger --all-features`
- `semgrep --config semgrep.yml --error .`

## ツール導入の考え方

- 運用者
  - `cargo run --bin oo-bot -- run`
- 開発者 / CI
  - Nix を使う場合は `nix develop`
  - 非 Nix 環境では `./scripts/bootstrap_security_tools.sh`
  - Nix を使う場合は、監査ツールの再現可能な配布手段として使う
  - end user に Nix を要求するためではなく、contributor / CI の toolchain 管理のために使う
