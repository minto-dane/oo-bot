# ADR-0002: 単漢字読みの source of truth を KANJIDIC2 + generated DB に固定

- Status: Accepted
- Date: 2026-04-07

## Context

手書き配列は更新性・監査性・再現性に問題がある。

## Decision

入力辞書を KANJIDIC2 とし、`xtask` で deterministic 生成した
`src/generated/kanji_oo_db.rs` をランタイム唯一の参照元とする。

## Consequences

- 利点: 生成手順と差分監査が可能
- 利点: ランタイム外部依存が不要
- 欠点: 辞書更新時に regenerate が必要
