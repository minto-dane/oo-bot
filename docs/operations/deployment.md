# Deployment Procedure

## 目的

このシステムを安全にデプロイするための標準手順を定義します。

## 前提

- Rust toolchain が導入済み
- 秘密情報は Secret Manager 管理

## デプロイ前チェック

1. `cargo test --test defaults_canonical --all-features`
2. `cargo test --workspace --all-features`
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
4. `cargo run --bin replay -- tests/fixtures/replay`
5. `just runtime-smoke`

## 本番設定

- 必須: `DISCORD_TOKEN`
- 推奨: `OO_MODE_OVERRIDE` は空
- 緊急停止: `OO_EMERGENCY_KILL_SWITCH=true`
- 設定生成は最初に `config init`（`cargo run --bin oo-bot -- config init`）を実行する
- 追加設定や環境依存の調整が必要な場合は `config setup`（`cargo run --bin oo-bot -- config setup`）を実行する
- `config setup` の実行タイミング例:
	- 初期化後にデフォルト値を変更したいとき
	- 運用環境に合わせて接続情報・監査パスなどを更新したいとき

詳細は [reference/env-reference.md](../reference/env-reference.md)

## 起動

```bash
cargo run --bin oo-bot -- run
```

## 起動後確認

- `bot is connected` ログ
- governor decision ログが出力される
- `mode` が想定値

## ロールバック

1. 前バージョン binary へ切り戻し
2. config bootstrap/render 経路が壊れていないことを `cargo test --test defaults_canonical --all-features` で確認
3. replay smoke 再実行
