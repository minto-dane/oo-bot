# Sandboxing Design

## 目的

解析器を capability 分離し、trusted core から隔離する理由と実装を定義します。

## 採用技術

- Wasmtime 43.0.0
- WAT guest (inline module)
- ABI version handshake (`SANDBOX_ABI_VERSION`)

実装:

- [src/sandbox/abi.rs](../../src/sandbox/abi.rs)
- [src/sandbox/host.rs](../../src/sandbox/host.rs)

## 入出力境界

入力:

- content bytes
- kanji_count
- special_phrase_hit

出力:

- `ActionProposal` のみ

非提供 capability:

- network
- filesystem
- env
- wall clock
- Discord token

## 資源制御

- `Config::consume_fuel(true)`
- `StoreLimitsBuilder` による memory/table/instance 制限
- linear memory 超過時 `AnalyzerError::ResourceLimit`
- fuel 枯渇時 `AnalyzerError::Timeout`

## fuel 採用理由

このシステムは replay/fault-injection で決定的再現性を優先するため、
epoch interruption より fuel を優先しています。

## ABI 互換制御

- host 起動時に `abi_version` を取得
- 不一致は `AnalyzerError::AbiMismatch`
- core は fail-safe で送信しない
