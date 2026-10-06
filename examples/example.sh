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

# Prints a git commit's changed files and their full content, so a split's
# effect on each commit is visible (not just the file names).
git_show_commit() {
  local repo="$1" rev="$2" label="$3"
  printf '%s:\n' "$label"
  local changed
  changed="$(git -C "$repo" diff-tree --no-commit-id --name-only -r "$rev" 2>/dev/null || true)"
  if [[ -z "$changed" ]]; then
    printf '  (no changes)\n'
    return 0
  fi
  local f
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    printf '  %s:\n' "$f"
    if git -C "$repo" cat-file -e "$rev:$f" 2>/dev/null; then
      git -C "$repo" show "$rev:$f" | sed 's/^/    /'
    else
      printf '    (deleted)\n'
    fi
  done <<< "$changed"
}

# jj equivalent of git_show_commit.
jj_show_commit() {
  local repo="$1" rev="$2" label="$3"
  (
    cd "$repo" || return 1
    printf '%s:\n' "$label"
    local changed
    changed="$(jj diff --name-only -r "$rev" 2>/dev/null || true)"
    if [[ -z "$changed" ]]; then
      printf '  (empty)\n'
      return 0
    fi
    local f
    while IFS= read -r f; do
      [[ -z "$f" ]] && continue
      printf '  %s:\n' "$f"
      jj file show -r "$rev" -- "$f" 2>/dev/null | sed 's/^/    /'
    done <<< "$changed"
  )
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
  git-split       split the working-tree changes into two commits
  git-squash      fold selected working-tree hunks into HEAD
  git-resolve     resolve a conflict via "oyui resolve"
  jj-diffedit     run oyui through "jj diffedit" (needs jj)
  jj-resolve      run oyui through "jj resolve" (needs jj)
  jj-split        split the current change via "jj split" (needs jj)
  jj-squash       squash the current change via "jj squash" (needs jj)
  jj-diff         show a revision diff via "oyui diff" (needs jj)
  git-diff        show a revision diff via "oyui diff" (needs git)
  jj-interdiff    compare two patches via "oyui interdiff" (needs jj)
  git-interdiff   compare two patches via "oyui interdiff" (needs git)
  current         open the current change via bare "oyui" (needs jj)
  check           verify fixtures and run the oyui test suite
  clean           remove the scratch directory ($WORK)

Run "examples/example.sh list" or "examples/example.sh help".
EOF
}

list_scenarios() {
  printf 'diff\nstaged\nmerge\nmerge-clean\nconflict\ngit-difftool\ngit-mergetool\ngit-split\ngit-squash\ngit-resolve\njj-diffedit\njj-resolve\njj-split\njj-squash\njj-diff\ngit-diff\njj-interdiff\ngit-interdiff\ncurrent\ncheck\nclean\n'
}

# --- scenarios -------------------------------------------------------------

scenario_diff() {
  # Read-only: confirming exits without writing.
  run_oyui jj difftool "$EXAMPLES/diff/left" "$EXAMPLES/diff/right"
}

scenario_staged() {
  local dir
  dir="$(prepare staged)"
  copy_dir "$EXAMPLES/diff/left" "$dir/left"
  copy_dir "$EXAMPLES/diff/right" "$dir/right"
  printf 'Editing %s/right -- unstage hunks to drop, press enter to write back.\n' "$dir"
  run_oyui jj edittool "$dir/left" "$dir/right"
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
  run_oyui jj mergetool "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/result.txt"
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
  run_oyui jj mergetool "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/result.txt"
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
  run_oyui jj mergetool "$dir/base.txt" "$dir/ours.txt" "$dir/theirs.txt" -o "$dir/merged.txt"
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
    git config difftool.oyui.cmd "$BIN git difftool \"\$LOCAL\" \"\$REMOTE\"" && \
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
  (cd "$repo" && git merge --no-ff side >/dev/null 2>&1 || true)
  (cd "$repo" && \
    git config mergetool.oyui.cmd "$BIN git mergetool \"\$BASE\" \"\$LOCAL\" \"\$REMOTE\" -o \"\$MERGED\"" && \
    git config mergetool.prompt false)
  printf 'Running "git mergetool" in %s (choose a side, enter to write)\n' "$repo"
  (cd "$repo" && git mergetool --tool=oyui)
  printf 'Resolved file:\n'; sed 's/^/  /' "$repo/file.txt"
}

