#!/usr/bin/env bash
set -euo pipefail

# Fallback bootstrap for non-Nix environments.
# If you use the repo flake, prefer `nix develop` instead.

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

rustup component add rustfmt clippy
rustup component add miri || true

cargo install cargo-audit --locked || true
cargo install cargo-deny --locked || true
cargo install cargo-nextest --locked || true
cargo install cargo-llvm-cov --locked || true
cargo install cargo-geiger --locked || true
cargo install cargo-hack --locked || true
cargo install cargo-fuzz --locked || true
cargo install cargo-mutants --locked || true
cargo install cargo-machete --locked || true

# cargo-udeps needs nightly toolchain.
rustup toolchain install nightly --profile minimal || true
cargo +nightly install cargo-udeps --locked || true

if command -v pipx >/dev/null 2>&1; then
  pipx install semgrep || true
else
  python3 -m pip install --user semgrep || true
fi

echo "security and quality tools bootstrap completed"
