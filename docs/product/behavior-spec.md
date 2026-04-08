# Behavior Specification

## 目的

この文書は「このシステムが何を返すか」を定義します。
実装詳細ではなく外部挙動を規定します。

## 入力

- MessageContext
  - message_id
  - author_id
  - channel_id
  - guild_id
  - author_is_bot
- message content (UTF-8 文字列)
- runtime config
- generated kanji DB

## 出力

- `BotAction::Noop`
- `BotAction::React { emoji_id, emoji_name, animated }`
- `BotAction::SendMessage { content }`

## 優先順位

1. full_disable 判定
2. self-trigger 判定
3. allow/deny 判定
4. duplicate 判定
5. suspicious 判定
6. analyzer proposal
7. mode gate
8. limiter/cooldown/caps

## 主要ルール

- author が bot の場合は常に Noop
- special phrase (`OO_SPECIAL_PHRASE`) が含まれる場合は stamp 1件送信
- total count が 1 の場合は react
- total count が 2 以上の場合は stamp テキストを空白区切りで送信
- count は `OO_MAX_COUNT_CAP` と `OO_MAX_SEND_CHARS` で上限化

## runtime protection での縮退

- `observe_only` / `audit_only` / `full_disable`: outbound しない
- `react_only`: send proposal を react へ縮退
- breaker open: mode を `observe_only` へ遷移
- session budget low: mode を `react_only` へ遷移
- sandbox failure spike: mode を `audit_only` へ遷移

## suppress_reason の仕様

以下の理由で Noop になる場合、`suppress_reason` を付与します。

- `self_trigger`
- `duplicate`
- `cooldown`
- `rate_limit`
- `circuit_open`
- `channel_denied`
- `guild_denied`
- `mode_restricted`
- `suspicious`
- `invalid_action`

## 非スコープ

- 文脈依存の語義判定
- 熟語読みの推定
- grapheme cluster 単位の一致

## 検証方法

- replay: [tests/replay_harness.rs](../../tests/replay_harness.rs)
- suppress reason 固定回帰: [tests/replay_suppress_reason_regression.rs](../../tests/replay_suppress_reason_regression.rs)
- runtime 統合: [tests/runtime_protection_integration.rs](../../tests/runtime_protection_integration.rs)
