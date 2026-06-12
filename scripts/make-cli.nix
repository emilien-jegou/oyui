{ pkgs }: {
  mkCli = { name, scripts, desc }:
    let
      lib = pkgs.lib;
      visibleScripts = builtins.filter (s: s.visible or true) scripts;

      # Helper to format and safely join commands
      # We assume exec/deps are strings or lists of strings.
      formatCmds = cmds: 
        let 
          list = if builtins.isList cmds then cmds else [ cmds ];
        in builtins.concatStringsSep " && " list;

      # Logic for executing a script definition
      genScriptCase = s: ''
        "${s.cmd}")
          ${if s ? deps then ''
            # Execute dependencies
            ${builtins.concatStringsSep "\n" (map (dep: ''"$0" "${dep}"'') s.deps)}
          '' else ""}

          ${if s ? dir then ''
            pushd "${s.dir}" >/dev/null
          '' else ""}

          # Execute main logic
          set -x
          ${formatCmds s.exec} "''${@:2}"
          EXIT_CODE=$?
          { set +x; } 2>/dev/null

          ${if s ? dir then "popd >/dev/null" else ""}

          [ $EXIT_CODE -eq 0 ] || exit $EXIT_CODE
          ;;
      '';

      dispatcher = ''
        #!/usr/bin/env bash
        set -e # Abort on error

        # Gray color for set -x output
        export PS4='$(tput setaf 8 2>/dev/null)+ $(tput sgr0 2>/dev/null)'

        run_cmd() {
          case "$1" in
            ${builtins.concatStringsSep "\n" (map genScriptCase scripts)}
            *)
              echo "Error: Command '$1' not found."
              exit 1
              ;;
          esac
        }

        case "''${1:-}" in
          "") 
             bold=$(tput bold 2>/dev/null || echo "")
             reset=$(tput sgr0 2>/dev/null || echo "")
             cyan=$(tput setaf 6 2>/dev/null || echo "")
             green=$(tput setaf 2 2>/dev/null || echo "")

             printf "%s%s🚀 %s%s\n" "$bold" "$cyan" "${desc}" "$reset"
             ${builtins.concatStringsSep "\n" (map (s: ''
               printf "  %s${name} %-12s%s  ${s.desc}\n" "$green" "${s.cmd}" "$reset"
             '') visibleScripts)}
             ;;
          *) 
             run_cmd "$@"
             ;;
        esac
      '';
    in
    pkgs.symlinkJoin {
      name = "${name}-cli";
      paths = [ (pkgs.writeShellScriptBin name dispatcher) ];
    };
}
