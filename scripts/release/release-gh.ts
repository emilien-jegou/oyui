#!/usr/bin/env bun
// Usage:
//   bun release-gh.ts

import { join } from "node:path";
import { existsSync, readdirSync, readFileSync } from "node:fs";

const root = join(import.meta.dir, "..");

const c = {
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  blue: (s: string) => `\x1b[34m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
};

const step = (msg: string) => console.log(`\n${c.bold(c.blue(`▶ ${msg}`))}`);
const ok = (msg: string) => console.log(`  ${c.green("✓")} ${msg}`);
const die = (msg: string): never => {
  console.error(`\n${c.red(`✗ ${msg}`)}`);
  process.exit(1);
};

async function runCommand(cmd: string[]) {
  const proc = Bun.spawn(cmd, { cwd: root, stdout: "pipe", stderr: "pipe" });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  await proc.exited;

  if (proc.exitCode !== 0) {
    die(`Command failed: ${cmd.join(" ")}\n${stderr.trim()}`);
  }
  return stdout.trim();
}

// Extract release log details for the specific targeted version
function extractChangelogSection(changelogPath: string, version: string): string {
  if (!existsSync(changelogPath)) return "";
  const lines = readFileSync(changelogPath, "utf-8").split("\n");

  let captures: string[] = [];
  let found = false;

  for (const line of lines) {
    if (line.startsWith("## [") || line.startsWith("## ")) {
      if (found) break;
      if (line.includes(version)) {
        found = true;
        continue;
      }
    }
    if (found) {
      captures.push(line);
    }
  }
  return captures.join("\n").trim();
}

// ── Execution Flow ───────────────────────────────────────────────────────────
step("GitHub Authentication Checks");

try {
  await runCommand(["which", "gh"]);
} catch {
  die("GitHub CLI (gh) is missing.");
}

const authCheck = await runCommand(["gh", "auth", "status"]).catch(() => null);
if (!authCheck) {
  die("GitHub CLI is unauthenticated. Log in first via 'gh auth login'.");
}
ok("GitHub CLI verified");

const oyuiCargo = await Bun.file(join(root, "crates/oyui/Cargo.toml")).text();
const currentVersion = oyuiCargo.match(/^version = "(.+?)"/m)?.[1];
if (!currentVersion) {
  die("Could not retrieve version string from Cargo.toml.");
}

step(`Locating Release Assets for v${currentVersion}`);
const distDir = join(root, "dist");
const assets: string[] = [];

if (existsSync(distDir)) {
  const files = readdirSync(distDir);
  for (const file of files) {
    if (file.endsWith(".zip") || file.endsWith(".tar.gz")) {
      assets.push(join(distDir, file));
    }
  }
}

if (assets.length === 0) {
  die(`No packaging assets found under 'dist/' for version v${currentVersion}. Did you compile and package first?`);
}
ok(`Identified ${assets.length} assets to upload`);

step("Extracting Release Notes");
const notes = extractChangelogSection(join(root, "CHANGELOG.md"), currentVersion);
if (!notes) {
  console.log("  No explicit changelog entry matched. Reverting to default fallback header.");
} else {
  ok("Release details successfully extracted");
}

step("Publishing Release to GitHub");
const tag = `v${currentVersion}`;
await runCommand([
  "gh",
  "release",
  "create",
  tag,
  ...assets,
  "--title",
  tag,
  "--notes",
  notes || `Release ${tag}`,
]);

ok(`GitHub release ${tag} successfully published.`);
