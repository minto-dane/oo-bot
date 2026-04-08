# Kanji Database Specification

## 目的

単漢字判定に使う生成 DB の仕様を定義します。

## データソース

- 入力: `data/vendor/kanjidic2.xml.gz`
- 由来: EDRDG KANJIDIC2
- 参照: [data/vendor/README.md](../../data/vendor/README.md)

## 対象読み

- `ja_kun`
- `nanori`
- `ja_on`（feature/flag で有効化）

条件: 正規化後に `おお` を含む場合にヒット。

## 出力

- Rust 生成物: [src/generated/kanji_oo_db.rs](../../src/generated/kanji_oo_db.rs)
- メタデータ: [data/generated/kanji_oo_db_meta.json](../../data/generated/kanji_oo_db_meta.json)

## deterministic 性

`cargo xtask generate` 後に CI で差分検査します。

- job: `deterministic-db`
- workflow: [.github/workflows/ci.yml](../../.github/workflows/ci.yml)

## 非目標

- 熟語読み
- 外部 API によるオンライン辞書参照

## 失敗時

- XML parse 失敗
- 0 character 抽出
- 生成物検証不一致

実装: [xtask/src/main.rs](../../xtask/src/main.rs)
