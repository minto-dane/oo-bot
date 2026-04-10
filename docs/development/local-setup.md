# Local Setup

## 目的

新規参加者が最短でローカル再現環境を作るための手順です。

## 前提

- Rust stable
- Git
- (任意) just
- (任意) Nix

## 手順

```bash
git clone <repo>
cd oo-bot
cp env.example .env
cargo run --bin oo-bot -- config init
```

`.env` には最低限 `DISCORD_TOKEN` を設定します。
初期値を調整したい場合は `cargo run --bin oo-bot -- config setup` または `cargo run --bin oo-bot -- tui --page setup` を使います。
初回起動は `cargo run --bin oo-bot` を実行します。
bot 起動後は別シェルから `cargo run --bin oo-bot -- tui` で何度でも TUI に入り直せます。
停止は `cargo run --bin oo-bot -- control stop` または TUI の停止導線を使います。

## 推奨ツール導入

Nix を使う場合:

```bash
nix develop
```

非 Nix 環境の場合:

```bash
./scripts/bootstrap_security_tools.sh
```

Nix shell の詳細は [nix-dev-shell.md](nix-dev-shell.md) を参照してください。

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
- config bootstrap/render 差分: `cargo test --test defaults_canonical --all-features` 未実行
