# Deployment Procedure

## 目的

このシステムを安全にデプロイするための標準手順を定義します。

## 前提

- Rust toolchain が導入済み
- 秘密情報は Secret Manager 管理
- `data/vendor/kanjidic2.xml.gz` が想定バージョン

## デプロイ前チェック

1. `cargo xtask verify`
2. `cargo test --workspace --all-features`
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
4. `cargo run --bin replay -- tests/fixtures/replay`
5. `just runtime-smoke`

## 本番設定

- 必須: `DISCORD_TOKEN`
- 推奨: `OO_MODE_OVERRIDE` は空
- 緊急停止: `OO_EMERGENCY_KILL_SWITCH=true`

詳細は [reference/env-reference.md](../reference/env-reference.md)

## 起動

```bash
cargo run
```

## 起動後確認

- `bot is connected` ログ
- governor decision ログが出力される
- `mode` が想定値

## ロールバック

1. 前バージョン binary へ切り戻し
2. 生成物が一致することを `cargo xtask verify` で確認
3. replay smoke 再実行
