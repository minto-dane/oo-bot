# Hardening and LSM Operations

## Scope

この文書は次を運用観点で定義します。

- stable 主線ビルドと hardened-x64 経路
- Linux LSM の active 検出と診断
- systemd/AppArmor/SELinux artefact の適用手順

## Build Hardening

### stable 主線

- `cargo build --release --bin oo-bot`
- release profile は `overflow-checks = true`, `panic = "abort"`, `lto = "thin"`, `codegen-units = 1`

### hardened-x64

- x86_64/Linux のときだけ試行
- pinned nightly: `nightly-2026-03-15`
- 実行:

```bash
./scripts/build_hardened_x64.sh
```

非対応ホスト・不足 toolchain では warning を出してスキップし、stable 主線は継続します。

### Hardening Verification

```bash
./scripts/verify_hardening.sh target/release/oo-bot stable
./scripts/verify_hardening.sh target/x86_64-unknown-linux-gnu/release/oo-bot hardened-x64
```

確認項目:

- PIE
- RELRO/NOW
- NX
- CET note (IBT/SHSTK)
- `__stack_chk_fail`
- CFI symbols (best-effort)

## LSM Runtime Detection

起動時に次を best-effort で検出します。

- major: AppArmor / SELinux / Smack / TOMOYO
- minor/diagnostic: Yama / LoadPin / SafeSetID

検出失敗は warning のみで起動継続します。

## AppArmor

- policy: `deploy/apparmor/oo-bot.apparmor`
- runtime control socket (`/run/oo-bot/control.sock`) を許可済み
- 例:

```bash
sudo cp deploy/apparmor/oo-bot.apparmor /etc/apparmor.d/oo-bot
sudo apparmor_parser -r /etc/apparmor.d/oo-bot
```

## SELinux

- policy source: `deploy/selinux/oo_bot.te`
- file context: `deploy/selinux/oo_bot.fc`
- installer: `deploy/selinux/install_selinux_policy.sh`

```bash
sudo ./deploy/selinux/install_selinux_policy.sh
```

installer は次を実施します。

- `checkmodule -> semodule_package(-f oo_bot.fc) -> semodule -i`
- `/opt/oo-bot/oo-bot`, `/etc/oo-bot`, `/var/lib/oo-bot`, `/run/oo-bot` への `restorecon`

期待する主要ラベル:

- `/opt/oo-bot/oo-bot` -> `oo_bot_exec_t`
- `/etc/oo-bot(/.*)?` -> `oo_bot_etc_t`
- `/var/lib/oo-bot(/.*)?` -> `oo_bot_var_lib_t`
- `/run/oo-bot(/.*)?` -> `oo_bot_var_run_t`

## systemd hardened unit

- unit: `deploy/systemd/oo-bot.service`
- rootless 実行を前提に `User=oo-bot` / `Group=oo-bot`
- read-only rootfs 前提で `StateDirectory=oo-bot` のみ書き込み許可