scenario_git_split() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare git-split)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity >/dev/null 2>&1)
  printf 'alpha\nbravo\ncharlie\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm base >/dev/null)
  printf 'ALPHA\nbravo\nCHARLIE\n' > "$repo/file.txt"
  printf 'fresh\n' > "$repo/new.txt"

  printf 'Base commit:\n'
  (cd "$repo" && git show --format='  %h %s' --stat HEAD | sed 's/^/  /')
  printf '\nRunning "oyui split" in %s\n' "$repo"
  printf 'Select the hunks for the FIRST commit with space, then press enter.\n'
  (cd "$repo" && run_oyui split)

  printf '\nHistory after split:\n'
  (cd "$repo" && git log --reverse --format='%h %s' --name-status | sed 's/^/  /')
  printf '\n'
  git_show_commit "$repo" 'HEAD~1' 'Selected (first) commit content'
  printf '\n'
  git_show_commit "$repo" 'HEAD' 'Remaining (second) commit content'
  printf '\nUndo: git reset --hard %s\n' \
    "$(cd "$repo" && git rev-parse --short refs/oyui/split-backup 2>/dev/null || echo '<none>')"
}

scenario_git_squash() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare git-squash)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity >/dev/null 2>&1)
  printf 'alpha\nbravo\ncharlie\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm base >/dev/null)
  printf 'second\n' > "$repo/other.txt"
  (cd "$repo" && git add . && git commit -qm second >/dev/null)
  printf 'ALPHA\nbravo\nCHARLIE\n' > "$repo/file.txt"

  printf 'Working change:\n'
  (cd "$repo" && git diff | sed 's/^/  /')
  printf '\nRunning "oyui squash" in %s\n' "$repo"
  printf 'Everything starts selected; unstage hunks to keep, then press enter.\n'
  (cd "$repo" && run_oyui squash)

  printf '\nHistory after squash:\n'
  (cd "$repo" && git log --reverse --format='%h %s' --name-status | sed 's/^/  /')
  printf '\n'
  git_show_commit "$repo" 'HEAD' 'HEAD content'
  printf '\nWorking tree status (unselected changes stay here):\n'
  (cd "$repo" && git status --short | sed 's/^/  /')
  printf '\nUndo: git reset --hard %s\n' \
    "$(cd "$repo" && git rev-parse --short refs/oyui/split-backup 2>/dev/null || echo '<none>')"
}

scenario_git_resolve() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare git-resolve)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity >/dev/null 2>&1)
  printf 'common\nvalue = base\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm base >/dev/null)
  (cd "$repo" && git checkout -qb side && \
    printf 'common\nvalue = theirs\n' > file.txt && git commit -qam theirs >/dev/null)
  (cd "$repo" && git checkout -q main && \
    printf 'common\nvalue = ours\n' > file.txt && git commit -qam ours >/dev/null)
  (cd "$repo" && git merge --no-ff side >/dev/null 2>&1 || true)

  printf 'Conflicted files:\n'
  (cd "$repo" && git diff --name-only --diff-filter=U | sed 's/^/  /')
  printf '\nRunning "oyui resolve" in %s (pick a side, enter to confirm)\n' "$repo"
  (cd "$repo" && run_oyui resolve)
  printf '\nStatus after resolve:\n'; (cd "$repo" && git status --short | sed 's/^/  /')
  printf 'Resolved file.txt:\n'; sed 's/^/  /' "$repo/file.txt"
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
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" && \
    jj config set --repo user.email "you@example.com")
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && jj commit --quiet -m base)
  printf 'value = ours\n' > "$repo/file.txt"
  printf 'Running "jj diffedit" in %s\n' "$repo"
  (cd "$repo" && jj --config "merge-tools.oyui.program=\"$BIN\"" \
    --config 'merge-tools.oyui.edit-args=["jj", "edittool", "$left", "$right"]' \
    diffedit --tool oyui)
}

