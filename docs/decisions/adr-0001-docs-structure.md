# ADR-0001: docs 構造の再編

- Status: Accepted
- Date: 2026-04-07

## Context

既存 docs は root に断片的に存在し、責務分離と導線が不足していた。

## Decision

`overview / product / architecture / security / operations / development / reference / decisions / appendices` の階層へ再編し、
`docs/index.md` を索引の source of navigation とする。

## Consequences

- 利点: 監査・運用・開発の読者別導線が明確化
- 欠点: 初期整備コスト増
- 運用: 構造変更時は本 ADR を更新
