# Metrics Reference

## 目的

runtime metrics とログフィールドの意味を定義します。

## RuntimeMetrics fields

| Field | 意味 |
|---|---|
| analyzer_calls_total | analyzer 呼び出し回数 |
| analyzer_traps_total | trap 系エラー回数 |
| analyzer_timeout_total | timeout 回数 |
| messages_dropped_total | drop したメッセージ数 |
| duplicate_suppressed_total | duplicate 抑止回数 |
| outbound_suppressed_total | outbound 抑止回数 |
| cooldown_hits_total | cooldown 命中回数 |
| invalid_request_prevented_total | 401/403/429 記録回数 |
| session_budget_low_total | budget low 判定回数 |
| reconnect_attempts_total | reconnect 試行回数 |
| resume_failures_total | resume 失敗回数 |
| mode_transitions_total | mode 遷移回数 |

定義: [src/security/core_governor.rs](../../src/security/core_governor.rs)

## ログ推奨利用

- `suppress_reason` と `mode` を時系列で追う
- `content_len` と `suspicion` で入力異常を追う
- `error` と `status` で Discord 側失敗を追う

## 制約

- 現在は in-process カウンタのみ
- exporter 不在のため外部時系列基盤連携は未実装
