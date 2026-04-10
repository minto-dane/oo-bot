#!/usr/bin/env bash
set -euo pipefail

POLICY_DIR="$(cd "$(dirname "$0")" && pwd)"

if ! command -v checkmodule >/dev/null 2>&1 || ! command -v semodule_package >/dev/null 2>&1 || ! command -v semodule >/dev/null 2>&1; then
  echo "[warn] SELinux policy build tools are unavailable"
  exit 0
fi

checkmodule -M -m -o "$POLICY_DIR/oo_bot.mod" "$POLICY_DIR/oo_bot.te"
semodule_package -o "$POLICY_DIR/oo_bot.pp" -m "$POLICY_DIR/oo_bot.mod"
semodule -i "$POLICY_DIR/oo_bot.pp"

echo "[info] SELinux policy installed: oo_bot"
