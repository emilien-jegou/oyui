#!/usr/bin/env bun
import { join } from "node:path";
import {
  prepare,
  cargoDeps,
  regexUpdate,
  changelogUpdate,
  prettyPrint,
  run,
  JjVcsProvider,
} from "relacher";
import type { ChangelogContext } from "relacher";

const c = {
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  blue: (s: string) => `\x1b[34m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
};

const step = (msg: string) => console.log(`\n${c.bold(c.blue(`▶ ${msg}`))}`);
const ok = (msg: string) => console.log(`  ${c.green("✓")} ${msg}`);
const warn = (msg: string) => console.log(`  ${c.yellow("⚠")} ${msg}`);
const die = (msg: string): never => {
  console.error(`\n${c.red(`✗ ${msg}`)}`);
  process.exit(1);
};

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const root = join(import.meta.dir, "../..");

function groupBy<T, K extends keyof T>(arr: T[], key: K): Record<string, T[]> {
  return arr.reduce(
    (acc, item) => {
      const group = String(item[key]);
      if (!acc[group]) acc[group] = [];
      acc[group].push(item);
      return acc;
    },
    {} as Record<string, T[]>,
  );
}

// Replicates the cliff.toml output template in pure TypeScript
function cliffTemplate({ version, date, commits }: ChangelogContext): string {
  const cleanVersion = version ? version.replace(/^v/, "") : "Unreleased";
  const lines = ['', `## [${cleanVersion}] - ${date}`];

  const grouped = groupBy(commits, "type");

  for (const [group, groupList] of Object.entries(grouped)) {
    if (groupList.length === 0) continue;
    lines.push(`\n### ${group.charAt(0).toUpperCase() + group.slice(1)}`);
    for (const commit of groupList) {
      const breaking = commit.isBreaking ? `[**breaking**] ` : ``;
      const scope = commit.scope ? `**${commit.scope}:** ` : ``;
      const desc = commit.description || commit.message;
      const msg = desc.charAt(0).toUpperCase() + desc.slice(1);
      lines.push(
        `- ${breaking}${scope}${msg} — [\`${commit.shortHash}\`](https://github.com/oyui/commit/${commit.hash}) by ${commit.author}`,
      );
    }
  }

  return lines.join("\n");
}

// ── Execution ───────────────────────────────────────────────────────────────
step("Preflight checks");

const vcs = new JjVcsProvider(root);
ok("Jujutsu VCS provider initialized");

const config = cargoDeps(root).on("oyui", (dep) =>
  dep
    .update(
      regexUpdate("./flake.nix", {
        search: 'version = "[^"]+"',
        replace: 'version = "{{version}}"',
      }),
    )
    .update(changelogUpdate('./crates/oyui/CHANGELOG.md', {}))
    .update(
      changelogUpdate("./CHANGELOG.md", {
        global: true,
        template: cliffTemplate,
      }),
    ))
  .on("oyui-tasker", (dep) => dep
    .update(changelogUpdate('./crates/oyui-tasker/CHANGELOG.md', {})))
  .on("oyui-rune-actions", (dep) => dep
    .update(changelogUpdate('./crates/oyui-rune-actions/CHANGELOG.md', {})))

  .couple('oyui-rune-actions', 'oyui-rune-actions-derive')
  .couple('oyui-tasker', 'oyui-tasker-derive');

step("Evaluating updates");
const updates = await prepare(config, vcs, {
  cwd: root,
  excludeNestedWatches: true,
  sizes: {
    major: { pattern: "^[a-z]+(?:\\([^)]+\\))?!|^[a-z]+\\([^)]+\\)!:|^BREAKING CHANGE", },
    minor: { pattern: "^(feat|revert|refactor|perf)" },
    patch: { pattern: "^(fix|bugfix|patch|deps)" },
    skip: { pattern: "^(release|chore|infra|docs|test|ci|build|nit|style)" },
  },
  cascade: {
    patch: {
      skip: "patch",
      patch: "patch",
      minor: "minor",
      major: "minor",
    },
  },
});

const activeUpdates = updates.filter((u) => u.bump !== "skip");
if (activeUpdates.length === 0) {
  warn("No package modifications detected. Everything is up to date.");
  process.exit(0);
}

prettyPrint(updates);

let liveRun = !dryRun;
if (liveRun) {
  const ans = prompt(`\nProceed with staging and committing release? [y/N]`);
  if (ans?.trim().toLowerCase() !== "y") {
    liveRun = false;
    warn("Aborting release run.");
    process.exit(0);
  }
}

if (liveRun) {
  step("Applying updates and creating commit/tags");
  await run(updates, vcs, { cwd: root });
  ok("Versions bumped, changelogs written, and tags set.");
} else {
  step("Dry run mode active");
  warn("No modifications written to disk.");
}
