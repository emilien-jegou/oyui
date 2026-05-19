{
  description = "A flake for oyui";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }@inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        buildToolchain = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = [ "rustc" "cargo" ];
        };

        devToolchain = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = buildToolchain;
          rustc = buildToolchain;
        };

        commonBuildArgs = {
          src = pkgs.lib.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
          buildInputs = [ pkgs.openssl pkgs.wmctrl ];
          doCheck = false;
        };

      in {
        packages = {
          oyui = rustPlatform.buildRustPackage (commonBuildArgs // {
            pname = "oyui";
            version = "0.1.0";

            postInstall = ''
              wrapProgram $out/bin/oyui \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.wmctrl ]}
            '';
          });

          default = pkgs.symlinkJoin {
            name = "oyui-workspace";
            paths = [
              self.packages.${system}.oyui
            ];
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            devToolchain
            openssl
            pkg-config
            wmctrl
            bacon
            jujutsu
          ];

          shellHook = ''
            export PATH="$PATH:$(pwd)/bin/";
            [ -f .localrc ] && source .localrc
          '';
        };
      });
}
