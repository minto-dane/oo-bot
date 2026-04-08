# Config Reference

## 目的

設定キーの意味と安全上の含意を、運用・監査で参照できる形に整理します。

## パース規則

- 整数: `u64` / `usize` パース
- 真偽値: `1,true,yes,on` / `0,false,no,off`
- ID リスト: `1,2,3` の comma-separated u64
- mode override: `normal|observe-only|observe_only|react-only|react_only|audit-only|audit_only|full-disable|full_disable`

不正値は `StartupError::InvalidEnv` で起動失敗します。

## Bot behavior keys

| Key | Type | Default | Allowed | Security implications | Operational implications | Example | Invalid value behavior |
|---|---|---|---|---|---|---|---|
| OO_SPECIAL_PHRASE | string | これはおお | 任意 UTF-8 | 過剰一致で送信増加の可能性 | 機能要件に直結 | OO_SPECIAL_PHRASE=これはおお | 起動成功（文字列として受理） |
| OO_EMOJI_ID | u64 | 1489695886773587978 | >0 推奨 | 無効 ID だと失敗応答増加 | 反応失敗率増加 | OO_EMOJI_ID=123 | parse 失敗で起動失敗 |
| OO_EMOJI_NAME | string | Omilfy | 非空推奨 | 空文字は invalid action 化 | react が Noop 化 | OO_EMOJI_NAME=MyEmoji | 起動成功（空は runtime で抑止） |
| OO_EMOJI_ANIMATED | bool | false | true/false 系 | なし | 見た目のみ | OO_EMOJI_ANIMATED=true | parse 失敗で起動失敗 |
| OO_STAMP | string | <:Omilfy:...> | 任意 | 文字列長増加は send 長制限に影響 | 出力内容変更 | OO_STAMP=:o: | 起動成功 |
| OO_MAX_COUNT_CAP | usize | 48 | >=1 推奨 | 高すぎると送信量増加 | レスポンス量増加 | OO_MAX_COUNT_CAP=24 | parse 失敗で起動失敗 |
| OO_MAX_SEND_CHARS | usize | 1900 | 1..2000 推奨 | 高すぎると API 制約接近 | 切り詰め挙動に影響 | OO_MAX_SEND_CHARS=1500 | parse 失敗で起動失敗 |

## Mode / access keys

| Key | Type | Default | Allowed | Security implications | Operational implications | Example | Invalid value behavior |
|---|---|---|---|---|---|---|---|
| OO_MODE_OVERRIDE | mode string | unset | runtime mode 値 | 不適切 override は停止/誤動作を誘発 | 手動縮退に利用 | OO_MODE_OVERRIDE=observe-only | 不正文字列は起動失敗 |
| OO_EMERGENCY_KILL_SWITCH | bool | false | bool | true で強制停止 | 緊急時停止手段 | OO_EMERGENCY_KILL_SWITCH=true | parse 失敗で起動失敗 |
| OO_ALLOW_GUILD_IDS | list<u64> | empty | comma-separated | 許可範囲を狭める | 誤設定で無応答化 | OO_ALLOW_GUILD_IDS=1,2 | parse 失敗で起動失敗 |
| OO_DENY_GUILD_IDS | list<u64> | empty | comma-separated | deny 優先でブロック | 誤設定で応答停止 | OO_DENY_GUILD_IDS=3 | parse 失敗で起動失敗 |
| OO_ALLOW_CHANNEL_IDS | list<u64> | empty | comma-separated | channel 制限 | 実験導入時に有効 | OO_ALLOW_CHANNEL_IDS=10 | parse 失敗で起動失敗 |
| OO_DENY_CHANNEL_IDS | list<u64> | empty | comma-separated | 即時ブロック | トラブル隔離に有効 | OO_DENY_CHANNEL_IDS=20 | parse 失敗で起動失敗 |

## Guardrail keys

| Key | Type | Default | Allowed | Security implications | Operational implications | Example | Invalid value behavior |
|---|---|---|---|---|---|---|---|
| OO_DUPLICATE_TTL_MS | u64 | 180000 | >=0 | 低すぎると再配送漏れ | duplicate 抑止期間 | OO_DUPLICATE_TTL_MS=60000 | parse 失敗で起動失敗 |
| OO_DUPLICATE_CACHE_CAP | usize | 8192 | >=64 推奨 | 低すぎると suppress 漏れ | メモリ/精度トレードオフ | OO_DUPLICATE_CACHE_CAP=4096 | parse 失敗で起動失敗 |
| OO_COOLDOWN_USER_MS | u64 | 900 | >=0 | 小さいと連投許容 | user 粒度抑止 | OO_COOLDOWN_USER_MS=1200 | parse 失敗で起動失敗 |
| OO_COOLDOWN_CHANNEL_MS | u64 | 400 | >=0 | 小さいと channel spam 余地 | channel 粒度抑止 | OO_COOLDOWN_CHANNEL_MS=800 | parse 失敗で起動失敗 |
| OO_COOLDOWN_GUILD_MS | u64 | 250 | >=0 | 小さいと guild burst 余地 | guild 粒度抑止 | OO_COOLDOWN_GUILD_MS=500 | parse 失敗で起動失敗 |
| OO_COOLDOWN_GLOBAL_MS | u64 | 100 | >=0 | 低すぎると全体 burst 増加 | 全体抑止 | OO_COOLDOWN_GLOBAL_MS=300 | parse 失敗で起動失敗 |
| OO_GLOBAL_RATE_PER_SEC | f64 | 20.0 | >0 | 低すぎると過抑止/高すぎると連投 | outbound スループット | OO_GLOBAL_RATE_PER_SEC=15 | parse 失敗で起動失敗 |
| OO_GLOBAL_RATE_BURST | u32 | 30 | >=1 | 高すぎると瞬間連投 | burst 許容量 | OO_GLOBAL_RATE_BURST=20 | parse 失敗で起動失敗 |
| OO_MAX_ACTIONS_PER_MESSAGE | u8 | 1 | 0..255 | 0 は常時抑止 | 緊急抑止に利用可 | OO_MAX_ACTIONS_PER_MESSAGE=1 | parse 失敗で起動失敗 |

