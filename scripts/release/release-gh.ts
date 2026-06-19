#!/usr/bin/env bun
// Usage:
//   bun release-gh.ts

import { join, basename, resolve } from "node:path";
import { existsSync, readdirSync, statSync, readFileSync, rmSync, mkdirSync, cpSync } from "node:fs";
import { $, which } from "bun";

// ── Configuration ────────────────────────────────────────────────────────────

const ROOT = resolve(join(import.meta.dir, "../.."));
const DIST_DIR = join(ROOT, "dist");
const WORKSPACE_CRATES = ["oyui", "syndiff"];
const DOC_FILES = ["README.md", "LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "CHANGELOG.md"];

const DOCKER_IMAGE = "joseluisq/rust-linux-darwin-builder:1.89.0";

interface TargetConfig {
  name: string;        // Suffix used in the package name (using LLVM target triple)
  rustTarget: string;  // Target triple for the Rust compiler
  env?: Record<string, string>; // Environment variables for target compilers
}

const TARGETS: TargetConfig[] = [
  { name: "x86_64-unknown-linux-gnu", rustTarget: "x86_64-unknown-linux-gnu" },
  { name: "aarch64-unknown-linux-gnu", rustTarget: "aarch64-unknown-linux-gnu" },
  {
    name: "x86_64-apple-darwin",
    rustTarget: "x86_64-apple-darwin",
    env: {
      CC: "o64-clang",
      CXX: "o64-clang++"
    }
  },
  {
    name: "aarch64-apple-darwin",
    rustTarget: "aarch64-apple-darwin",
    env: {
      CC: "oa64-clang",
      CXX: "oa64-clang++"
    }
  },
];

$.cwd(ROOT);

// ── Loggers ──────────────────────────────────────────────────────────────────

const log = {
  step: (msg: string) => console.log(`\n\x1b[1m\x1b[34m▶ ${msg}\x1b[0m`),
  ok: (msg: string) => console.log(`  \x1b[32m✓\x1b[0m ${msg}`),
  warn: (msg: string) => console.log(`  \x1b[33m⚠\x1b[0m ${msg}`),
  error: (msg: string) => console.error(`\n\x1b[31m✗ ${msg}\x1b[0m`),
  gray: (s: string) => `\x1b[90m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
  cyan: (s: string) => `\x1b[36m${s}\x1b[0m`,
  blue: (s: string) => `\x1b[34m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
};

function die(msg: string): never {
  log.error(msg);
  process.exit(1);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async function checkPreflights() {
  log.step("Preflight Verification");

  if (!await which("gh")) die("GitHub CLI (gh) is missing.");
  if (!await which("docker")) die("Docker is required to cross-compile the targets.");

  try {
    await $`gh auth status`.quiet();
  } catch {
    die("GitHub CLI is unauthenticated. Log in first via 'gh auth login'.");
  }

  log.ok("GitHub CLI and Docker verified and authenticated.");
}

async function getVersion(): Promise<string> {
  const cargoTomlPath = join(ROOT, "crates/oyui/Cargo.toml");
  if (!existsSync(cargoTomlPath)) {
    die(`Cargo.toml not found at path: ${cargoTomlPath}`);
  }
  const content = await Bun.file(cargoTomlPath).text();
  const version = content.match(/^version = "(.+?)"/m)?.[1];
  if (!version) {
    die("Unable to identify current version in crates/oyui/Cargo.toml.");
  }
  return version;
}

async function buildTarget(target: TargetConfig) {
  log.ok(`Preparing and building inside container (${DOCKER_IMAGE})...`);

  let envPrefix = "";
  if (target.env) {
    envPrefix = Object.entries(target.env)
      .map(([key, val]) => `${key}=${val}`)
      .join(" ") + " ";
  }

  const buildCmd = `rm -rf target/${target.rustTarget} && ${envPrefix}cargo build --release --target ${target.rustTarget}`;

  await $`docker run --rm -v ${ROOT}:/root/src -w /root/src ${DOCKER_IMAGE} sh -c ${buildCmd}`;
}

async function packageTarget(target: TargetConfig, version: string) {
  const releaseDir = join(ROOT, "target", target.rustTarget, "release");
  if (!existsSync(releaseDir)) return;

  const files = readdirSync(releaseDir);
  for (const file of files) {
    const fullPath = join(releaseDir, file);
    const stat = statSync(fullPath);
    if (!stat.isFile()) continue;

    const isExecutable = (stat.mode & 0x49) !== 0;
    if (isExecutable && WORKSPACE_CRATES.includes(file)) {
      const packageDirName = `${file}-v${version}-${target.name}`;
      const packagePath = join(DIST_DIR, packageDirName);

      rmSync(packagePath, { recursive: true, force: true });
      mkdirSync(packagePath, { recursive: true });

      cpSync(fullPath, join(packagePath, file));

      for (const doc of DOC_FILES) {
        const docPath = join(ROOT, doc);
        if (existsSync(docPath)) {
          cpSync(docPath, join(packagePath, doc));
        }
      }

      const archiveTarget = `${packageDirName}.tar.gz`;
      await $`tar -czf ${join(DIST_DIR, archiveTarget)} -C ${DIST_DIR} ${packageDirName}`.quiet();
      log.ok(`Package built: ${archiveTarget}`);
    }
  }
}

function extractChangelog(version: string): string {
  const path = join(ROOT, "CHANGELOG.md");
  if (!existsSync(path)) return "";

  const lines = readFileSync(path, "utf-8").split("\n");
  const captures: string[] = [];
  let tracking = false;

  for (const line of lines) {
    if (line.startsWith("## [") || line.startsWith("## ")) {
      if (tracking) break;
      if (line.includes(version)) {
        tracking = true;
        continue;
      }
    }
    if (tracking) {
      captures.push(line);
    }
  }
  return captures.join("\n").trim();
}

function renderMarkdown(md: string): string {
  if (!md) return log.gray("No release notes found.");
  return md
    .split("\n")
    .map((line) => {
      if (line.startsWith("### ")) return `\n${log.bold(log.cyan(line.replace("### ", "⬢ ")))}`;
      if (line.startsWith("## ")) return `\n${log.bold(log.blue(line.replace("## ", "■ ")))}\n`;
      if (line.trim().startsWith("- ") || line.trim().startsWith("* ")) {
        const indent = line.match(/^\s*/)?.[0] ?? "";
        return `${indent}  ${log.yellow("•")} ${line.trim().substring(2)}`;
      }
      return line.replace(/`([^`]+)`/g, (_, code) => log.gray(code));
    })
    .join("\n");
}

// ── Main Pipeline ────────────────────────────────────────────────────────────

async function main() {
  await checkPreflights();

  const version = await getVersion();
  console.log(`Working with Release Version: v${version}`);

  // 1. Compile & Package
  log.step("Compiling and packaging binaries");
  rmSync(DIST_DIR, { recursive: true, force: true });
  mkdirSync(DIST_DIR, { recursive: true });

  for (const target of TARGETS) {
    console.log(`\n  Target: ${log.bold(target.name)} (${target.rustTarget})`);
    try {
      await buildTarget(target);
      await packageTarget(target, version);
    } catch (e) {
      log.error(`Failed to build target ${target.name}. Error: ${e}`);
    }
  }

  // 2. Locate Assets
  log.step(`Locating Release Assets for v${version}`);
  const assets = readdirSync(DIST_DIR)
    .filter(file => file.endsWith(".tar.gz"))
    .map(file => join(DIST_DIR, file));

  if (assets.length === 0) {
    die(`No package assets found under 'dist/' for version v${version}.`);
  }
  log.ok(`Identified ${assets.length} assets to upload.`);

  // 3. Extract Changelog
  log.step("Extracting Release Notes");
  const notes = extractChangelog(version);
  if (!notes) {
    log.warn("No explicit changelog entry matched. Reverting to fallback header.");
  } else {
    log.ok("Release details successfully extracted.");
  }

  // 4. Confirmation Prompt
  log.step("Review Release Properties");
  console.log(`\n${log.bold("Target Tag:")} v${version}`);
  console.log(`\n${log.bold("Assets to Upload:")}`);
  assets.forEach(asset => console.log(`  ${log.gray("-")} ${basename(asset)}`));
  console.log(`\n${log.bold("Changelog Preview:")}`);
  console.log(log.gray("─".repeat(50)));
  console.log(renderMarkdown(notes || `Release v${version}`));
  console.log(log.gray("─".repeat(50)) + "\n");

  const input = prompt("Proceed to publish release to GitHub? (y/N):");
  if (input?.trim().toLowerCase() !== "y" && input?.trim().toLowerCase() !== "yes") {
    die("Release process aborted by user.");
  }

  // 5. GitHub Publish
  log.step("Publishing Release to GitHub");
  const tag = `v${version}`;
  await $`gh release create ${tag} ${assets} --title ${tag} --notes ${notes || `Release ${tag}`}`;

  log.ok(`GitHub release ${tag} successfully published.`);
}

main().catch((err) => {
  die(err instanceof Error ? err.message : String(err));
});
