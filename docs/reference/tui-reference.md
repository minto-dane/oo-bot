# TUI Reference

## 目的

operator TUI の画面構成、操作、停止導線、安全制約を定義します。

## 起動コマンド

- `cargo run --bin oo-bot -- tui`
- `cargo run --bin oo-bot -- tui --page dashboard`
- `cargo run --bin oo-bot -- tui --page setup`
- `cargo run --bin oo-bot -- tui --page diagnostics`
- `cargo run --bin oo-bot -- tui --page audit`
- `cargo run --bin oo-bot -- config setup`
- `cargo run --bin oo-bot -- audit tui`

`tui` / `config setup` / `audit tui` は bot 本体とは別プロセスです。
TUI を閉じても、別プロセスで稼働中の bot は継続します。

## 画面

- Welcome
  - 最初に `WELCOME TO OO DETECTION / RESPONSE ANALYZER` を表示
  - English / 日本語を選択
- Dashboard
  - config fingerprint
  - detector backend
  - active LSM
  - hardening status
  - bot runtime 状態
  - control socket 状態
- Setup
  - YAML 正本の preview
  - strict schema validation
  - canonical defaults の適用
  - 署名設定
- Diagnostics
  - self-check
  - confinement / hardening
  - integrity / dependency snapshot
- Audit
  - read-only SQLite 参照
  - 検索 / 並び替え / mode filter

## 共通キー

- `1` dashboard
- `2` setup
- `3` diagnostics
- `4` audit
- `l` 言語切替
- `R` 実行時スナップショット再読込
- `x` bot 停止確認
- `q` TUI 終了

`x` は 2 段階です。
1 回目で確認状態に入り、2 回目で bot へ停止コマンドを送信します。
`Esc` または別キー入力で確認状態は解除されます。

## setup キー

- `←` / `→` ページ切替
- `↑` / `↓` 項目選択
- `Enter` 既定値適用
- `e` カスタム入力
- `Space` 値切替
- `p` preview へ移動
- `s` preview 画面で保存

## audit キー

- `/` 検索
- `o` 並び替え
- `m` mode filter
- `r` 監査一覧再読込

## 停止導線

- TUI の `q` は TUI だけを終了
- TUI の `x` は bot へ明示 stop command を送信
- CLI からも `cargo run --bin oo-bot -- control stop` を実行可能

## 安全制約

- secrets は表示しない
- raw identifiers は表示しない
- raw message content は表示しない
- audit DB は read-only 接続
- 表示件数は cap される
- heavy query は実行しない
- TUI から send / react action は発火しない
- TUI から可能な bot 制御は local runtime control socket 経由の `status` / `stop` のみ

## 言語と文言

- operator 文言は `config/i18n/operator_tui.yaml` が正本
- code から直接 user-facing copy を増やさない
- 新規文言は英日両方を同時に追加する

関連:

- [reference/cli-reference.md](cli-reference.md)
- [operations/service-control.md](../operations/service-control.md)
- [reference/config-reference.md](config-reference.md)
