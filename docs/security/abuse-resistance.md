# Abuse Resistance

## 目的

ユーザー入力由来の濫用（スパム・高負荷・再配送）への耐性を定義します。

## 想定 abuse

- 短時間多投稿
- 重複 event 再配送
- 極端長文
- bidi 制御文字や反復過多入力
- REST 失敗誘発パターン

## 制御

- duplicate guard
- per-user/per-channel/per-guild/global cooldown
- global token bucket
- suspicious soft/hard
- max send chars / max action count
- breaker open による observe-only

## 仕様境界

- suspicious soft は send を react に縮退する
- suspicious hard は suppress reason `suspicious` で Noop

## 検証

- fixture: [tests/fixtures/replay](../../tests/fixtures/replay)
- replay: [tests/replay_harness.rs](../../tests/replay_harness.rs)
- suppress meta: [tests/replay_suppress_reason_regression.rs](../../tests/replay_suppress_reason_regression.rs)
- property: [tests/property_runtime.rs](../../tests/property_runtime.rs)