## Suspicious / breaker keys

| Key | Type | Default | Allowed | Security implications | Operational implications | Example | Invalid value behavior |
|---|---|---|---|---|---|---|---|
| OO_LONG_MESSAGE_SOFT_CHARS | usize | 2000 | >=1 | 低すぎると過剰縮退 | send->react 変換閾値 | OO_LONG_MESSAGE_SOFT_CHARS=3000 | parse 失敗で起動失敗 |
| OO_LONG_MESSAGE_HARD_CHARS | usize | 8000 | >=soft 推奨 | 低すぎると過抑止 | hard drop 閾値 | OO_LONG_MESSAGE_HARD_CHARS=12000 | parse 失敗で起動失敗 |
| OO_SUSPICIOUS_REPETITION_THRESHOLD | usize | 256 | >=1 | 低すぎると過検出 | 反復検知閾値 | OO_SUSPICIOUS_REPETITION_THRESHOLD=300 | parse 失敗で起動失敗 |
| OO_BREAKER_WINDOW_MS | u64 | 60000 | >=1 | 窓が短いと検知鈍化 | breaker 感度 | OO_BREAKER_WINDOW_MS=30000 | parse 失敗で起動失敗 |
| OO_BREAKER_THRESHOLD | usize | 64 | >=1 | 低すぎると過剰停止 | breaker 発火点 | OO_BREAKER_THRESHOLD=20 | parse 失敗で起動失敗 |
| OO_BREAKER_OPEN_MS | u64 | 120000 | >=1 | 長すぎると停止長期化 | 開放時間 | OO_BREAKER_OPEN_MS=60000 | parse 失敗で起動失敗 |

## Sandbox / session keys

| Key | Type | Default | Allowed | Security implications | Operational implications | Example | Invalid value behavior |
|---|---|---|---|---|---|---|---|
| OO_SANDBOX_FUEL_LIMIT | u64 | 50000 | >=1 | 低すぎると timeout 多発 | 解析コスト上限 | OO_SANDBOX_FUEL_LIMIT=80000 | parse 失敗で起動失敗 |
| OO_SANDBOX_MEMORY_BYTES | usize | 65536 | >=1 | 低すぎると resource limit 多発 | 入力許容長に影響 | OO_SANDBOX_MEMORY_BYTES=131072 | parse 失敗で起動失敗 |
| OO_SANDBOX_TABLE_ELEMENTS | usize | 64 | >=1 | 通常は影響小 | table 制限 | OO_SANDBOX_TABLE_ELEMENTS=64 | parse 失敗で起動失敗 |
| OO_SANDBOX_INSTANCE_LIMIT | usize | 4 | >=1 | 制限緩和で資源圧迫余地 | 同時 instance 制限 | OO_SANDBOX_INSTANCE_LIMIT=4 | parse 失敗で起動失敗 |
| OO_SANDBOX_FAILURE_WINDOW_MS | u64 | 30000 | >=1 | 小さすぎると spike 見落とし | audit_only 遷移窓 | OO_SANDBOX_FAILURE_WINDOW_MS=45000 | parse 失敗で起動失敗 |
| OO_SANDBOX_FAILURE_THRESHOLD | usize | 10 | >=1 | 低すぎると過剰 audit | 失敗許容度 | OO_SANDBOX_FAILURE_THRESHOLD=5 | parse 失敗で起動失敗 |
| OO_SESSION_BUDGET_TOTAL | u32 | 1000 | >=1 | 参照値 | 運用判断に使用 | OO_SESSION_BUDGET_TOTAL=1000 | parse 失敗で起動失敗 |
| OO_SESSION_BUDGET_REMAINING | u32 | 1000 | 0..total | 低値で react_only へ | 縮退起動判定 | OO_SESSION_BUDGET_REMAINING=12 | parse 失敗で起動失敗 |
| OO_SESSION_BUDGET_RESET_AFTER | u64 | 86400 | >=1 | 参照値 | 復旧判断に使用 | OO_SESSION_BUDGET_RESET_AFTER=3600 | parse 失敗で起動失敗 |
| OO_SESSION_BUDGET_LOW_WATERMARK | u32 | 5 | >=1 | 高すぎると早期縮退 | react_only 閾値 | OO_SESSION_BUDGET_LOW_WATERMARK=10 | parse 失敗で起動失敗 |

## 参照

- [reference/env-reference.md](env-reference.md)
- [architecture/config-model.md](../architecture/config-model.md)
