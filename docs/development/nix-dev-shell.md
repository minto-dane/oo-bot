# Nix Dev Shell

## 目的

Nix を使って、このリポジトリの開発・監査ツールを再現可能に揃えるための手順です。

この文書は contributor / CI 向けです。Bot を動かすだけの運用者へ Nix を必須要求しません。

## 何が入るか

`nix develop` で入る default shell には次を含めます。

- Rust toolchain
  - `cargo`
  - `rustc`
  - `rustfmt`
  - `clippy`
  - `rust-analyzer`
- 補助 CLI
  - `just`
  - `pkg-config`
  - `openssl`
- 監査・品質ツール
  - `cargo-audit`
  - `cargo-deny`
  - `cargo-nextest`
  - `cargo-llvm-cov`
  - `cargo-geiger`
  - `cargo-hack`
  - `cargo-fuzz`
  - `cargo-mutants`
  - `cargo-machete`
  - `cargo-udeps`
  - `semgrep`

## 基本フロー

```bash
nix develop
cargo test --workspace --all-features
```

shell に入らず、ツール群だけ profile へ追加したい場合:

```bash
nix profile install .#security-tools
```

監査込みの確認は次です。

```bash
just ci-local
```

fixture 回帰だけを見たい場合:

```bash
cargo run --bin replay -- tests/fixtures/replay
```

## 非 Nix 環境との使い分け

- Nix を使う場合
  - contributor / CI の toolchain を揃えやすい
  - 個別の `cargo install` / `pipx install` を避けやすい
- Nix を使わない場合
  - `./scripts/bootstrap_security_tools.sh` を使う
  - 既存の Rust / Python 開発環境に合わせたい人向け

## 注意

- `miri` や nightly 固有検証は、この default shell だけでは完結しない場合があります
- `flake.nix` は toolchain 配布のための導線であり、本番ホストでの常駐要件ではありません
- end user 向けインストール手順として Nix を要求しない方針です
