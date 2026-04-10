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

## Deterministic テスト原則

- OS 環境依存 (`/run` の有無、環境変数の競合) を直接テスト条件にしない
- path 選択ロジックは resolver へ分離し、注入可能な文脈で分岐を固定化する
- global 環境変数を書き換えるテストは最小化し、必要なら lock で直列化する
- property テストはケース数と入力長を明示し、CI 予算内で安定実行する

## 推奨実行順

1. `cargo test --workspace --all-features`
2. `cargo test --test replay_harness --test replay_suppress_reason_regression --all-features`
3. `cargo test --test fault_injection --all-features`
4. `just fuzz-smoke` (任意)

## バグ固定方針

- 挙動バグ: replay fixture を追加
- 抑止理由バグ: `expected_suppress_reason` を追加
- 回帰確認: replay 系 CI job に常設
