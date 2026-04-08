# Local Setup

## 目的

新規参加者が最短でローカル再現環境を作るための手順です。

## 前提

- Rust stable
- Git
- (任意) just

## 手順

```bash
git clone <repo>
cd oo-bot
cp env.example .env
```

`.env` には最低限 `DISCORD_TOKEN` を設定します。

## 推奨ツール導入

```bash
./scripts/bootstrap_security_tools.sh
```

## ビルド確認

```bash
cargo check --workspace --all-features
```

## ローカル検証（Discord 不要）

```bash
cargo run --bin replay -- tests/fixtures/replay
cargo test --workspace --all-features
```

## よくある失敗

- tool 未導入: `cargo-nextest` など不足
- env 不正: bool/list/mode の形式違反
- generated DB 差分: `cargo xtask generate` 未実行
