# Secrets Handling

## 目的

token を中心とした秘密情報の取り扱いを定義します。

## 現行仕様

- 必須 secret: `DISCORD_TOKEN`
- 読み込み元: 環境変数
- ローカル開発補助: `.env` + `dotenvy`

実装: [src/main.rs](../../src/main.rs)

## 保護要件

- token をログに出力しない
- token を sandbox に渡さない
- token を replay fixture に含めない

## 運用ルール

- 本番では Secret Manager を使用
- `.env` は開発用途限定
- token rotation は [operations/rotation-and-recovery.md](../operations/rotation-and-recovery.md) を参照

## 失効時挙動

- token 欠落/不正形式は起動失敗
- 実行中 401/403 増加時は breaker で outbound 抑止

## 監査証跡

- 起動ログ（token 値なし）
- mode transition ログ
- CI security gate 実行ログ
