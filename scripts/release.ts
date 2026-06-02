#!/usr/bin/env bun
// Usage:
//   bun release.ts patch          # 0.0.2 → 0.0.3
//   bun release.ts minor          # 0.0.2 → 0.1.0
//   bun release.ts major          # 0.0.2 → 1.0.0
//   bun release.ts 0.1.0          # explicit version
//   bun release.ts patch --dry-run

import { join } from "path";

// ── colours ───────────────────────────────────────────────────────────────────
const c = {
  red:    (s: string) => `\x1b[31m${s}\x1b[0m`,
  green:  (s: string) => `\x1b[32m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  blue:   (s: string) => `\x1b[34m${s}\x1b[0m`,
  bold:   (s: string) => `\x1b[1m${s}\x1b[0m`,
};

const step = (msg: string) => console.log(`\n${c.bold(c.blue(`▶ ${msg}`))}`);
const ok   = (msg: string) => console.log(`  ${c.green("✓")} ${msg}`);
const warn = (msg: string) => console.log(`  ${c.yellow("⚠")} ${msg}`);
const die  = (msg: string): never => { console.error(`\n${c.red(`✗ ${msg}`)}`); process.exit(1); };

// ── args ──────────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
if (args.length === 0) die("Usage: bun release.ts <patch|minor|major|X.Y.Z> [--dry-run]");

const bump   = args.filter(a => a !== "--dry-run")[0];
const dryRun = args.includes("--dry-run");
const root   = join(import.meta.dir, ".."); // script lives in scripts/, root is one level up

if (!bump) die("Usage: bun release.ts <patch|minor|major|X.Y.Z> [--dry-run]");

// ── helpers ───────────────────────────────────────────────────────────────────
async function run(cmd: string[], opts: { cwd?: string; allowFailure?: boolean } = {}) {
  const proc = Bun.spawn(cmd, { cwd: opts.cwd ?? root, stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (code !== 0) {
    if (opts.allowFailure) {
      throw new Error(err.trim());
    }
    die(`Command failed: ${cmd.join(" ")}\n${err.trim()}`);
  }
  return out.trim();
}

async function which(bin: string): Promise<boolean> {
  try { 
    await run(["which", bin], { allowFailure: true }); 
    return true; 
  } catch { 
    return false; 
  }
}

function bumpVersion(current: string, type: string): string {
  const [maj, min, pat] = current.split(".").map(Number);
  if (type === "major") return `${maj + 1}.0.0`;
  if (type === "minor") return `${maj}.${min + 1}.0`;
  if (type === "patch") return `${maj}.${min}.${pat + 1}`;
  return type; // explicit version passed
}

function confirm(msg: string): boolean {
  const ans = prompt(`\n  ${msg} [y/N]`);
  return ans?.trim().toLowerCase() === "y";
}

// ── preflight ─────────────────────────────────────────────────────────────────
step("Preflight checks");

const fail = dryRun
  ? (msg: string) => warn(`[ignored in dry-run] ${msg}`)
  : die;

// In jj, @ may be ahead of main — check that main bookmark is a reachable ancestor
// main::@ checks for descendants of main that are also ancestors of @
const ancestorCheck = await run(["jj", "log", "-r", "main::@", "--no-graph", "-T", "change_id"], { allowFailure: true })
  .catch(() => null);

if (ancestorCheck === null) {
  fail("main bookmark not found");
} else if (ancestorCheck === "") {
  fail("@ is not a descendant of main");
} else {
  ok("main is reachable ancestor of @");
}

const dirty = await run(["jj", "diff"]);
if (dirty) fail(`Working copy has uncommitted changes — stash or describe them first.\n${dirty}`);
else ok("Working copy clean");

for (const bin of ["cargo", "jj"]) {
  if (!await which(bin)) die(`${bin} not found`);
}
ok("Required tools present");

// ── version ───────────────────────────────────────────────────────────────────
step("Version");

const mainToml = await Bun.file(join(root, "crates/oyui/Cargo.toml")).text();
const current  = mainToml.match(/^version = "(.+?)"/m)?.[1] ?? die("Could not read current version");

const isExplicit = /^\d+\.\d+\.\d+$/.test(bump);
if (!isExplicit && !["patch", "minor", "major"].includes(bump)) die(`Invalid bump '${bump}'`);

const next = bumpVersion(current, bump);
console.log(`  ${current} ${c.bold("→")} ${next}`);

let liveRun = !dryRun;
if (liveRun) {
  warn("No --dry-run flag provided — this will publish to crates.io and push to GitHub.");
  if (confirm("Do a dry run first?")) {
    liveRun = false;
    warn("Dry run — no changes will be made");
  } else if (!confirm("Proceed with real release?")) {
    process.exit(0);
  }
}

// ── update Cargo.toml files ───────────────────────────────────────────────────
step("Bumping versions in Cargo.toml files");

const tomls = [
  "Cargo.toml",
  "crates/oyui/Cargo.toml",
  "crates/oyui-rune-actions/Cargo.toml",
  "crates/oyui-rune-actions/derive/Cargo.toml",
  "crates/oyui-tasker/Cargo.toml",
  "crates/oyui-tasker/derive/Cargo.toml",
  "crates/syndiff/Cargo.toml",
  "flake.nix",
];

for (const rel of tomls) {
  const path = join(root, rel);
  const file = Bun.file(path);
  if (!await file.exists()) { warn(`${rel} not found, skipping`); continue; }
  if (liveRun) {
    const updated = (await file.text()).replaceAll(`"${current}"`, `"${next}"`);
    await Bun.write(path, updated);
  }
  ok(rel);
}

if (liveRun) {
  step("Updating Cargo.lock");
  await run(["cargo", "check", "-q", "--manifest-path", join(root, "Cargo.toml")]);
  ok("Cargo.lock updated");
}

// ── changelog ─────────────────────────────────────────────────────────────────
step("Changelog");

if (await which("git-cliff")) {
  if (liveRun) {
    await run(["git-cliff", "--config", join(root, "cliff.toml"), "--tag", `v${next}`, "--output", join(root, "CHANGELOG.md")]);
  } else {
    const preview = await run(["git-cliff", "--config", join(root, "cliff.toml"), "--tag", `v${next}`, "--unreleased"]).catch(() => "");
    if (preview) console.log(`\n--- CHANGELOG PREVIEW ---\n${preview}\n--- END PREVIEW ---`);
  }
  ok("CHANGELOG.md");
} else {
  warn("git-cliff not found — install with: cargo install git-cliff");
  warn("CHANGELOG.md not updated");
}

// ── commit + tag + push ───────────────────────────────────────────────────────
step("Commit, tag, push");

if (liveRun) {
  // Commit working copy changes
  await run(["jj", "commit", "-m", `chore: release v${next}`]);
  // Move standard main bookmark to the newly created release commit (@-)
  await run(["jj", "bookmark", "set", "main", "-r", "@-"]);
  // Tag the release commit
  await run(["jj", "tag", "set", `v${next}`, "-r", "@-"]);
  // Force a git sync so downstream tools see it (highly recommended for non-colocated setups)
  await run(["jj", "git", "export"]);
  // Push both the main bookmark and the tag to remote
  await run(["jj", "git", "push", "--bookmark", "main"]);
  await run(["git", "push", "origin", `v${next}`]);
  ok(`Committed, tagged v${next}, and pushed`);
} else {
  warn(`[dry-run] would: jj commit -m 'chore: release v${next}'`);
  warn(`[dry-run] would: jj bookmark set main -r @-`);
  warn(`[dry-run] would: jj tag set v${next} -r @-`);
  warn(`[dry-run] would: jj git export`);
  warn(`[dry-run] would: jj git push --bookmark main`);
  warn(`[dry-run] would: git push origin v${next}`);
}

// ── publish to crates.io ──────────────────────────────────────────────────────
step("Publishing to crates.io");

if (liveRun) {
  // Cargo natively supports workspace-wide publishes in dependency topological order
  await run(["cargo", "publish", "--workspace"]);
  ok("All workspace crates published");
} else {
  warn("[dry-run] would: cargo publish --workspace");
}

// ── github release ────────────────────────────────────────────────────────────
step("GitHub release");

if (await which("gh")) {
  let notes = "";
  if (await which("git-cliff")) {
    notes = await run(["git-cliff", "--config", join(root, "cliff.toml"), "--tag", `v${next}`, "--strip", "all", "--unreleased"]).catch(() => "");
  }
  if (liveRun) {
    await run(["gh", "release", "create", `v${next}`, "--title", `v${next}`, "--notes", notes || `Release v${next}`]);
    ok(`GitHub release v${next} created`);
  } else {
    warn(`[dry-run] would: gh release create v${next}`);
  }
} else {
  warn("gh CLI not found — install from https://cli.github.com");
  warn(`Create the release manually at: https://github.com/emilien-jegou/oyui/releases/new`);
}

// ── done ──────────────────────────────────────────────────────────────────────
const doneMsg = liveRun ? `✓ Released v${next}` : `✓ Dry run complete — rerun without --dry-run to release v${next}`;
console.log(`\n${c.bold(c.green(doneMsg))}\n`);