scenario_jj_resolve() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir
  dir="$(prepare jj-resolve)"
  local repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" && \
    jj config set --repo user.email "you@example.com")
  # Building a jj conflict depends on jj's revset/CLI semantics; treat the
  # setup as best-effort so a version mismatch does not abort the runner.
  if ! (
    cd "$repo" || exit 1
    printf 'common\nvalue = base\n' > file.txt
    jj commit --quiet -m base &&
      jj new -m ours && printf 'common\nvalue = ours\n' > file.txt && jj commit --quiet -m ours &&
      jj new 'description(base*)' -m theirs && \
        printf 'common\nvalue = theirs\n' > file.txt && jj commit --quiet -m theirs &&
      jj new 'description(ours*)' 'description(theirs*)' -m merge
  ); then
    printf 'could not build a jj conflict automatically (jj version?); skipping.\n'
    return 0
  fi
  printf 'Running "jj resolve" in %s\n' "$repo"
  (cd "$repo" && jj --config "merge-tools.oyui.program=\"$BIN\"" \
    --config 'merge-tools.oyui.merge-args=["jj", "mergetool", "$base", "$left", "$right", "-o", "$output"]' \
    resolve --tool oyui)
}

scenario_jj_split() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare jj-split)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" >/dev/null 2>&1 && \
    jj config set --repo user.email "you@example.com" >/dev/null 2>&1)
  printf 'alpha\nbravo\ncharlie\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m base >/dev/null 2>&1)
  printf 'ALPHA\nbravo\nCHARLIE\n' > "$repo/file.txt"

  printf 'Change to split:\n'
  (cd "$repo" && jj diff | sed 's/^/  /')
  printf '\nRunning "oyui split" in %s\n' "$repo"
  printf 'Select the hunks for the FIRST commit with space, then press enter.\n'
  (cd "$repo" && run_oyui split -m selected)

  printf '\nHistory after split:\n'
  (cd "$repo" && jj log --reversed -T log_with_files | sed 's/^/  /')
  printf '\n'
  jj_show_commit "$repo" '@-' 'Selected (first) commit content'
  printf '\n'
  jj_show_commit "$repo" '@' 'Remaining (second) commit content'
}

scenario_jj_squash() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare jj-squash)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" >/dev/null 2>&1 && \
    jj config set --repo user.email "you@example.com" >/dev/null 2>&1)
  printf 'alpha\nbravo\ncharlie\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m base >/dev/null 2>&1)
  printf 'ALPHA\nbravo\nCHARLIE\n' > "$repo/file.txt"

  printf 'Change to squash:\n'
  (cd "$repo" && jj diff | sed 's/^/  /')
  printf '\nRunning "oyui squash" in %s\n' "$repo"
  printf 'Everything starts selected; unstage hunks to keep, then press enter.\n'
  (cd "$repo" && run_oyui squash)

  printf '\nHistory after squash:\n'
  (cd "$repo" && jj log --reversed -T log_with_files | sed 's/^/  /')
  printf '\n'
  jj_show_commit "$repo" '@-' 'Parent commit content'
}

scenario_jj_diff() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare jj-diff)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" >/dev/null 2>&1 && \
    jj config set --repo user.email "you@example.com" >/dev/null 2>&1)
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m base >/dev/null 2>&1)
  printf 'value = ours\nextra = 1\n' > "$repo/file.txt"

  printf 'Working copy change:\n'
  (cd "$repo" && jj diff | sed 's/^/  /')
  printf '\nRunning "oyui diff --from @-" in %s (read-only viewer)\n' "$repo"
  (cd "$repo" && run_oyui diff --from @-)
}

