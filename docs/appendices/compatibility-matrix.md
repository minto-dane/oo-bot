# Compatibility Matrix

## 目的

主要機能と検証手段の対応を一覧化します。

| 項目 | 実装 | テスト | CI |
|---|---|---|---|
| oo non-overlap | domain/oo_counter | property_oo, integration | unit-and-integration-tests |
| special phrase priority | app/analyze_message | analyze_message_integration | unit-and-integration-tests |
| runtime mode gate | security/core_governor | runtime_protection_integration | runtime-protection |
| suppress_reason | security/core_governor + app/replay | replay_suppress_reason_regression | runtime-protection |
| sandbox limits | sandbox/host | fault_injection + host unit | runtime-protection + security-heavy |
| deterministic generation | xtask | generated_db + xtask tests | deterministic-db |
| replay format | app/replay | replay_harness | runtime-protection |
