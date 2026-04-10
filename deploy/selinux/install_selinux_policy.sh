#!/usr/bin/env bash
set -euo pipefail

POLICY_DIR="$(cd "$(dirname "$0")" && pwd)"
MODULE_NAME="oo_bot"
MODULE_VERSION="1.1"
MODULE_MOD="$POLICY_DIR/${MODULE_NAME}.mod"
MODULE_PP="$POLICY_DIR/${MODULE_NAME}.pp"

if ! command -v checkmodule >/dev/null 2>&1 || ! command -v semodule_package >/dev/null 2>&1 || ! command -v semodule >/dev/null 2>&1; then
  echo "[warn] SELinux policy build tools are unavailable"
  exit 0
fi

checkmodule -M -m -o "$MODULE_MOD" "$POLICY_DIR/${MODULE_NAME}.te"
semodule_package -o "$MODULE_PP" -m "$MODULE_MOD" -f "$POLICY_DIR/${MODULE_NAME}.fc"
semodule -i "$MODULE_PP"

if ! command -v restorecon >/dev/null 2>&1; then
  echo "[warn] restorecon is unavailable; file relabeling was skipped"
  exit 0
fi

for path in /opt/oo-bot/oo-bot /etc/oo-bot /var/lib/oo-bot /run/oo-bot; do
  if [[ -e "$path" ]]; then
    restorecon -Rv "$path"
  else
    echo "[info] relabel skipped (not found): $path"
  fi
done

echo "[info] SELinux policy installed: ${MODULE_NAME} v${MODULE_VERSION}"
