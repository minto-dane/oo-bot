# Replay Fixture Format

## 目的

replay fixture のスキーマを固定し、回帰追加を統一します。

## 対応形式

- YAML (`.yaml`, `.yml`)
- JSON (`.json`)

## 共通フィールド

| Field | Type | Required | 説明 |
|---|---|---|---|
| name | string | yes | ケース識別名 |
| content | string | yes | メッセージ本文 |
| message_id | u64 | no | 未指定時は name から安定ハッシュ |
| author_id | u64 | no | default 100 |
| channel_id | u64 | no | default 200 |
| guild_id | u64? | no | DM は null |
| author_is_bot | bool | no | default false |
| expected | enum | yes | 最終期待 action |
| expected_mode | enum? | no | mode 期待値 |
| expected_suppress_reason | enum? | no | suppress reason 期待値 |
| runtime | object | no | runtime override |

## expected.type

- noop
- react
- send_message

## runtime fields

- mode_override
- emergency_kill_switch
- allow_guild_ids / deny_guild_ids
- allow_channel_ids / deny_channel_ids
- inject_statuses
- soft_char_limit / hard_char_limit / repetition_threshold
- preserve_state

### runtime fields の意味

- `inject_statuses`:
  trusted core に HTTP status を事前注入します。
  breaker を開いたり invalid request 系の縮退を再現したい場合に使います。
- `soft_char_limit` / `hard_char_limit` / `repetition_threshold`:
  その fixture 実行中だけ suspicious classifier を上書きします。
  次の fixture には持ち越されません。
- `preserve_state`:
  `true` の場合、直前ケースの duplicate cache / breaker / sandbox failure spike などの state を共有します。
  既定値は `false` です。

## suppress reason 値

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

## 例

```yaml
name: duplicated_event_second
message_id: 500
content: "oo"
runtime:
  preserve_state: true
expected_suppress_reason: duplicate
expected:
  type: noop
```

```yaml
name: breaker_observe_mode
content: "oooo"
runtime:
  inject_statuses: [429, 429]
expected_suppress_reason: mode_restricted
expected_mode: observe_only
expected:
  type: noop
```

## 実務上の指針

- duplicate や multi-step breaker fixture 以外では `preserve_state` を付けない
- runtime override を使う場合は、`expected_mode` か `expected_suppress_reason` のどちらかも付けて意図を固定する
- `sandbox_timeout` / `sandbox_trap` は replay harness 内の injected error で再現されるため、実際の wall-clock timeout を期待しない

## 実装

- schema model: [src/app/replay.rs](../../src/app/replay.rs)
- fixtures: [tests/fixtures/replay](../../tests/fixtures/replay)
