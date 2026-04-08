# ADR-0004: analyzer sandbox に Wasmtime + fuel を採用

- Status: Accepted
- Date: 2026-04-07

## Context

通常の Rust モジュール分離だけでは capability 強制が弱く、
入力依存の暴走を deterministic に再現制御する必要があった。

## Decision

Wasmtime で analyzer を実行し、`consume_fuel(true)` と `StoreLimits` を採用する。
epoch interruption ではなく fuel を採用する。

## Consequences

- 利点: replay/fault-injection で決定的な timeout 再現が可能
- 利点: memory/table/instance を明示制限
- 欠点: pure analyzer よりオーバーヘッドが増える
