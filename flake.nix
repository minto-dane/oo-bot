{
  description = "Reproducible development shell for discord-oo-bot";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        toolchain = with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          rust-analyzer
          just
          pkg-config
          openssl
          cargo-audit
          cargo-deny
          cargo-nextest
          cargo-llvm-cov
          cargo-geiger
          cargo-hack
          cargo-fuzz
          cargo-mutants
          cargo-machete
          cargo-udeps
          semgrep
          python3
          python3Packages.pipx
        ];
      in
      {
        packages.security-tools = pkgs.symlinkJoin {
          name = "oo-bot-security-tools";
          paths = toolchain;
        };

        devShells.default = pkgs.mkShell {
          packages = toolchain;

          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

          shellHook = ''
            echo "oo-bot dev shell"
            echo "  quick start: cargo xtask generate && cargo test --workspace --all-features"
            echo "  full local CI: just ci-local"
            echo "  replay only: cargo run --bin replay -- tests/fixtures/replay"
          '';
        };
      });
}
