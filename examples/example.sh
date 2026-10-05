#!/usr/bin/env bash
#
# Run oyui on the bundled example fixtures.
#
# Usage:
#   examples/example.sh list          # list scenarios
#   examples/example.sh <scenario>    # prepare fixtures and launch oyui
#
# The script always drives the *local* binary. Override it with OYUI_BIN, and
# the scratch directory with OYUI_EXAMPLES_DIR (default /tmp/oyui-examples).
#
# Because oyui is a TUI, scenarios that launch it hand over the terminal.
# Scenarios that need a repository create one under the scratch directory so
# writes never touch the checked-in fixtures.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES="$ROOT/examples"
WORK="${OYUI_EXAMPLES_DIR:-/tmp/oyui-examples}"
BIN="${OYUI_BIN:-$ROOT/target/debug/oyui}"

# --- helpers ---------------------------------------------------------------

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

ensure_bin() {
  if [[ ! -x "$BIN" ]]; then
    printf 'building oyui (local)...\n'
    (cd "$ROOT" && cargo build -p oyui)
  fi
  [[ -x "$BIN" ]] || die "oyui binary not found at $BIN"
}

# prepare <name> -> creates and prints a clean scratch dir.
prepare() {
  local dir="$WORK/$1"
  rm -rf "$dir"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

copy_dir() {
  mkdir -p "$2"
  cp -R "$1/." "$2/"
}

# Runs the local binary; with OYUI_EXAMPLE_DRY=1 it only prints the command.
run_oyui() {
  if [[ "${OYUI_EXAMPLE_DRY:-0}" == "1" ]]; then
    printf 'DRY: %s' "$BIN"
    printf ' %q' "$@"
    printf '\n'
    return 0
  fi
  "$BIN" "$@"
}

usage() {
  cat <<EOF
oyui example runner

  OYUI_BIN=$BIN

Scenarios:
  diff            inspect a two-way directory diff (read-only)
  staged          stage a two-way diff and write the result
  merge           three-way merge with a conflicting region
  merge-clean     three-way merge with no conflicts
  conflict        resolve a target that already has conflict markers
  git-difftool    run oyui through "git difftool"
  git-mergetool   run oyui through "git mergetool"
  jj-diffedit     run oyui through "jj diffedit" (needs jj)
  jj-resolve      run oyui through "jj resolve" (needs jj)
  check           verify fixtures and run the oyui test suite
  clean           remove the scratch directory ($WORK)

Run "examples/example.sh list" or "examples/example.sh help".
EOF
}

list_scenarios() {
  printf 'diff\nstaged\nmerge\nmerge-clean\nconflict\ngit-difftool\ngit-mergetool\njj-diffedit\njj-resolve\ncheck\nclean\n'
}

# --- scenarios -------------------------------------------------------------

scenario_diff() {
  # Read-only: confirming exits without writing.
  run_oyui diff "$EXAMPLES/diff/left" "$EXAMPLES/diff/right" --no-write
}

scenario_staged() {
  local dir
  dir="$(prepare staged)"
  copy_dir "$EXAMPLES/diff/left" "$dir/left"
  copy_dir "$EXAMPLES/diff/right" "$dir/right"
  printf 'Editing %s/right -- stage hunks and press enter to write back.\n' "$dir"
  run_oyui diff "$dir/left" "$dir/right"
  printf 'Result written under %s/right\n' "$dir"
}

scenario_merge() {
  local dir
  dir="$(prepare merge)"
  cp "$EXAMPLES/merge/base.txt" "$dir/base.txt"
  cp "$EXAMPLES/merge/ours.txt" "$dir/ours.txt"
  cp "$EXAMPLES/merge/theirs.txt" "$dir/theirs.txt"
  cp "$EXAMPLES/merge/theirs.txt" "$dir/result.txt"
  printf 'Conflicting merge; oyui synthesizes the conflict. Writes %s/result.txt\n' "$dir"
  run_oyui merge "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/result.txt"
  printf 'Result:\n'; sed 's/^/  /' "$dir/result.txt"
}

scenario_merge_clean() {
  local dir
  dir="$(prepare merge-clean)"
  cp "$EXAMPLES/merge/clean_base.txt" "$dir/base.txt"
  cp "$EXAMPLES/merge/clean_ours.txt" "$dir/ours.txt"
  cp "$EXAMPLES/merge/clean_theirs.txt" "$dir/theirs.txt"
  cp "$EXAMPLES/merge/theirs.txt" "$dir/result.txt"
  printf 'Clean merge; non-overlapping edits from both sides are combined.\n'
  run_oyui merge "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/result.txt"
  printf 'Result:\n'; sed 's/^/  /' "$dir/result.txt"
}

scenario_conflict() {
  local dir
  dir="$(prepare conflict)"
  cp "$EXAMPLES/conflict/base.txt" "$dir/base.txt"
  cp "$EXAMPLES/conflict/ours.txt" "$dir/ours.txt"
  cp "$EXAMPLES/conflict/theirs.txt" "$dir/theirs.txt"
  cp "$EXAMPLES/conflict/merged.txt" "$dir/merged.txt"
  printf 'Target already contains markers; the resolver opens automatically.\n'
  run_oyui merge "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/merged.txt"
  printf 'Result:\n'; sed 's/^/  /' "$dir/merged.txt"
}

git_identity() {
  git config user.email "you@example.com"
  git config user.name "Example"
}

scenario_git_difftool() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir
  dir="$(prepare git-difftool)"
  local repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q && git_identity)
  cp "$EXAMPLES/diff/right/hello.txt" "$repo/hello.txt"
  (cd "$repo" && git add hello.txt && git commit -qm base && \
    cp "$EXAMPLES/diff/left/hello.txt" hello.txt && \
    git config difftool.oyui.cmd "$BIN diff \"\$LOCAL\" \"\$REMOTE\" --no-write" && \
    git config difftool.prompt false)
  printf 'Running "git difftool" in %s (press q/enter to move on)\n' "$repo"
  (cd "$repo" && git difftool --tool=oyui -y)
}

