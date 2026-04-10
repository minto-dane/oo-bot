# Control Plane Architecture

## 目的

runtime control plane の境界、責務、実装位置を architecture 観点で定義します。

## 対象読者

- 実装者
- 運用設計者
- セキュリティレビュー担当

## スコープ

- local runtime control の通信境界
- command surface と payload の制約
- bot 本体とのプロセス境界

## 非スコープ

- operator TUI の画面仕様とキー操作
- AppArmor/SELinux の導入手順
- systemd の運用手順

## Boundary

- transport は Unix domain socket のみ
- remote network listener は持たない
- command は `status` / `stop` のみ
- payload は runtime metadata に限定し、Discord token や message content は含めない

## Path Resolution

socket path は次の優先順で解決します。

1. `OO_CONTROL_SOCKET_PATH`
2. `/run/oo-bot/control.sock`
3. `$XDG_RUNTIME_DIR/oo-bot/control.sock`
4. `/tmp/oo-bot-control-<hash>.sock`

この順序により、systemd 管理下と開発環境の双方で同一 command surface を維持します。

## Process Roles

- server: `oo-bot run`
  - control socket を bind し、status/stop を受理
- client: `oo-bot control status` / `oo-bot control stop`
  - local socket に接続して command を送信

operator TUI は bot 本体とは別プロセスで、同じ local control plane を利用します。

## Source Of Truth

- [src/control.rs](../../src/control.rs)
- [src/main.rs](../../src/main.rs)

## 関連ドキュメント

- TUI 仕様: [docs/reference/tui-reference.md](../reference/tui-reference.md)
- サービス制御手順: [docs/operations/service-control.md](../operations/service-control.md)
- LSM/hardening 運用: [docs/operations/hardening-and-lsm.md](../operations/hardening-and-lsm.md)