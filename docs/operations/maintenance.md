# Maintenance

## 目的

定常保守作業を手順化し、運用品質を均一化します。

## 定期作業

- weekly
  - `security-heavy` workflow 結果確認
  - dependency advisory の確認
- release 前
  - `just ci-local`
  - replay/fault-inject 実行

## 設定保守

1. `config/oo-bot.yaml` の変更を確認
2. `cargo test --test defaults_canonical --all-features`
3. replay / runtime integration の結果を確認

## テスト資産保守

- 新しいバグは replay fixture と suppress_reason 期待で固定
- property/fuzz corpus を更新

## 文書保守

- 実装変更と同一 PR で docs 更新
- 仕様変更時は ADR 追加
