{ pkgs }: {
  mkCli = { name, scripts, desc }:
    let
      # 1. Generate the dispatcher script
      dispatcher = ''
        #!/usr/bin/env bash

        # safely handle empty arguments
        case "''${1:-}" in
          ${builtins.concatStringsSep "\n" (map (s: ''
            "${s.cmd}")
              shift # removes $1 so we can pass "$@" to the underlying command if needed
              ${s.exec} "$@"
              ;;
          '') scripts)}
          *) 
            bold=$(tput bold 2>/dev/null || echo "")
            reset=$(tput sgr0 2>/dev/null || echo "")
            cyan=$(tput setaf 6 2>/dev/null || echo "")
            green=$(tput setaf 2 2>/dev/null || echo "")

            printf "%s%s🚀 %s%s\n" "$bold" "$cyan" "${desc}" "$reset"
            ${builtins.concatStringsSep "\n" (map (s: ''
              printf "  %s${name} %-12s%s  ${s.desc}\n" "$green" "${s.cmd}" "$reset"
            '') scripts)}
            ;;
        esac
      '';

    in
    # symlinkJoin bundles our script and completions into a single Nix package
    pkgs.symlinkJoin {
      name = "${name}-cli";
      paths = [
        (pkgs.writeShellScriptBin name dispatcher)
      ];
    };
}