scenario_git_mergetool() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir
  dir="$(prepare git-mergetool)"
  local repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity)
  printf 'common\nvalue = base\n' > "$repo/file.txt"
  (cd "$repo" && git add file.txt && git commit -qm base)
  (cd "$repo" && git checkout -qb side && \
    printf 'common\nvalue = theirs\n' > file.txt && git commit -qam theirs)
  (cd "$repo" && git checkout -q main && \
    printf 'common\nvalue = ours\n' > file.txt && git commit -qam ours)
  (cd "$repo" && git merge side >/dev/null 2>&1 || true)
  (cd "$repo" && \
    git config mergetool.oyui.cmd "$BIN merge \"\$BASE\" \"\$LOCAL\" \"\$REMOTE\" -o \"\$MERGED\"" && \
    git config mergetool.prompt false)
  printf 'Running "git mergetool" in %s (choose a side, enter to write)\n' "$repo"
  (cd "$repo" && git mergetool --tool=oyui)
  printf 'Resolved file:\n'; sed 's/^/  /' "$repo/file.txt"
}

have_git() { command -v git >/dev/null 2>&1; }
have_jj() { command -v jj >/dev/null 2>&1; }

scenario_jj_diffedit() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir
  dir="$(prepare jj-diffedit)"
  local repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init -q && \
    jj config set --repo user.name "Example" && \
    jj config set --repo user.email "you@example.com")
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -qm base)
  printf 'value = ours\n' > "$repo/file.txt"
  printf 'Running "jj diffedit" in %s\n' "$repo"
  (cd "$repo" && jj --config-toml \
    "ui.diff-editor = [\"$BIN\", \"diff\", \"\$left\", \"\$right\"]" diffedit)
}

scenario_jj_resolve() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir
  dir="$(prepare jj-resolve)"
  local repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init -q && \
    jj config set --repo user.name "Example" && \
    jj config set --repo user.email "you@example.com")
  # Building a jj conflict depends on jj's revset/CLI semantics; treat the
  # setup as best-effort so a version mismatch does not abort the runner.
  if ! (
    cd "$repo" || exit 1
    printf 'common\nvalue = base\n' > file.txt
    jj commit -qm base &&
      jj new -m ours && printf 'common\nvalue = ours\n' > file.txt && jj commit -qm ours &&
      jj new 'description(base)' -m theirs && \
        printf 'common\nvalue = theirs\n' > file.txt && jj commit -qm theirs &&
      jj new 'description(ours)' 'description(theirs)' -m merge
  ); then
    printf 'could not build a jj conflict automatically (jj version?); skipping.\n'
    return 0
  fi
  printf 'Running "jj resolve" in %s\n' "$repo"
  (cd "$repo" && jj --config-toml "merge-tools.oyui.program = \"$BIN\"" \
    --config-toml 'merge-tools.oyui.merge-args = ["merge", "$base", "$left", "$right", "--allow-unresolved"]' \
    resolve --tool oyui)
}

scenario_clean() {
  rm -rf "$WORK"
  printf 'removed %s\n' "$WORK"
}

scenario_check() {
  local missing=0
  while IFS= read -r fixture; do
    if [[ ! -e "$fixture" ]]; then
      printf 'missing fixture: %s\n' "$fixture" >&2
      missing=1
    fi
  done <<EOF
$EXAMPLES/diff/left/hello.txt
$EXAMPLES/diff/right/hello.txt
$EXAMPLES/merge/base.txt
$EXAMPLES/merge/clean_base.txt
$EXAMPLES/conflict/merged.txt
$EXAMPLES/example.sh
EOF
  [[ "$missing" == 0 ]] || die "fixtures incomplete"
  printf 'fixtures ok\n'

  # Dry-run the launching scenarios: guards against a runner that recurses or
  # forgets to emit the oyui invocation.
  for s in diff staged merge merge-clean conflict; do
    out="$(OYUI_EXAMPLE_DRY=1 "$0" "$s")"
    if [[ "$out" != *DRY:* ]]; then
      die "dry-run for '$s' produced no oyui command"
    fi
  done
  printf 'dry-run ok\n'

  if [[ ! -x "$BIN" ]]; then
    printf 'building oyui (local)...\n'
    (cd "$ROOT" && cargo build -p oyui)
  fi
  printf 'running the test suite...\n'
  (cd "$ROOT" && cargo test -p oyui)
}

# --- dispatch --------------------------------------------------------------

scenario="${1:-help}"
case "$scenario" in
  list) list_scenarios ;;
  help|-h|--help) usage ;;
  diff|staged|merge|merge-clean|conflict|git-difftool|git-mergetool|jj-diffedit|jj-resolve)
    ensure_bin
    "scenario_${scenario//-/_}"
    ;;
  check)
    scenario_check
    ;;
  clean)
    scenario_clean
    ;;
  *)
    usage >&2
    die "unknown scenario '$scenario'"
    ;;
esac
