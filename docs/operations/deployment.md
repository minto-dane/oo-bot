# Deployment Procedure

## 目的

このシステムを安全にデプロイするための標準手順を定義します。

## 前提

- Rust toolchain が導入済み
- SQLite CLI と開発ライブラリが導入済み
- ビルド依存 (`pkg-config`, OpenSSL 開発ヘッダ, C toolchain) が導入済み
- 秘密情報は Secret Manager 管理

## 必要コンポーネント

- Rust (`cargo`, `rustc`)
- SQLite (`sqlite3`)
- OpenSSL 開発ヘッダ
- `pkg-config`
- C/C++ ビルドツール (`gcc/clang`, `make`)
- `git`, `curl`, `ca-certificates`

Lindera は `embedded://ipadic` を使うため、追加の外部辞書パッケージは不要です。

## インストール例（主要 package manager）

### Debian / Ubuntu (apt)

```bash
sudo apt-get update
sudo apt-get install -y \
	git curl ca-certificates \
	build-essential pkg-config libssl-dev \
	sqlite3 libsqlite3-dev
```

### Fedora / RHEL 系 (dnf)

```bash
sudo dnf install -y \
	git curl ca-certificates \
	gcc gcc-c++ make pkgconf-pkg-config openssl-devel \
	sqlite sqlite-devel
```

### Arch Linux (pacman)

```bash
sudo pacman -S --needed \
	git curl ca-certificates \
	base-devel pkgconf openssl sqlite
```

### openSUSE (zypper)

```bash
sudo zypper install -y \
	git curl ca-certificates \
	gcc gcc-c++ make pkg-config libopenssl-devel \
	sqlite3 sqlite3-devel
```

### macOS (Homebrew)

```bash
brew install git curl pkg-config openssl@3 sqlite3
```

## Rust 導入（推奨: rustup）

OS に関わらず、Rust は rustup で stable を入れる運用を推奨します。

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
```

ディストリ標準の Rust パッケージを使う場合は、次のコマンドでも導入できます。

- apt: `sudo apt-get install -y rustc cargo`
- dnf: `sudo dnf install -y rust cargo`
- pacman: `sudo pacman -S --needed rust`
- zypper: `sudo zypper install -y rust cargo`
- brew: `brew install rust`

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

systemd 運用時は [service-control.md](service-control.md) の unit を使います。
本番では、前景 `cargo run` より systemd 常駐を優先します。

## 起動後確認

- `bot is connected` ログ
- governor decision ログが出力される
- `mode` が想定値
- `cargo run --bin oo-bot -- control status` が応答する
- `cargo run --bin oo-bot -- tui` から dashboard / diagnostics / audit / stop 導線へ入れる

## ロールバック

1. 前バージョン binary へ切り戻し
2. config bootstrap/render 経路が壊れていないことを `cargo test --test defaults_canonical --all-features` で確認
3. replay smoke 再実行
