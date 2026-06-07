#!/usr/bin/env bun
// Usage:
//   bun release-crates.ts
//   bun release-crates.ts --skip-publish

import { join } from "node:path";
import { existsSync, readdirSync, statSync, writeFileSync, appendFileSync } from "node:fs";
import { homedir } from "node:os";

const logPath = "/tmp/oyui-release.log";
const args = process.argv.slice(2);
const skipPublish = args.includes("--skip-publish");
const root = join(import.meta.dir, "..");

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

function hasCargoToken(): boolean {
  if (process.env.CARGO_REGISTRY_TOKEN) return true;
  const paths = [join(homedir(), ".cargo/credentials.toml"), join(homedir(), ".cargo/credentials")];
  for (const p of paths) {
    try {
      if (existsSync(p) && /token\s*=/i.test(Bun.file(p).text())) {
        return true;
      }
    } catch { }
  }
  return false;
}

// ── Execution Flow ───────────────────────────────────────────────────────────
step("Preflight Verification");

if (!skipPublish && !hasCargoToken()) {
  die("crates.io registry token is missing. Log in via 'cargo login' or set CARGO_REGISTRY_TOKEN.");
}
ok("crates.io token configuration verified");

// Retrieve current version from central crate
const oyuiCargo = await Bun.file(join(root, "crates/oyui/Cargo.toml")).text();
const nextVersion = oyuiCargo.match(/^version = "(.+?)"/m)?.[1];
if (!nextVersion) {
  die("Unable to identify current package version inside crates/oyui/Cargo.toml.");
}
console.log(`Working with Release Version: v${nextVersion}`);

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

if (!skipPublish) {
  step("Publishing crates to crates.io");
  await runCommand(["cargo", "publish", "--workspace"]);
  ok("Crates submitted to registry");
} else {
  warn("Skipping publishing step via override request.");
}
