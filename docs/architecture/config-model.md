# Configuration Model

## 目的

設定の責務境界と検証ポリシーを定義します。

## 読み込み責務

- main が `oo-bot.yaml` を読み込み、型変換・妥当性検証を実施
- `OO_CONFIG_PATH` を指定した場合はその YAML を使う
- config 不在時は sample YAML から初期ファイルを生成する
- invalid 値は `StartupError::InvalidEnv` で起動失敗
- 未指定キーの default は埋め込み sample YAML 由来で補完する

実装: [src/main.rs](../../src/main.rs)

## 構成要素

- BotConfig
  - 絵文字・special phrase・count/send cap
- RuntimeProtectionConfig
  - cooldown/bucket/breaker/allow-deny/suspicious/mode
- SandboxConfig
  - fuel/memory/table/instance
- Session budget
  - total/remaining/reset_after/low watermark

## 変換規則

- bool: `1,true,yes,on` / `0,false,no,off`
- list: comma-separated u64
- mode: `normal|observe-only|react-only|audit-only|full-disable`

## 安全側フォールバック

- パース不能値: 起動停止
- 未設定値: YAML default を採用
- emergency kill switch: 強制 full disable

詳細キー一覧は [reference/config-reference.md](../reference/config-reference.md) を参照。
