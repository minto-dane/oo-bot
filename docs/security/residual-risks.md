# Residual Risks

## 目的

現時点の設計で残るリスクを明示し、過信を防ぎます。

## 残留リスク一覧

1. Discord platform 障害
- Bot 単体では回避不能。

2. confusable 完全対策の未実装
- NFKC と明示ルール外の視覚同形は未対応。

3. 依存ライブラリ脆弱性
- CI gate で検出はするがゼロにはできない。

4. 運用誤設定
- invalid env は起動失敗するが、意図しない値は動作変更を引き起こす。

5. 観測基盤の限定
- メトリクスは構造体として保持されるが、外部 exporter は未実装。

## 低減策

- 定期 security-heavy workflow 実行
- replay/fault-injection をリリース前に必須化
- config change レビューの二重化

## 参照

- [security/threat-model.md](threat-model.md)
- [operations/maintenance.md](../operations/maintenance.md)
- [operations/troubleshooting.md](../operations/troubleshooting.md)
