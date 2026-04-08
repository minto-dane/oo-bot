# Logging and Redaction

## 目的

インシデント調査可能性を維持しつつ、機密と個人情報の過剰露出を防ぎます。

## 記録する項目

- guild_id, channel_id, message_id, author_id
- content_len
- analyzer_result
- final_action
- suppress_reason
- mode
- suspicion

実装: [src/infra/discord_handler.rs](../../src/infra/discord_handler.rs)

## 記録しない項目

- message content 全文
- token 値

## redaction ポリシー

- 文字列内容は長さ・分類メタへ置換
- エラーは status code と分類を優先

## semgrep ガード

- no-token-logging
- no-message-content-logs

設定: [semgrep.yml](../../semgrep.yml)

## 運用注意

- デバッグ時も message 本文を常時ログしない
- triage 目的で必要な場合は一時的かつ限定範囲で運用手順に従う
