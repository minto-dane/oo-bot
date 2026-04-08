# Module Boundaries

## 目的

コード配置と責務境界を固定し、ロジックの逆流を防ぎます。

## レイヤ定義

- domain
  - 純粋関数群。外部 I/O なし。
  - 対象: oo カウント、読み正規化、漢字 DB 参照。
- app
  - ユースケース調停。
  - pure analyzer と replay 入出力モデルを提供。
- sandbox
  - ActionProposal 生成器。
  - Wasmtime host と ABI。
- security
  - trusted core/governor。
  - mode、duplicate、cooldown、bucket、breaker、session budget。
- infra
  - serenity adapter。
  - Discord message/event を core 入力へ変換し、BotAction を API 呼び出しへ変換。

## 依存方向

- `infra -> security -> sandbox/app/domain`
- `app -> domain`
- `domain` は他層へ依存しない

## 禁止事項

- sandbox から Discord API 呼び出し
- domain から serenity 型参照
- handler への判定ロジック逆流
- token を core 以外へ伝搬

## 実装根拠

- [src/lib.rs](../../src/lib.rs)
- [src/infra/discord_handler.rs](../../src/infra/discord_handler.rs)
- [src/security/core_governor.rs](../../src/security/core_governor.rs)
- [src/sandbox/host.rs](../../src/sandbox/host.rs)
