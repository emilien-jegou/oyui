
import { changelogUpdate, loadCargoDeps, regexUpdate, type ChangelogContext } from 'relacher';

const genChangelog = (folderPath: string, opts = {}) =>
  changelogUpdate(`${folderPath}${folderPath.at(-1) === '/' ? '' : '/'}CHANGELOG.md`, {
    onlyOn: ['major', 'minor', 'patch'],
    ...opts
  });

export const depsBuilder = (root: string) =>
  loadCargoDeps(root).onPackageBump("oyui",
    regexUpdate("./flake.nix", {
      search: 'version = "[^"]+"',
      replace: 'version = "{{version}}"',
    }),
    genChangelog('./crates/oyui'),
    genChangelog('./', {
      global: true,
      template: cliffTemplate
    }))
    .onPackageBump("oyui-tasker", genChangelog('./crates/oyui-tasker'))
    .onPackageBump("oyui-rune-actions", genChangelog('./crates/oyui-rune-actions'))
    .couple('oyui-rune-actions', 'oyui-rune-actions-derive')
    .couple('oyui-tasker', 'oyui-tasker-derive')
    .addWatchFiles('oyui', './scripts/release/release-gh.ts');



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
        `- ${breaking}${scope}${msg} — [\`${commit.shortHash}\`](https://github.com/emilien-jegou/oyui/commit/${commit.hash}) by ${commit.author}`,
      );
    }
  }

  return lines.join("\n");
}

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
