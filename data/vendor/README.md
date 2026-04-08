# Vendored Dictionary Data

## Source

- Dataset: KANJIDIC2
- Provider: Electronic Dictionary Research and Development Group (EDRDG)
- URL: https://www.edrdg.org/kanjidic/kanjidic2.xml.gz

## Files

- `kanjidic2.xml.gz`: vendored upstream source used by `xtask` generation.

## Usage in this repository

- Runtime reads only generated Rust source (`src/generated/kanji_oo_db.rs`).
- `kanjidic2.xml.gz` is used only during generation (`cargo xtask generate`).

## Update procedure

1. Replace `kanjidic2.xml.gz` with a newer upstream file.
2. Run `cargo xtask generate`.
3. Run `cargo xtask verify` and tests.
4. Review metadata diff in `data/generated/kanji_oo_db_meta.json`.

## License and attribution

KANJIDIC2 licensing/terms are defined by EDRDG.  
When redistributing this repository, keep this attribution and verify the current KANJIDIC2 terms.
