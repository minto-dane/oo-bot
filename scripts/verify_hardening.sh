#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${1:-target/release/oo-bot}"
MODE="${2:-stable}"

warn() {
  printf '[warn] %s\n' "$*" >&2
}

info() {
  printf '[info] %s\n' "$*"
}

if [[ ! -f "$BIN_PATH" ]]; then
  warn "binary not found: $BIN_PATH"
  exit 0
fi

if ! command -v readelf >/dev/null 2>&1; then
  warn "readelf not available, skipping verification"
  exit 0
fi

if ! command -v objdump >/dev/null 2>&1; then
  warn "objdump not available, skipping verification"
  exit 0
fi

info "verifying hardening for $BIN_PATH ($MODE)"

# PIE check
if readelf -h "$BIN_PATH" | grep -q 'Type:.*DYN'; then
  info "PIE: enabled"
else
  warn "PIE: not detected"
fi

# RELRO / NOW check
if readelf -l "$BIN_PATH" | grep -q 'GNU_RELRO'; then
  info "RELRO: enabled"
else
  warn "RELRO: not detected"
fi

if readelf -d "$BIN_PATH" | grep -q 'BIND_NOW'; then
  info "NOW: enabled"
else
  warn "NOW: not detected"
fi

# NX stack check
if readelf -W -l "$BIN_PATH" | awk '/GNU_STACK/ {print $0}' | grep -q 'RWE'; then
  warn "NX: disabled (executable stack detected)"
else
  info "NX: enabled"
fi

# CET notes (x86_64 only)
if [[ "$(uname -m)" == "x86_64" ]]; then
  if readelf -n "$BIN_PATH" | grep -Eq 'IBT|SHSTK'; then
    info "CET note: present"
  else
    warn "CET note: not detected (toolchain/host may not support)"
  fi
fi

# stack protector symbol
if objdump -T "$BIN_PATH" | grep -q '__stack_chk_fail'; then
  info "stack protector symbol: detected"
else
  warn "stack protector symbol: not detected"
fi

# CFI symbol best-effort
if objdump -T "$BIN_PATH" | grep -Eq '__cfi_|cfi_check'; then
  info "CFI symbols: detected"
else
  warn "CFI symbols: not detected (this can be expected on unsupported toolchain)"
fi
