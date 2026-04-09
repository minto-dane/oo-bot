# Security Controls

## 目的

コードに実装された防御制御を、監査可能な形で列挙します。

## 制御一覧

### 1. Secret boundary

- token 読み込み: [src/main.rs](../../src/main.rs)
- token 検証: `validate_discord_token`
- token 非露出: logging で token を出力しない

### 2. Capability separation

- analyzer は `ActionProposal` のみ返す
- Discord API 呼び出しは handler のみ

根拠:

- [src/sandbox/abi.rs](../../src/sandbox/abi.rs)
- [src/infra/discord_handler.rs](../../src/infra/discord_handler.rs)

### 3. Runtime governor

- duplicate suppression
- cooldown (user/channel/guild/global)
- token bucket
- breaker (401/403/429)
- mode transitions
- suspicious input classifier
- send/action caps

根拠: [src/security/core_governor.rs](../../src/security/core_governor.rs)

### 4. Sandbox resource guard

- fuel limit
- memory/table/instance limits
- ABI mismatch reject

根拠: [src/sandbox/host.rs](../../src/sandbox/host.rs)

### 5. Supply-chain/quality gate

- fmt/clippy/tests/nextest/coverage
- audit/deny/geiger/semgrep
- Dependabot version updates / alerts / security updates
- heavy: miri/fuzz/mutation/bench

根拠:

- [.github/workflows/ci.yml](../../.github/workflows/ci.yml)
- [.github/workflows/security.yml](../../.github/workflows/security.yml)
- [.github/dependabot.yml](../../.github/dependabot.yml)

## 制御の限界

- Discord API 全障害は制御不能
- confusable 完全対策は未実装
- process-level isolation はインフラ依存
