{ pkgs, ... }:

let
  make = import ./scripts/make-cli.nix { inherit pkgs; };
in {
  packages = [
    (make.mkCli (import ./make.nix))
    pkgs.bacon
    pkgs.bun
    pkgs.wtype
    pkgs.git-cliff
    pkgs.gh
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
    components =[ "rustc" "cargo" "rust-src" "rustfmt" "rust-analyzer" "clippy" ];
    targets =[ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ];
  };

  enterShell = ''
    [ -f .localrc ] && source .localrc
    dev
  '';

  dotenv.enable = true;
}
