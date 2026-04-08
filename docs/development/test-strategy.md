# Test Strategy

## 目的

このリポジトリのテスト責務分割と実行順序を定義します。

## レイヤ別

- unit
  - domain/app/security/sandbox 各モジュール
- integration
  - runtime protection, replay harness, generated DB
- fault injection
  - trap/timeout/kill-switch/degrade
- property
  - Unicode 入力で panic しないこと、cap 不変条件
- fuzz
  - analyzer/ABI/replay parser
- benchmark
  - pure vs sandbox+governor overhead

## 推奨実行順

1. `cargo test --workspace --all-features`
2. `cargo test --test replay_harness --test replay_suppress_reason_regression --all-features`
3. `cargo test --test fault_injection --all-features`
4. `just fuzz-smoke` (任意)

## バグ固定方針

- 挙動バグ: replay fixture を追加
- 抑止理由バグ: `expected_suppress_reason` を追加
- 回帰確認: replay 系 CI job に常設
