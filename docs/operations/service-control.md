# Service And Process Control

## 目的

bot 本体の稼働継続、停止経路、systemd 運用を定義します。

## プロセスモデル

- `oo-bot run`
  - bot 本体
  - Discord Gateway に接続して常駐
  - local runtime control socket を公開
- `oo-bot tui`
  - operator console
  - bot 本体とは別プロセス
- `oo-bot audit *`
  - read-only 監査 / export CLI
  - bot 本体とは別プロセス

このため、TUI の終了や再入場は bot の稼働に影響しません。

## 明示制御

### 状態確認

```bash
cargo run --bin oo-bot -- control status
```

出力:

- `state`
- `pid`
- `started_at_unix`
- `config_path`
- `config_fingerprint`
- `detector_backend`
- `active_lsm`
- `hardening_status`
- `socket_path`

### 停止

```bash
cargo run --bin oo-bot -- control stop
```

TUI からは `x` を 2 回押して同じ stop command を送信します。

## signal 方針

- `SIGINT`
  - 直ちには停止しない
  - warning を記録し、`control stop` または TUI 停止導線を使うよう促す
- `SIGTERM`
  - 管理者 / service manager からの停止要求として扱う
  - graceful shutdown を開始

これは「対話端末の accidental stop を避ける」と「service manager からの正当な停止を受け付ける」を両立するためです。

## runtime control socket

- Unix domain socket のみ
- remote network listener は持たない
- 既定優先順:
  1. `OO_CONTROL_SOCKET_PATH`
  2. `/run/oo-bot/control.sock`
  3. `$XDG_RUNTIME_DIR/oo-bot/control.sock`
  4. `/tmp/oo-bot-control-<hash>.sock`
- socket は local-only 管理面であり、Discord token や message payload は流さない

## systemd の役割

systemd service は次を担います。

- bot をバックグラウンド常駐させる
- OS 起動時に自動起動する
- crash 時に再起動する
- sandbox / hardening を unit で強制する
- `systemctl start/stop/status/restart` を統一導線にする

本リポジトリの unit は [deploy/systemd/oo-bot.service](../../deploy/systemd/oo-bot.service) にあります。

## systemd 運用

主要項目:

- `ExecStart=/opt/oo-bot/oo-bot run`
- `ExecStop=/opt/oo-bot/oo-bot control stop`
- `Restart=on-failure`
- `ProtectSystem=strict`
- `NoNewPrivileges=true`

代表操作:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now oo-bot
sudo systemctl status oo-bot
sudo systemctl stop oo-bot
sudo systemctl restart oo-bot
```

## 運用上の注意

- `control stop` は同じ設定経路を共有する必要があるため、`OO_CONFIG_PATH` を bot 本体と合わせる
- TUI は監査閲覧・診断・設定編集・停止要求のためのコンソールであり、bot action 実行面ではない
- bot を継続稼働させる本番運用では、前景 `cargo run` より systemd service を優先する

関連:

- [operations/deployment.md](deployment.md)
- [operations/troubleshooting.md](troubleshooting.md)
- [reference/tui-reference.md](../reference/tui-reference.md)
