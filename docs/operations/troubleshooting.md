# Troubleshooting

## 目的

運用中の典型障害に対する一次切り分け手順を定義します。

## 症状別手順

### 1. 起動失敗

- `StartupError::MissingEnv`:
  - `DISCORD_TOKEN` 未設定
- `StartupError::InvalidEnv`:
  - 環境変数型不正、mode 文字列不正
- `StartupError::SandboxInit`:
  - Wasmtime 初期化失敗

対応:

1. [reference/env-reference.md](../reference/env-reference.md) を確認
2. `cargo run` の stderr を確認

### 2. 反応しない（Noop 多発）

確認順:

1. `author_is_bot` 由来の self-trigger でないか
2. allow/deny で落ちていないか
3. duplicate/cooldown で抑止されていないか
4. mode が observe/audit/full_disable でないか
5. suspicious 判定で落ちていないか

### 3. 429 連発

- breaker open の可能性
- mode が observe_only へ遷移しているか確認

### 4. replay 失敗

- `expected_suppress_reason` 不一致を確認
- fixture の `preserve_state` が妥当か確認

## 再現コマンド

```bash
cargo test --test replay_harness --test replay_suppress_reason_regression --all-features
cargo test --test fault_injection --all-features
```
