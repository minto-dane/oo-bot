# Unicode Normalization Specification

## 目的

漢字読み判定の正規化規約を定義し、辞書生成とランタイム判定の整合を保証します。

## 適用範囲

- ランタイムの `reading_normalizer` 実装
- detector の token reading 正規化

## 正規化手順

1. NFKC
2. trim
3. 読み記号/空白除去
   - `. - ‐ ‑ – — ― ・ ･ 空白 タブ 改行`
4. カタカナをひらがなへ変換（U+30A1..U+30F6 から 0x60 減算）
5. 再度 NFKC

実装:

- [src/domain/reading_normalizer.rs](../../src/domain/reading_normalizer.rs)
- [src/domain/detector.rs](../../src/domain/detector.rs)

## 仕様上の意図

- 辞書読み記法差分を吸収
- 半角カナを同一視
- 判定を deterministic に維持

## 既知の限界

- confusable 全対応はしない
- 歴史的仮名遣い・語彙文脈は解決しない
- grapheme cluster 単位ではなく Unicode scalar 値単位

## テスト観点

- idempotence
- 半角カナの吸収
- 記号除去

検証: [tests/property_oo.rs](../../tests/property_oo.rs)
