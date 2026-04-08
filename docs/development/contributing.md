# Contributing Guide

## 目的

変更の品質基準と提出手順を定義します。

## 基本原則

- 実装と docs を同一 PR で更新
- replay/fault/property のどれで検証したかを PR に記載
- 仕様変更時は ADR を追加

## 必須チェック

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --bin replay -- tests/fixtures/replay
```

## runtime protection 変更時

- suppress_reason 期待値付き fixture を追加
- `replay_suppress_reason_regression` を通す

## docs 変更時

- [reference/docs-conventions.md](../reference/docs-conventions.md) に従う
- [docs/index.md](../index.md) のリンクを更新

## 禁止事項

- token をログ出力する変更
- handler へロジック逆流
- sandbox へ外部 capability 追加
