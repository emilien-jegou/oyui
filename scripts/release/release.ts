import { join } from 'node:path';
import { stdin as input, stdout as output } from 'node:process';
import readline from 'node:readline/promises';

import { Effect, pipe } from 'effect';
import {
  prepare,
  prettyPrint,
  run,
  makeRCVersionManager,
  VersionManagerService,
  VcsProviderService,
  makeJjVcsProvider,
  printDependencyList,
  log,
  init,
} from 'relacher';

import { depsBuilder } from './deps';
// ── Argument Parsing & Validation ──────────────────────────────────────────
const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const root = join(import.meta.dir, '../..');

const workspaceDeps = depsBuilder(root);

const positionalArgs = args.filter((arg) => !arg.startsWith('-'));
const mode = positionalArgs[0];

if (!mode || !['pre-release', 'release', 'init'].includes(mode)) {
  log.error(
    [
      'Invalid or missing mode.',
      '',
      'Usage:',
      '  dev relacher pre-release [--dry-run]',
      '  dev relacher release     [--dry-run]',
      '  dev relacher init',
    ].join('\n'),
  );
  process.exit(1);
}

const vcs = makeJjVcsProvider(root);


async function askConfirmation(query: string): Promise<boolean> {
  const rl = readline.createInterface({ input, output });
  const ans = await rl.question(query);
  rl.close();
  return ans.trim().toLowerCase() === 'y';
}

if (mode === 'init') {
  const runInit = Effect.gen(function*() {
    log.step(
      `Preflight checks & Workspace Discovery (${pipe(mode.toUpperCase(), log.c.bold, log.c.magenta)} mode)`,
    );

    log.step(`Scanned ${workspaceDeps.length} dependencies`);
    printDependencyList(workspaceDeps, true);

    yield* init(workspaceDeps, {
      cwd: root,
    }).pipe(Effect.provideService(VcsProviderService, vcs));
  });

  Effect.runPromise(runInit).catch((err) => {
    log.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  });
} else {
  const isPreRelease = mode === 'pre-release';

  // ── Execution Flow ─────────────────────────────────────────────────────────
  const runRelease = Effect.gen(function*() {

    log.step(
      `Preflight checks & Workspace Discovery (${pipe(mode.toUpperCase(), log.c.bold, log.c.magenta)} mode)`,
    );

    log.step(`Scanned ${workspaceDeps.length} dependencies`);
    printDependencyList(workspaceDeps, true);

    const vm = makeRCVersionManager(vcs, {
      upgradeReady: !isPreRelease,
      sizes: {

        major: { pattern: "^[a-z]+(?:\\([^)]+\\))?!|^[a-z]+\\([^)]+\\)!:|^BREAKING CHANGE", },
        minor: { pattern: "^(feat|revert|refactor|perf)" },
        patch: { pattern: "^(fix|bugfix|patch|deps|build)" },
        skip: { pattern: "^(release|chore|infra|docs|test|ci|nit|style)" },
      },
      cascade: {
        skip: 'skip',
        patch: 'patch',
        minor: 'minor',
        major: 'minor',
      },
    });

    const updates = yield* prepare(workspaceDeps, {
      cwd: root,
    }).pipe(Effect.provideService(VersionManagerService, vm));

    log.step('Proposed Updates');
    prettyPrint(updates);

    let liveRun = !dryRun;
    if (liveRun) {
      const confirmed = yield* Effect.promise(() =>
        askConfirmation(`\nProceed with staging and committing ${mode} updates? [y/N] `),
      );
      if (!confirmed) {
        liveRun = false;
        log.warn('Aborting execution flow.');
        return;
      }
    }

    if (liveRun) {
      log.step('Applying updates and writing changes');
      yield* run(updates, {
        cwd: root,
        commitTitle: (_: string, bumps: Record<string, string>) => {
          let itemCounts = Object.keys(bumps).length;
          if ('oyui' in bumps) {
            let s = '';
            s += `oyui-v${bumps['oyui']}`;
            let othersCount = itemCounts - 1;
            if (othersCount > 0) {
              s += ` (+${othersCount} lib)`
            }

            return 'release: ' + s;
          } else if (itemCounts > 0) {
            let libs = Object.entries(bumps)
              .map(([k, v]) => `${k}-v${v}`).join(', ');
            return 'release(libs):' + libs;
          } else {
            return 'release: ?'
          }

        }
      }).pipe(Effect.provideService(VcsProviderService, vcs));
      log.ok(`Updates completed successfully under ${mode} mode.`);
    } else {
      log.step('Dry run mode active');
      log.warn('No modifications written to disk.');
    }
  });

  Effect.runPromise(runRelease).catch((err) => {
    log.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  });
}
