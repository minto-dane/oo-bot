# Troubleshooting

## 目的

運用中の典型障害に対する一次切り分け手順を定義します。

## 症状別手順

### 1. 起動失敗

- `StartupError::MissingEnv`:
  - `DISCORD_TOKEN` 未設定
- `StartupError::InvalidEnv`:
  - 環境変数型不正、mode 文字列不正
  - 型幅超過
    - 例: `OO_MAX_ACTIONS_PER_MESSAGE=256`
    - 例: `OO_GLOBAL_RATE_BURST=4294967296`
  - session budget の整合性不正
    - 例: `OO_SESSION_BUDGET_REMAINING > OO_SESSION_BUDGET_TOTAL`
- `StartupError::SandboxInit`:
  - Wasmtime 初期化失敗

対応:

1. [reference/env-reference.md](../reference/env-reference.md) を確認
2. `cargo run` の stderr を確認
3. `env.example` と見比べて、値の型と単位を確認

### 2. 反応しない（Noop 多発）

確認順:

1. `author_is_bot` 由来の self-trigger でないか
2. allow/deny で落ちていないか
3. duplicate/cooldown で抑止されていないか
4. mode が observe/audit/full_disable でないか
5. suspicious 判定で落ちていないか

見るべきログ項目:

- `suppress_reason`
- `mode`
- `analyzer_result`
- `content_len`

### 3. 429 連発

- breaker open の可能性
- mode が observe_only へ遷移しているか確認
- `record_http_status(429)` が `OO_BREAKER_THRESHOLD` 回以上、breaker window 内で蓄積していないか確認
- recovery には `OO_BREAKER_OPEN_MS` 経過が必要

### 4. replay 失敗

- `expected_suppress_reason` 不一致を確認
- fixture の `preserve_state` が妥当か確認
- ほぼ全部 `Cooldown` で落ちる場合:
  - harness が state をケース間で共有しすぎていないか確認
  - replay CLI と test harness が同じ core builder を使っているか確認
- trap/timeout 系 fixture が再現しない場合:
  - `[[sandbox_trap]]` / `[[sandbox_timeout]]` を含む入力になっているか確認
  - replay 専用 analyzer injection が有効か確認

### 5. TUI を閉じると bot も止まると思っていた

- `oo-bot tui` は bot 本体とは別プロセス
- `q` は TUI だけを終了
- bot 停止は `control stop` または TUI の `x` 停止導線を使う

### 6. `control status` / `control stop` が失敗する

確認順:

1. bot 本体が実行中か
2. `OO_CONFIG_PATH` が bot 本体と一致しているか
3. control socket が存在するか
4. socket への権限があるか

見るべき点:

- `/run/oo-bot/control.sock`
- `$XDG_RUNTIME_DIR/oo-bot/control.sock`
- `/tmp/oo-bot-control-*.sock`
- `cargo run --bin oo-bot -- control status`

### 7. `Ctrl+C` で bot が止まらない

- 現行実装では `SIGINT` は accidental stop 防止のため停止に使わない
- 明示停止は `control stop` または systemd `stop` を使う
- service manager からの `SIGTERM` は graceful stop として受け付ける

## 再現コマンド

```bash
cargo run --bin replay -- tests/fixtures/replay
cargo test --test replay_harness --test replay_suppress_reason_regression --all-features
cargo test --test fault_injection --all-features
```
