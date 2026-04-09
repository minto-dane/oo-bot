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
- 設定キー/デフォルト: [src/main.rs](../src/main.rs), [env.example](../env.example)
- CI/CD: [.github/workflows/ci.yml](../.github/workflows/ci.yml), [.github/workflows/security.yml](../.github/workflows/security.yml)
- 生成データ: [src/generated/kanji_oo_db.rs](../src/generated/kanji_oo_db.rs), [data/generated/kanji_oo_db_meta.json](../data/generated/kanji_oo_db_meta.json)
- 生成器: [xtask/src/main.rs](../xtask/src/main.rs)
- ローカル実行導線: [Justfile](../Justfile), [flake.nix](../flake.nix)

ドキュメントが実装と矛盾する場合は、上記 source of truth を優先し、ドキュメントを更新します。
