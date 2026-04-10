# Config Reference

## 目的

strict YAML 設定のキーと検証規則を定義します。

## 読み込みモデル

- 起動時に `OO_CONFIG_PATH` (未指定時は `config/oo-bot.yaml`) を 1 回読み込みます。
- config が存在しない場合は、チェックイン済み `config/oo-bot.yaml` を基に初期 YAML を生成します。
- unknown key は `deny_unknown_fields` により拒否され、起動失敗します。
- hot reload はありません。
- detached signature を設定した場合、検証失敗は起動失敗します。
- `oo-bot config setup` / `oo-bot tui --page setup` の保存先も同じ YAML です。

## defaults と source of truth

`oo-bot.yaml` が唯一の source of truth です。

- 起動・TUI 編集・初回 bootstrap は同じ YAML schema を使います。
- `src/config.rs` は schema / validation / YAML 読み書きの責務を持ちます。
- `BotConfig::default()` / `DetectorPolicy::default()` / `RuntimeProtectionConfig::default()` は埋め込み済みの sample YAML を参照します。
- `config/oo-bot.yaml` は bootstrap 用 sample であり、初回起動時に生成される YAML の元になります。

## detector

- backend: `morphological_reading` のみ
- `target_readings`: 空不可
- `literal_sequence_patterns`: 空可
- `special_phrases`: 空不可

`morphological_reading` は Lindera (`embedded://ipadic`) を使用し、
`details()[7]` / `details()[8]` / surface を正規化して判定します。

## bot

- `stamp_text`: 1..128 文字
- `send_template`: 1..512 文字
- `reaction.emoji_id`: 非 0
- `reaction.emoji_name`: 1..64 文字
- `max_count_cap`: 1..=4096
- `max_send_chars`: 1..=8000
- `action_policy`: `react_or_send` / `react_only` / `no_outbound`

許可プレースホルダ:

- `${count}`
- `${stamp}`
- `${matched_backend}`
- `${matched_reading}`
- `${action_kind}`

未定義プレースホルダを含むテンプレートは起動失敗です。

## runtime / audit / diagnostics / integrity

- `runtime`: `RuntimeProtectionConfig` を strict で読み込み
- `audit.export_max_rows`: 1..=1,000,000
- `audit.query_max_rows`: 1..=1,000,000
- `diagnostics.audit_verify_max_rows`: 1..=100,000
- `integrity.config_signature`: 任意
- `integrity.pseudo_id_hmac_key_env`: 任意

## 環境変数

運用で必須または主要な環境変数:

- `DISCORD_TOKEN`
- `OO_CONFIG_PATH` (任意、config パス上書き)
- `OO_PSEUDO_ID_HMAC_KEY` (pseudo-id 有効化時)
- `OO_SANDBOX_*` / `OO_SESSION_BUDGET_*` (runtime 上書き用)

## 設定更新

- 共通の初期設定は `cargo run --bin oo-bot -- config setup` で TUI から変更できます。
- dashboard / diagnostics / audit をまとめて見る場合は `cargo run --bin oo-bot -- tui` を使います。
- 高度な設定は `oo-bot.yaml` を直接編集します。
- TUI / CLI で保存した変更は `OO_CONFIG_PATH` が指す YAML に反映されます。
- `integrity.config_signature` を使う場合、保存時に署名も再生成されます。

## 参照

- [config/oo-bot.yaml](../../config/oo-bot.yaml)
- [src/config.rs](../../src/config.rs)
- [src/domain/detector.rs](../../src/domain/detector.rs)
