# Code Generation

## 目的

KANJIDIC2 由来データの生成手順と検証方法を定義します。

## コマンド

```bash
cargo xtask generate
cargo xtask verify
```

## 生成器責務

- gzip 解凍
- XML parse
- doctype 除去
- 読み正規化
- codepoint 集約と整列
- Rust source と metadata JSON 出力

実装: [xtask/src/main.rs](../../xtask/src/main.rs)

## 生成物

- [src/generated/kanji_oo_db.rs](../../src/generated/kanji_oo_db.rs)
- [data/generated/kanji_oo_db_meta.json](../../data/generated/kanji_oo_db_meta.json)

## CI 検証

- `deterministic-db` job が差分検査

## 変更時注意

- 正規化ロジック変更時は runtime と xtask の双方を更新
- golden test と generated_db テストを更新
