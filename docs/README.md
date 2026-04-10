# docs/

このディレクトリは、このリポジトリの設計・仕様・運用・監査の一次資料です。

## 対象読者

- 新規参加エンジニア
- SRE/運用担当
- セキュリティレビュー担当
- 監査担当

## 先に読む順序

1. [docs/index.md](index.md)
2. [docs/overview/architecture.md](overview/architecture.md)
3. [docs/product/behavior-spec.md](product/behavior-spec.md)
4. [docs/architecture/runtime-protection.md](architecture/runtime-protection.md)
5. [docs/operations/deployment.md](operations/deployment.md)

## Source of Truth

- 実行挙動: [src/](../src)
- 設定 source of truth: [config/oo-bot.yaml](../config/oo-bot.yaml)
- 設定 schema / validation / bootstrap: [src/config.rs](../src/config.rs)
- CI/CD: [.github/workflows/ci.yml](../.github/workflows/ci.yml), [.github/workflows/security.yml](../.github/workflows/security.yml)
- ローカル実行導線: [Justfile](../Justfile), [flake.nix](../flake.nix)

ドキュメントが実装と矛盾する場合は、上記 source of truth を優先し、ドキュメントを更新します。
