#!/usr/bin/env bash
set -euo pipefail

TOOLCHAIN="nightly-2026-03-15"
TARGET="x86_64-unknown-linux-gnu"

warn() {
  printf '[warn] %s\n' "$*" >&2
}

info() {
  printf '[info] %s\n' "$*"
}

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  warn "hardened-x64 is only attempted on x86_64/Linux; skipping"
  exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
  warn "cargo is unavailable"
  exit 0
fi

if ! rustup toolchain list | grep -q "$TOOLCHAIN"; then
  info "installing pinned toolchain: $TOOLCHAIN"
  rustup toolchain install "$TOOLCHAIN" --component rust-src || {
    warn "failed to install pinned nightly toolchain; skipping hardened build"
    exit 0
  }
fi

export OO_HARDENED_X64=1
export RUSTFLAGS="-Zcf-protection=full -C stack-protector=strong -C link-arg=-Wl,-z,relro,-z,now"

if command -v clang >/dev/null 2>&1 && command -v ld.lld >/dev/null 2>&1; then
  info "clang/lld detected: trying CFI-capable linking path"
  export RUSTFLAGS="$RUSTFLAGS -Clinker=clang -Clink-arg=-fuse-ld=lld"
else
  warn "clang/lld not available; continuing without CFI-specific linker path"
fi

if cargo +"$TOOLCHAIN" build --release --target "$TARGET"; then
  info "hardened-x64 build completed"
else
  warn "hardened-x64 build failed; stable path should still be used"
  exit 0
fi
