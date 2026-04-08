# Runtime Protection Architecture

## 目的

Bot 自体で暴走・連投・制約違反連発を抑止する設計を定義します。

## 設計原則

- capability 分離を主防御にする
- fail-open ではなく fail-safe を選ぶ
- 異常検知時は mode 縮退へ遷移
- suppress 理由を観測可能にする

## trusted core の責務

- `MessageContext` の受理
- allow/deny 判定
- duplicate suppression
- suspicious input classification
- analyzer proposal の受理
- final gate (mode/cooldown/rate/invalid action)
- mode 遷移

実装: [src/security/core_governor.rs](../../src/security/core_governor.rs)

## mode/state machine

| Mode | 説明 | 主なトリガ |
|---|---|---|
| normal | 通常送信 | 回復時 |
| observe_only | outbound 抑止 | breaker open |
| react_only | send を react に縮退 | session budget low |
| audit_only | outbound 抑止 | sandbox failure spike |
| full_disable | 完全停止 | emergency kill switch |

## 異常 -> mode 遷移

| 異常 | mode |
|---|---|
| repeated 401/403/429 | observe_only |
| sandbox timeout/trap spike | audit_only |
| session budget low | react_only |
| emergency kill switch | full_disable |

## suppress_reason

Noop の原因を機械判定可能な enum で記録します。

- self_trigger
- duplicate
- cooldown
- rate_limit
- circuit_open
- channel_denied
- guild_denied
- mode_restricted
- suspicious
- invalid_action

## 検証

- replay suppress regression: [tests/replay_suppress_reason_regression.rs](../../tests/replay_suppress_reason_regression.rs)
- runtime integration: [tests/runtime_protection_integration.rs](../../tests/runtime_protection_integration.rs)
- fault injection: [tests/fault_injection.rs](../../tests/fault_injection.rs)
