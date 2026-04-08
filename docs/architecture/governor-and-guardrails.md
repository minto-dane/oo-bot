# Governor and Guardrails

## 目的

core が適用する保護ガードを機能別に定義します。

## 入力検証系

- self-trigger 防止
- guild/channel allowlist/denylist
- duplicate guard (message_id + TTL)
- suspicious classifier (soft/hard)

## 出力制御系

- mode gate
- global token bucket
- per-user/per-channel/per-guild/global cooldown
- per-message max action count
- max send chars
- invalid action rejection

## 外部エラー制御系

- 401/403/429 を breaker に集約
- breaker open 中は outbound 抑止
- session budget low 時は `react_only`

## 実装マップ

- duplicate: [src/security/duplicate_guard.rs](../../src/security/duplicate_guard.rs)
- bucket: [src/security/rate_limiter.rs](../../src/security/rate_limiter.rs)
- breaker: [src/security/circuit_breaker.rs](../../src/security/circuit_breaker.rs)
- session budget: [src/security/session_budget.rs](../../src/security/session_budget.rs)
- suspicious: [src/security/suspicious_input.rs](../../src/security/suspicious_input.rs)

## 期待する安全性

- analyzer failure が即 outbound にならない
- REST/Gateway 側異常で無制限再試行しない
- 同一イベント再配送で重複反応しない
