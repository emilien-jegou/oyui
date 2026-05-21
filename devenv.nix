{ pkgs, ... }:

let
  make = import ./nix/make-cli.nix { inherit pkgs; };
in {
  packages = [
    (make.mkCli (import ./scripts.nix))
    pkgs.bacon
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
