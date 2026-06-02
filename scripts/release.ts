#!/usr/bin/env bun
// Usage:
//   bun release.ts patch          # 0.0.2 → 0.0.3
//   bun release.ts minor          # 0.0.2 → 0.1.0
//   bun release.ts major          # 0.0.2 → 1.0.0
//   bun release.ts 0.1.0          # explicit version
//   bun release.ts patch --dry-run
//   bun release.ts patch --skip-crates-publish

import { join } from "path";
import { writeFileSync, appendFileSync, existsSync, readFileSync, readdirSync, statSync } from "fs";
import { homedir } from "os";

const logPath = "/tmp/oyui-release.log";

try {
  writeFileSync(logPath, `=== Release Session: ${new Date().toISOString()} ===\n\n`);
} catch (e) {
  // Graceful fallback if /tmp is not writable
}

// ── colours ───────────────────────────────────────────────────────────────────
const c = {
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  blue: (s: string) => `\x1b[34m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
  gray: (s: string) => `\x1b[90m${s}\x1b[0m`,
};

const step = (msg: string) => console.log(`\n${c.bold(c.blue(`▶ ${msg}`))}`);
const ok = (msg: string) => console.log(`  ${c.green("✓")} ${msg}`);
const warn = (msg: string) => console.log(`  ${c.yellow("⚠")} ${msg}`);
const die = (msg: string): never => { console.error(`\n${c.red(`✗ ${msg}`)}`); process.exit(1); };

// ── args ──────────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const flags = ["--dry-run", "--skip-crates-publish"];
const bump = args.filter(a => !flags.includes(a))[0];

const dryRun = args.includes("--dry-run");
const skipPublish = args.includes("--skip-crates-publish");
const root = join(import.meta.dir, ".."); // script lives in scripts/, root is one level up

if (!bump) {
  die("Usage: bun release.ts <patch|minor|major|X.Y.Z> [--dry-run] [--skip-crates-publish]");
}

// ── config ────────────────────────────────────────────────────────────────────
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

// Derive potential binary output names from workspace folders (e.g. "oyui", "syndiff")
const workspaceCrates = tomls
  .map(t => {
    const parts = t.split("/");
    return parts.length > 1 ? parts[parts.length - 2] : null;
  })
  .filter((n): n is string => !!n);

// ── helpers ───────────────────────────────────────────────────────────────────
async function run(cmd: string[], opts: { cwd?: string; allowFailure?: boolean } = {}) {
  console.log(`  ${c.gray(`$ ${cmd.join(" ")}`)}`);

  try {
    appendFileSync(logPath, `\n[${new Date().toISOString()}] Command: ${cmd.join(" ")}\n`);
  } catch {
    // Ignore logging write failures
  }

  const proc = Bun.spawn(cmd, { cwd: opts.cwd ?? root, stdout: "pipe", stderr: "pipe" });

  const history: string[] = [];
  let linesPrinted = 0;
  let fullStdout = "";
  let fullStderr = "";

  const cols = process.stdout.columns || 80;

  function updatePreview(newLines: string[]) {
    // Move cursor back up and clear down to overwrite previous frame
    if (linesPrinted > 0) {
      process.stdout.write(`\x1b[${linesPrinted}A\x1b[J`);
    }
    for (const line of newLines) {
      const cleanLine = line.replace(/[\r\n]/g, "");
      // Truncate based on terminal columns to avoid unexpected terminal line-wraps breaking offsets
      const truncated = cleanLine.length > cols - 4 ? cleanLine.substring(0, cols - 7) + "..." : cleanLine;
      process.stdout.write(`> ${c.gray(truncated)}\n`);
    }
    linesPrinted = newLines.length;
  }

  function clearPreview() {
    if (linesPrinted > 0) {
      process.stdout.write(`\x1b[${linesPrinted}A\x1b[J`);
      linesPrinted = 0;
    }
  }

  const handleLine = (line: string, isError: boolean) => {
    if (isError) {
      fullStderr += line + "\n";
    } else {
      fullStdout += line + "\n";
    }

    try {
      appendFileSync(logPath, `  ${isError ? "[ERR]" : "[OUT]"} ${line}\n`);
    } catch {
      // Ignore logging write failures
    }

    history.push(line);
    if (history.length > 4) {
      history.shift();
    }
    updatePreview(history);
  };

  const stdoutReader = proc.stdout.getReader();
  const stderrReader = proc.stderr.getReader();

  async function streamRead(reader: ReadableStreamDefaultReader<Uint8Array>, isError: boolean) {
    const decoder = new TextDecoder();
    let buffer = "";
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        if (buffer.trim()) {
          handleLine(buffer, isError);
        }
        break;
      }
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        handleLine(line, isError);
      }
    }
  }

  await Promise.all([
    streamRead(stdoutReader, false),
    streamRead(stderrReader, true),
    proc.exited,
  ]);

  const code = proc.exitCode;
  clearPreview();

  try {
    appendFileSync(logPath, `Exit Code: ${code}\n----------------------------------------\n`);
  } catch {
    // Ignore logging write failures
  }

  if (code !== 0) {
    if (opts.allowFailure) {
      throw new Error(fullStderr.trim() || fullStdout.trim());
    }
    die(`Command failed: ${cmd.join(" ")}\n${fullStderr.trim()}`);
  }
  return fullStdout.trim();
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

function hasCargoToken(): boolean {
  if (process.env.CARGO_REGISTRY_TOKEN) return true;
  const paths = [
    join(homedir(), ".cargo/credentials.toml"),
    join(homedir(), ".cargo/credentials")
  ];
  for (const p of paths) {
    try {
      if (existsSync(p)) {
        const content = readFileSync(p, "utf-8");
        if (/token\s*=/i.test(content)) {
          return true;
        }
      }
    } catch { }
  }
  return false;
}

async function getHostTarget(): Promise<string> {
  try {
    const output = await run(["rustc", "-vV"]);
    const match = output.match(/^host:\s+(.+)$/m);
    return match ? match[1] : "unknown-target";
  } catch {
    return "unknown-target";
  }
}

function getTargetBinaries(target: string): string[] {
  const dir = join(root, "target", target, "release");
  if (!existsSync(dir)) return [];
  const files = readdirSync(dir);
  const bins: string[] = [];

  for (const f of files) {
    const path = join(dir, f);
    try {
      const stat = statSync(path);
      if (stat.isFile()) {
        const isExe = target.includes("windows") ? f.endsWith(".exe") : (stat.mode & 0o111) !== 0;
        const stem = f.replace(/\.exe$/i, "");

        if (isExe && workspaceCrates.includes(stem)) {
          bins.push(path);
        }
      }
    } catch { }
  }
  return bins;
}

async function createArchive(sourceDir: string, archivePath: string) {
  const parentDir = join(sourceDir, "..");
  const dirName = sourceDir.replace(/\\/g, "/").split("/").pop()!;

  if (archivePath.endsWith(".zip")) {
    if (process.platform === "win32") {
      await run(["powershell", "-Command", `Compress-Archive -Path "${sourceDir}" -DestinationPath "${archivePath}" -Force`]);
    } else {
      await run(["zip", "-r", archivePath, dirName], { cwd: parentDir });
    }
  } else {
    await run(["tar", "-czf", archivePath, "-C", parentDir, dirName]);
  }
}

// ── preflight ─────────────────────────────────────────────────────────────────
step("Preflight checks");

const fail = dryRun
  ? (msg: string) => warn(`[ignored in dry-run] ${msg}`)
  : die;

for (const bin of ["cargo", "jj", "gh"]) {
  if (!await which(bin)) die(`${bin} not found`);
}
ok("Required tools present");

// Verify GitHub authentication
const ghAuth = await run(["gh", "auth", "status"], { allowFailure: true }).catch(() => null);
if (ghAuth === null) {
  fail("GitHub CLI (gh) is not authenticated. Run 'gh auth login' first.");
} else {
  ok("GitHub authenticated");
}

// Verify repository permissions on GitHub
const repoView = await run(["gh", "repo", "view", "--json", "viewerPermission"], { allowFailure: true }).catch(() => null);
if (repoView === null) {
  fail("Could not verify GitHub repository permissions. Ensure a remote exists and is reachable.");
} else {
  try {
    const { viewerPermission } = JSON.parse(repoView);
    if (viewerPermission === "ADMIN" || viewerPermission === "WRITE") {
      ok(`GitHub repository permissions verified (${viewerPermission})`);
    } else {
      fail(`Insufficient GitHub permissions: ${viewerPermission}. WRITE or ADMIN access is required.`);
    }
  } catch {
    fail("Failed to parse GitHub repository permissions payload.");
  }
}

// Verify crates.io token (skip checks if publishing is disabled)
if (skipPublish) {
  ok("crates.io registry token check skipped (publish bypassed)");
} else {
  if (hasCargoToken()) {
    ok("crates.io registry token found");
  } else {
    fail("No crates.io token found. Run 'cargo login' or set CARGO_REGISTRY_TOKEN.");
  }
}

// In jj, @ may be ahead of main — check that main bookmark is a reachable ancestor
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

// ── version ───────────────────────────────────────────────────────────────────
step("Version");

const mainToml = await Bun.file(join(root, "crates/oyui/Cargo.toml")).text();
const current = mainToml.match(/^version = "(.+?)"/m)?.[1] ?? die("Could not read current version");

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
  // Force a git sync so downstream tools see it
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

// ── build & package archives ──────────────────────────────────────────────────
step("Building and packaging binaries per architecture");

const hostTarget = await getHostTarget();
// Add additional targets here if cross-compilers are configured on your host environment
const TARGETS = [hostTarget];

const distDir = join(root, "dist");
const archiveAssets: string[] = [];

if (liveRun) {
  // Reset the distribution output folder
  if (existsSync(distDir)) {
    await run(["rm", "-rf", distDir]);
  }
  await run(["mkdir", "-p", distDir]);

  for (const target of TARGETS) {
    console.log(`\n  Compiling for target: ${c.bold(target)}...`);
    await run(["cargo", "build", "--release", "--target", target]);

    const binaries = getTargetBinaries(target);
    if (binaries.length === 0) {
      warn(`No workspace binaries found for target ${target}, skipping packaging`);
      continue;
    }

    for (const binPath of binaries) {
      const binName = binPath.split("/").pop()!;
      const stemName = binName.replace(/\.exe$/i, "");

      // Directory name layout: syndiff-v0.1.0-x86_64-apple-darwin
      const pkgName = `${stemName}-v${next}-${target}`;
      const pkgDir = join(distDir, pkgName);
      await run(["mkdir", "-p", pkgDir]);

      // Copy executable binary into directory
      await run(["cp", binPath, join(pkgDir, binName)]);

      // Copy accompanying documents into directory if present
      for (const doc of ["README.md", "LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "CHANGELOG.md"]) {
        const docPath = join(root, doc);
        if (existsSync(docPath)) {
          await run(["cp", docPath, join(pkgDir, doc)]);
        }
      }

      const isWin = target.includes("windows");
      const ext = isWin ? ".zip" : ".tar.gz";
      const archivePath = join(distDir, `${pkgName}${ext}`);

      console.log(`    Creating package: ${c.gray(`${pkgName}${ext}`)}`);
      await createArchive(pkgDir, archivePath);
      archiveAssets.push(archivePath);
    }
  }
  ok(`Packaged ${archiveAssets.length} artifacts in dist/`);
} else {
  warn(`[dry-run] would: cross-compile for targets: [ ${TARGETS.join(", ")} ]`);
  warn(`[dry-run] would: arrange binaries inside dist/ and package as .tar.gz / .zip archives`);
}

// ── publish to crates.io ──────────────────────────────────────────────────────
step("Publishing to crates.io");

if (skipPublish) {
  warn("Skipping crates.io publishing as requested via the --skip-crates-publish flag.");
} else if (liveRun) {
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
    // Extract the changelog segment ONLY for the latest tag (v${next})
    notes = await run(["git-cliff", "--config", join(root, "cliff.toml"), "--latest", "--strip", "all"]).catch(() => "");
  }

  if (liveRun) {
    if (archiveAssets.length > 0) {
      console.log(`  Uploading archives: ${archiveAssets.map(a => a.split("/").pop()).join(", ")}`);
    }

    // Pass target archives only; CHANGELOG.md is no longer appended as an asset.
    // The specific changelog notes for this version populate the description.
    await run(["gh", "release", "create", `v${next}`, ...archiveAssets, "--title", `v${next}`, "--notes", notes || `Release v${next}`]);
    ok(`GitHub release v${next} created with ${archiveAssets.length} target archives`);
  } else {
    warn(`[dry-run] would: gh release create v${next} [packaged archives...] --title v${next}`);
  }
} else {
  warn("gh CLI not found — install from https://cli.github.com");
  warn(`Create the release manually at: https://github.com/emilien-jegou/oyui/releases/new`);
}

// ── done ──────────────────────────────────────────────────────────────────────
const doneMsg = liveRun ? `✓ Released v${next}` : `✓ Dry run complete — rerun without --dry-run to release v${next}`;
console.log(`\n${c.bold(c.green(doneMsg))}\n`);
