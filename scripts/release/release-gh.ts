#!/usr/bin/env bun
// Usage:
//   bun release-gh.ts

import { join, basename } from "node:path";
import { existsSync, readdirSync, statSync, readFileSync } from "node:fs";

const root = join(import.meta.dir, "../..");

const c = {
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  blue: (s: string) => `\x1b[34m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
  gray: (s: string) => `\x1b[90m${s}\x1b[0m`,
  cyan: (s: string) => `\x1b[36m${s}\x1b[0m`,
};

const step = (msg: string) => console.log(`\n${c.bold(c.blue(`▶ ${msg}`))}`);
const ok = (msg: string) => console.log(`  ${c.green("✓")} ${msg}`);
const warn = (msg: string) => console.log(`  ${c.yellow("⚠")} ${msg}`);
const die = (msg: string): never => {
  console.error(`\n${c.red(`✗ ${msg}`)}`);
  process.exit(1);
};

async function runCommand(cmd: string[], opts: { cwd?: string } = {}) {
  const proc = Bun.spawn(cmd, { cwd: opts.cwd ?? root, stdout: "pipe", stderr: "pipe" });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  await proc.exited;

  if (proc.exitCode !== 0) {
    die(`Command failed: ${cmd.join(" ")}\n${stderr.trim()}`);
  }
  return stdout.trim();
}

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

// Basic markdown formatter for terminal presentation
function renderMarkdownTerminal(md: string): string {
  if (!md) return c.gray("No release notes found.");
  
  return md
    .split("\n")
    .map((line) => {
      // Headers
      if (line.startsWith("### ")) {
        return `\n${c.bold(c.cyan(line.replace("### ", "⬢ ")))}`;
      }
      if (line.startsWith("## ")) {
        return `\n${c.bold(c.blue(line.replace("## ", "■ ")))}\n`;
      }
      // Lists
      if (line.trim().startsWith("- ") || line.trim().startsWith("* ")) {
        const indent = line.match(/^\s*/)?.[0] ?? "";
        const cleanLine = line.trim().substring(2);
        return `${indent}  ${c.yellow("•")} ${cleanLine}`;
      }
      // Inline Code blocks
      return line.replace(/`([^`]+)`/g, (_, code) => c.gray(code));
    })
    .join("\n");
}

// ── Execution Flow ───────────────────────────────────────────────────────────

// 1. Preflight & GitHub Verification
step("Preflight Verification");

try {
  await runCommand(["which", "gh"]);
} catch {
  die("GitHub CLI (gh) is missing.");
}

const authCheck = await runCommand(["gh", "auth", "status"]).catch(() => null);
if (!authCheck) {
  die("GitHub CLI is unauthenticated. Log in first via 'gh auth login'.");
}
ok("GitHub CLI verified and authenticated");

// 2. Identify Version
const oyuiCargo = await Bun.file(join(root, "crates/oyui/Cargo.toml")).text();
const nextVersion = oyuiCargo.match(/^version = "(.+?)"/m)?.[1];
if (!nextVersion) {
  die("Unable to identify current package version inside crates/oyui/Cargo.toml.");
}
console.log(`Working with Release Version: v${nextVersion}`);

// 3. Compile and Package
step("Compiling and packaging binaries");

const hostTarget = (await runCommand(["rustc", "-vV"])).match(/^host:\s+(.+)$/m)?.[1] ?? "unknown-target";
const targets = [hostTarget];
const distDir = join(root, "dist");
const workspaceCrates = ["oyui", "syndiff"];

if (existsSync(distDir)) {
  await runCommand(["rm", "-rf", distDir]);
}
await runCommand(["mkdir", "-p", distDir]);

for (const target of targets) {
  console.log(`  Target architecture: ${c.bold(target)}`);
  await runCommand(["cargo", "build", "--release", "--target", target]);

  const releaseDir = join(root, "target", target, "release");
  if (!existsSync(releaseDir)) continue;

  const files = readdirSync(releaseDir);
  for (const file of files) {
    const fullPath = join(releaseDir, file);
    const stat = statSync(fullPath);
    if (!stat.isFile()) continue;

    const isExecutable = target.includes("windows") ? file.endsWith(".exe") : (stat.mode & 0x49) !== 0;
    const stem = file.replace(/\.exe$/i, "");

    if (isExecutable && workspaceCrates.includes(stem)) {
      const packageDirName = `${stem}-v${nextVersion}-${target}`;
      const packagePath = join(distDir, packageDirName);
      await runCommand(["mkdir", "-p", packagePath]);

      // Copy executable and documentation files
      await runCommand(["cp", fullPath, join(packagePath, file)]);
      for (const doc of ["README.md", "LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "CHANGELOG.md"]) {
        const docPath = join(root, doc);
        if (existsSync(docPath)) {
          await runCommand(["cp", docPath, join(packagePath, doc)]);
        }
      }

      const archiveExt = target.includes("windows") ? ".zip" : ".tar.gz";
      const archiveTarget = join(distDir, `${packageDirName}${archiveExt}`);

      if (archiveExt === ".zip") {
        if (process.platform === "win32") {
          await runCommand(["powershell", "-Command", `Compress-Archive -Path "${packagePath}" -DestinationPath "${archiveTarget}" -Force`]);
        } else {
          await runCommand(["zip", "-r", archiveTarget, packageDirName], { cwd: distDir });
        }
      } else {
        await runCommand(["tar", "-czf", archiveTarget, "-C", distDir, packageDirName]);
      }
      ok(`Package built: ${packageDirName}${archiveExt}`);
    }
  }
}

// 4. Locate Assets for Release
step(`Locating Release Assets for v${nextVersion}`);
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
  die(`No packaging assets found under 'dist/' for version v${nextVersion}.`);
}
ok(`Identified ${assets.length} assets to upload`);

// 5. Extract Release Notes
step("Extracting Release Notes");
const notes = extractChangelogSection(join(root, "CHANGELOG.md"), nextVersion);
if (!notes) {
  warn("No explicit changelog entry matched. Reverting to default fallback header.");
} else {
  ok("Release details successfully extracted");
}

// 6. Review Draft and Prompt User
step("Review Release Properties");

console.log(`\n${c.bold("Target Tag:")} v${nextVersion}`);

console.log(`\n${c.bold("Assets to Upload:")}`);
for (const asset of assets) {
  console.log(`  ${c.gray("-")} ${basename(asset)}`);
}

console.log(`\n${c.bold("Changelog Preview:")}`);
console.log(c.gray("─".repeat(50)));
console.log(renderMarkdownTerminal(notes || `Release v${nextVersion}`));
console.log(c.gray("─".repeat(50)) + "\n");

const confirmInput = prompt("Proceed to publish release to GitHub? (y/N):");
if (confirmInput?.trim().toLowerCase() !== "y" && confirmInput?.trim().toLowerCase() !== "yes") {
  die("Release process aborted by user request.");
}

// 7. Publish Release to GitHub
step("Publishing Release to GitHub");
const tag = `v${nextVersion}`;
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
