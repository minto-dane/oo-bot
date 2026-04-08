# Observability

## 目的

運転状態の把握に必要なログ/メトリクスを定義します。

## ログ

- 実装: [src/infra/discord_handler.rs](../../src/infra/discord_handler.rs)
- 主フィールド:
  - guild_id
  - channel_id
  - message_id
  - author_id
  - content_len
  - analyzer_result
  - final_action
  - suppress_reason
  - mode
  - suspicion

## メトリクス（内部カウンタ）

- analyzer_calls_total
- analyzer_traps_total
- analyzer_timeout_total
- messages_dropped_total
- duplicate_suppressed_total
- outbound_suppressed_total
- cooldown_hits_total
- invalid_request_prevented_total
- session_budget_low_total
- reconnect_attempts_total
- resume_failures_total
- mode_transitions_total

定義: [src/security/core_governor.rs](../../src/security/core_governor.rs)

## 現在の制約

- 外部 exporter (Prometheus/OpenTelemetry) は未実装
- triage は structured log 中心

## 推奨運用

- suppress_reason の急増をアラート条件化
- 401/403/429 連発時は mode を確認
- session budget low ログを release 判定に利用
