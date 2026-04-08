# ADR-0003: runtime protection 境界を trusted core に集中

- Status: Accepted
- Date: 2026-04-07

## Context

Discord token を持つ領域を最小化し、解析失敗が送信暴走に直結しない構造が必要だった。

## Decision

token と outbound capability を trusted core に集中し、
解析器は ActionProposal のみ返す sandbox へ分離する。

## Consequences

- 利点: capability 分離が明確
- 利点: mode/suppress_reason による縮退制御が集中
- 欠点: 変換段（proposal -> action）の設計負荷が増える
