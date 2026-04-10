# Environment Variable Reference

## 目的

運用/開発で使用する環境変数を、現行実装に合わせて整理します。

このプロジェクトは YAML 正本 (`config/oo-bot.yaml`) を基本とし、環境変数は最小限です。

## 必須

- DISCORD_TOKEN

`DISCORD_TOKEN` は未設定で起動失敗します。
空文字、`.` を含まない値、極端に短い値も `StartupError::InvalidEnv` になります。

## 設定ファイル導線

- OO_CONFIG_PATH

補足:

- 未指定時は `config/oo-bot.yaml` を使います。
- `OO_CONFIG_PATH` は YAML ファイルの参照先を上書きします。

## 擬似ID HMAC キー

- OO_PSEUDO_ID_HMAC_KEY

補足:

- `integrity.pseudo_id_hmac_key_env` が参照するキーです（既定は上記）。
- 未設定でも起動できますが、監査の pseudo-id は無効になります。

## sandbox

- OO_SANDBOX_MEMORY_BYTES
- OO_SANDBOX_TABLE_ELEMENTS
- OO_SANDBOX_INSTANCE_LIMIT
- OO_SANDBOX_FUEL_LIMIT

補足:

- これらは sandbox 設定の runtime override です。
- parse 失敗は起動失敗になります。
- 既定値:
	- `OO_SANDBOX_MEMORY_BYTES=65536`
	- `OO_SANDBOX_TABLE_ELEMENTS=64`
	- `OO_SANDBOX_INSTANCE_LIMIT=4`
	- `OO_SANDBOX_FUEL_LIMIT=50000`

## session budget

- OO_SESSION_BUDGET_TOTAL
- OO_SESSION_BUDGET_REMAINING
- OO_SESSION_BUDGET_RESET_AFTER

補足:

- `OO_SESSION_BUDGET_TOTAL` / `OO_SESSION_BUDGET_REMAINING` は `u32`
- `OO_SESSION_BUDGET_REMAINING > OO_SESSION_BUDGET_TOTAL` は不正として起動失敗します
- 既定値:
	- `OO_SESSION_BUDGET_TOTAL=1000`
	- `OO_SESSION_BUDGET_RESET_AFTER=86400`
	- `OO_SESSION_BUDGET_REMAINING` 未指定時は `OO_SESSION_BUDGET_TOTAL` と同値

## 旧環境変数について

`OO_MODE_OVERRIDE` などの多数の旧 `OO_*` は現行では参照されません。
runtime policy や detector/bot 設定は YAML (`config/oo-bot.yaml`) 側で管理します。

## 例

```bash
export DISCORD_TOKEN=xxxxx.yyyyy.zzzzz
export OO_CONFIG_PATH=config/oo-bot.yaml
export OO_PSEUDO_ID_HMAC_KEY=CHANGE_ME_TO_RANDOM_32B
export OO_SANDBOX_FUEL_LIMIT=50000
export OO_SESSION_BUDGET_TOTAL=1000
export OO_SESSION_BUDGET_REMAINING=1000
```

詳細は [config-reference.md](config-reference.md) を参照。