scenario_git_diff() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare git-diff)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity >/dev/null 2>&1)
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm base >/dev/null)
  printf 'value = ours\nextra = 1\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm ours >/dev/null)

  printf 'History:\n'
  (cd "$repo" && git log --format='  %h %s' | sed 's/^/  /')
  printf '\nRunning "oyui diff --from HEAD~1 --to HEAD" in %s (read-only viewer)\n' "$repo"
  (cd "$repo" && run_oyui diff --from 'HEAD~1' --to HEAD)
}

scenario_jj_interdiff() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare jj-interdiff)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" >/dev/null 2>&1 && \
    jj config set --repo user.email "you@example.com" >/dev/null 2>&1)
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m base >/dev/null 2>&1)
  printf 'value = v1\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m v1 >/dev/null 2>&1)
  printf 'value = v2\nextra = 1\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m v2 >/dev/null 2>&1)

  printf 'Patch evolution (@-- vs @-):\n'
  (cd "$repo" && jj diff -r @-- | sed 's/^/  /')
  (cd "$repo" && jj diff -r @- | sed 's/^/  /')
  printf '\nRunning "oyui interdiff --from @-- --to @-" in %s\n' "$repo"
  (cd "$repo" && run_oyui interdiff --from @-- --to @-)
}

scenario_git_interdiff() {
  if ! have_git; then
    printf 'git is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare git-interdiff)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && git init -q -b main && git_identity >/dev/null 2>&1)
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm base >/dev/null)
  printf 'value = v1\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm v1 >/dev/null)
  printf 'value = v2\nextra = 1\n' > "$repo/file.txt"
  (cd "$repo" && git add . && git commit -qm v2 >/dev/null)

  printf 'Patch evolution (HEAD~2 vs HEAD~1):\n'
  (cd "$repo" && git show --format= --patch 'HEAD~1' | sed 's/^/  /')
  (cd "$repo" && git show --format= --patch HEAD | sed 's/^/  /')
  printf '\nRunning "oyui interdiff --from HEAD~2 --to HEAD~1" in %s\n' "$repo"
  (cd "$repo" && run_oyui interdiff --from 'HEAD~2' --to 'HEAD~1')
}

scenario_current() {
  if ! have_jj; then
    printf 'jj is not installed; skipping.\n'; return 0
  fi
  local dir repo
  dir="$(prepare current)"
  repo="$dir/repo"
  mkdir -p "$repo"
  (cd "$repo" && jj git init >/dev/null 2>&1 && \
    jj config set --repo user.name "Example" >/dev/null 2>&1 && \
    jj config set --repo user.email "you@example.com" >/dev/null 2>&1)
  printf 'value = base\n' > "$repo/file.txt"
  (cd "$repo" && jj commit -m base >/dev/null 2>&1)
  printf 'value = ours\n' > "$repo/file.txt"

  printf 'Current change:\n'
  (cd "$repo" && jj diff | sed 's/^/  /')
  printf '\nRunning bare "oyui" in %s (opens the current change)\n' "$repo"
  (cd "$repo" && run_oyui)
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
  for s in git-split:git git-squash:git git-resolve:git jj-split:jj jj-squash:jj jj-diff:jj git-diff:git jj-interdiff:jj git-interdiff:git current:jj; do
    scenario="${s%%:*}"; tool="${s##*:}"
    if "have_$tool"; then
      out="$(OYUI_EXAMPLE_DRY=1 "$0" "$scenario")"
      if [[ "$out" != *DRY:* ]]; then
        die "dry-run for '$scenario' produced no oyui command"
      fi
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
  diff|staged|merge|merge-clean|conflict|git-difftool|git-mergetool|git-split|git-squash|git-resolve|jj-diffedit|jj-resolve|jj-split|jj-squash|jj-diff|git-diff|jj-interdiff|git-interdiff|current)
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
