# Configuration Model

## 目的

設定の責務境界と検証ポリシーを定義します。

## 読み込み責務

- main が環境変数を読み込み、型変換・妥当性検証を実施
- invalid 値は `StartupError::InvalidEnv` で起動失敗
- 未設定値はコードデフォルトを適用

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
- 未設定値: デフォルト採用
- emergency kill switch: 強制 full disable

詳細キー一覧は [reference/config-reference.md](../reference/config-reference.md) を参照。
