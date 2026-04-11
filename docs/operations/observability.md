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

## 高度分析（Target 検出 / Response）

監査 DB (`audit_events`) には、target 検出と response 判定の詳細フィールド
(`matched_readings_json`, `sequence_hits`, `kanji_hits`, `selected_action`, `suppressed_reason`, `mode`, `processing_time_ms`) が保存されます。

詳細分析は次のスクリプトで実行できます。

```bash
python3 scripts/audit_advanced_analysis.py \
  --db state/audit/events.sqlite3 \
  --out-dir state/security/audit-advanced-analysis \
  --chart
```

出力される主な成果物:

- `summary.json`
- `event_type_counts.csv`
- `response_action_counts.csv`
- `suppression_reason_counts.csv`
- `mode_counts.csv`
- `matched_readings_top.csv`
- `events_per_hour.csv`
- `*.png` (チャート生成時)

`--chart` は `matplotlib` が必要です。未導入時は CSV/JSON のみ生成されます。

## 推奨運用

- suppress_reason の急増をアラート条件化
- 401/403/429 連発時は mode を確認
- session budget low ログを release 判定に利用
