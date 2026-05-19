{ pkgs ? import (fetchTarball {
    url = "https://channels.nixos.org/nixos-25.11/nixexprs.tar.xz";
  }) {} }:

let
  rust-overlay = import (builtins.fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
  });

  pkgs = import <nixpkgs> {
    overlays = [ rust-overlay ];
  };
in
pkgs.mkShell {
    buildInputs = with pkgs; [
      ## Rust build dependencies
      #gcc
      #openssl
      #wmctrl
      #pkg-config
      (pkgs.rust-bin.nightly."2026-05-01".default.override {
        extensions = ["rust-src" "rustfmt" "rust-analyzer" "clippy"];
        targets = ["wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ];
      })
   ];

    NIX_ENFORCE_PURITY = false;

    shellHook =
    ''
      [ ! -f .packages/bin/bacon ] && cargo install bacon --locked --root .packages/

      export PATH="$PATH:$(pwd)/.packages/bin/:$(pwd)/bin/";

      [ -f .localrc ] && source .localrc
    '';
}
