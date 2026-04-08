# Error Reference

## 目的

主要エラー型と運用時の解釈を定義します。

## StartupError

実装: [src/main.rs](../../src/main.rs)

- MissingEnv(name)
  - 必須 env 未設定
- InvalidEnv(name)
  - env 型/値が不正
- SandboxInit(msg)
  - Wasmtime 初期化失敗
- ClientBuild(msg)
  - serenity client 構築失敗

## AnalyzerError

実装: [src/sandbox/abi.rs](../../src/sandbox/abi.rs)

- AbiMismatch
- Trap
- ResourceLimit
- Timeout
- InvalidWire

## ReplayError

実装: [src/app/replay.rs](../../src/app/replay.rs)

- ReadFixture
- ParseFixture

## suppress_reason（準エラー分類）

- 送信を抑止した理由を示す運用上の重要フィールド
- 詳細: [reference/replay-fixture-format.md](replay-fixture-format.md)

## 切り分け順

1. 起動失敗か runtime failure か
2. AnalyzerError か Discord HTTP error か
3. suppress_reason 由来の正常抑止か
