# Replay Harness Architecture

## 目的

Discord 非依存で runtime decision を再現・固定回帰する仕組みを定義します。

## コンポーネント

- replay model: [src/app/replay.rs](../../src/app/replay.rs)
- replay CLI: [src/bin/replay.rs](../../src/bin/replay.rs)
- fixture directory: [tests/fixtures/replay](../../tests/fixtures/replay)

## fixture 特性

- YAML/JSON 両対応
- runtime overrides (mode, allow/deny, status inject, thresholds)
- expected action
- expected mode
- expected suppress_reason

## 実行モデル

replay は pure analyzer の一致確認だけでなく、governed path の固定回帰を目的にしています。

- CLI と test harness は同じ replay core builder を使います
- baseline runtime は replay 向けに固定されます
  - cooldown はすべて `0`
  - breaker threshold は `2`
  - long message soft/hard threshold は baseline fixture が純粋 analyzer と整合するよう緩めに設定
- fixture ごとに `TrustedCore` を再生成するのが既定動作です
- `runtime.preserve_state: true` の場合だけ stateful fixture として直前状態を持ち越します

これにより、duplicate / breaker / sandbox failure のような stateful guardrail だけを明示的に再現できます。

## suppress_reason 回帰固定

- harness は `expected_suppress_reason` を照合
- 専用回帰テスト: [tests/replay_suppress_reason_regression.rs](../../tests/replay_suppress_reason_regression.rs)
- CI runtime-protection job で常時実行

## 実行コマンド

```bash
cargo run --bin replay -- tests/fixtures/replay
cargo test --test replay_harness --test replay_suppress_reason_regression --all-features
```

## 注意点

- duplicate fixture は `preserve_state: true` が必要
- breaker を開かせたい fixture も、複数ケースで状態を共有するなら `preserve_state: true` が必要
- threshold override (`soft_char_limit` / `hard_char_limit` / `repetition_threshold`) は各 fixture の終了後に持ち越されない
- sandbox trap/timeout は harness analyzer で injected error を返す
- replay mismatch が `Cooldown` に偏る場合は、fixture 側ではなく harness の state reset ルールを疑う
