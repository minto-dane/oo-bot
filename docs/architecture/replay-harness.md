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
- sandbox trap/timeout は harness analyzer で injected error を返す
