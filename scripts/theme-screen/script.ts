#!/usr/bin/env bun

import { $ } from "bun";
import { existsSync, mkdirSync, rmSync, readdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

const TERMINAL_CMD = ["alacritty", "--class", "oyui-screens", "-e"];
const WINDOW_WIDTH = 1200;
const WINDOW_HEIGHT = 800;

const THEMES_DIR = "crates/oyui/themes";
const ISSUES_FILE = "scripts/theme-screen/theme-issues.json";

export type Success<T> = { ok: true; value: T };
export type Failure<E> = { ok: false; error: E };
export type Result<T, E = Error> = Success<T> | Failure<E>;

export function ok<T>(value: T): Success<T> { return { ok: true, value }; }
export function err<E>(error: E): Failure<E> { return { ok: false, error }; }

export async function safeAsync<T>(promise: Promise<T>): Promise<Result<T, Error>> {
  try { return ok(await promise); }
  catch (e) { return err(e instanceof Error ? e : new Error(String(e))); }
}

export function safeSync<T>(fn: () => T): Result<T, Error> {
  try { return ok(fn()); }
  catch (e) { return err(e instanceof Error ? e : new Error(String(e))); }
}

interface Theme { name: string; issues: string[]; }

function getSeverityScore(theme: Theme): number {
  if (theme.issues.length === 0) return 0;
  if (theme.issues.some((i) => i.includes("❌"))) return 2;
  if (theme.issues.some((i) => i.includes("⚠️"))) return 1;
  return 1;
}

async function loadThemes(): Promise<Result<Theme[], Error>> {
  const dirRes = safeSync(() => readdirSync(THEMES_DIR));
  if (!dirRes.ok) {
    return err(new Error(`Could not read themes directory '${THEMES_DIR}'. Are you running this from the repo root?`));
  }

  const themeFiles = dirRes.value.filter(file => file.endsWith(".tmTheme"));
  if (themeFiles.length === 0) {
    return err(new Error(`No .tmTheme files found in ${THEMES_DIR}`));
  }

  let issuesMap: Record<string, string[]> = {};
  if (existsSync(ISSUES_FILE)) {
    const issuesRes = await safeAsync(Bun.file(ISSUES_FILE).json());
    if (issuesRes.ok) issuesMap = issuesRes.value;
    else console.warn(`⚠️ Warning: ${ISSUES_FILE} exists but could not be parsed.`);
  }

  const themes: Theme[] = themeFiles
    .map(file => {
      const name = file.replace(".tmTheme", "");
      return { name, issues: issuesMap[name] || [] };
    })
    .sort((a, b) => {
      const scoreA = getSeverityScore(a);
      const scoreB = getSeverityScore(b);
      if (scoreA !== scoreB) return scoreA - scoreB;
      return a.name.localeCompare(b.name);
    });

  return ok(themes);
}

async function checkSystemRequirements(): Promise<Result<void, Error>> {
  if (!process.env.WAYLAND_DISPLAY) return err(new Error("WAYLAND_DISPLAY is not set."));
  if (!process.env.HYPRLAND_INSTANCE_SIGNATURE) return err(new Error("HYPRLAND_INSTANCE_SIGNATURE is not set."));

  const requiredTools = ["grim", "wtype", "hyprctl", "unzip", TERMINAL_CMD[0]];
  for (const tool of requiredTools) {
    if (!Bun.which(tool)) return err(new Error(`Required tool '${tool}' is not installed.`));
  }

  if (!existsSync("./target/release/oyui")) return err(new Error("./target/release/oyui does not exist. Please build it."));
  if (!existsSync("scripts/theme-screen/diff_target.zip")) return err(new Error("Zip file 'scripts/theme-screen/diff_target.zip' must exist."));

  return ok(undefined);
}

async function setupHyprlandRules(): Promise<Result<void, Error>> {
  const rules = [
    `float,class:^(oyui-screens)$`,
    `size ${WINDOW_WIDTH} ${WINDOW_HEIGHT},class:^(oyui-screens)$`,
    `center,class:^(oyui-screens)$`,
    `animation off,class:^(oyui-screens)$`
  ];

  for (const rule of rules) {
    const res = await safeAsync($`hyprctl keyword windowrulev2 ${rule}`.quiet());
    if (!res.ok) return err(res.error);
  }
  return ok(undefined);
}

async function getTerminalGeometry(): Promise<Result<string, Error>> {
  const hyprctlRes = await safeAsync($`hyprctl activewindow -j`.json());
  if (!hyprctlRes.ok) return err(hyprctlRes.error);

  const output = hyprctlRes.value;
  return ok(`${output.at[0]},${output.at[1]} ${output.size[0]}x${output.size[1]}`);
}

async function takeScreenshot(outputPath: string): Promise<Result<void, Error>> {
  const geomRes = await getTerminalGeometry();
  if (!geomRes.ok) return err(geomRes.error);

  const grimRes = await safeAsync($`grim -g ${geomRes.value} ${outputPath}`.quiet());
  if (!grimRes.ok) return err(grimRes.error);

  return ok(undefined);
}

async function pressKey(key: string): Promise<Result<void, Error>> {
  const typeRes = await safeAsync($`wtype ${key}`.quiet());
  if (!typeRes.ok) return err(typeRes.error);
  return ok(undefined);
}

async function pressSequence(keys: string[], delayMs: number = 10): Promise<Result<void, Error>> {
  for (const key of keys) {
    const res = await pressKey(key);
    if (!res.ok) return err(res.error);
    await Bun.sleep(delayMs);
  }
  return ok(undefined);
}

async function generateThemeScreenshots(themes: Theme[], assetsFolderPath: string): Promise<Result<void, Error>> {
  const sysRes = await checkSystemRequirements();
  if (!sysRes.ok) return err(sysRes.error);

  const ruleRes = await setupHyprlandRules();
  if (!ruleRes.ok) return err(ruleRes.error);

  const mkdirRes = safeSync(() => mkdirSync(assetsFolderPath, { recursive: true }));
  if (!mkdirRes.ok) return err(mkdirRes.error);

  const diffDir = join(tmpdir(), `oyui-diff-${Date.now()}`);
  const diffMkdirRes = safeSync(() => mkdirSync(diffDir, { recursive: true }));
  if (!diffMkdirRes.ok) return err(diffMkdirRes.error);

  try {
    const unzipRes = await safeAsync($`unzip -q scripts/theme-screen/diff_target.zip -d ${diffDir}`.quiet());
    if (!unzipRes.ok) return err(unzipRes.error);

    const dir0 = join(diffDir, "0");
    const dir1 = join(diffDir, "1");

    for (const theme of themes) {
      console.log(`Processing theme: ${theme.name}...`);

      const configPath = join(tmpdir(), `oyui-config-${theme.name}-${Date.now()}.toml`);
      const writeRes = await safeAsync(Bun.write(configPath, `chosen_theme = "${theme.name}"\n`));
      if (!writeRes.ok) return err(writeRes.error);

      const spawnRes = safeSync(() => Bun.spawn(
        [...TERMINAL_CMD, "./target/release/oyui", "--config", configPath, dir0, dir1],
        { stdin: "ignore", stdout: "ignore", stderr: "ignore" }
      ));

      if (!spawnRes.ok) return err(spawnRes.error);
      const tuiProcess = spawnRes.value;

      try {
        let windowReady = false;
        for (let i = 0; i < 20; i++) {
          const activeRes = await safeAsync($`hyprctl activewindow -j`.json());

          if (activeRes.ok && activeRes.value.class === "oyui-screens") {
            windowReady = true;
            if (activeRes.value.floating === false) {
              await safeAsync($`hyprctl dispatch togglefloating`);
              await safeAsync($`hyprctl dispatch resizeactive exact ${WINDOW_WIDTH} ${WINDOW_HEIGHT}`);
            }
            break;
          }
          await Bun.sleep(200);
        }

        if (!windowReady) return err(new Error(`Window for ${theme.name} did not appear.`));

        await Bun.sleep(200);
        const seq1 = await pressSequence(["j", "j", " "], 10);
        if (!seq1.ok) return err(seq1.error);

        const s1Res = await takeScreenshot(join(assetsFolderPath, `${theme.name}-1.png`));
        if (!s1Res.ok) return err(s1Res.error);

        await Bun.sleep(10);

        const seq2 = await pressSequence([
          "G", "k", "k", "l", " ", "n", "n", " ", "n", " ", "g", "g"
        ], 10);
        if (!seq2.ok) return err(seq2.error);

        await Bun.sleep(50);
        const s2Res = await takeScreenshot(join(assetsFolderPath, `${theme.name}.png`));
        if (!s2Res.ok) return err(s2Res.error);

        await Bun.sleep(50);
        const qRes = await pressKey("q");
        if (!qRes.ok) return err(qRes.error);

        await Promise.race([tuiProcess.exited, Bun.sleep(3000)]);
      } finally {
        tuiProcess.kill();
        safeSync(() => { if (existsSync(configPath)) rmSync(configPath); });
      }
    }
  } finally {
    safeSync(() => { if (existsSync(diffDir)) rmSync(diffDir, { recursive: true, force: true }); });
  }

  return ok(undefined);
}

async function generateReadme(themes: Theme[], themeMarkdownPath: string, assetsFolderPath: string): Promise<Result<void, Error>> {
  const header = `# Builtin themes\n\nMany of the themes in this list have issues, feel free to provide upgrades or to provide your own themes.\n\n> This document was auto generated via \`dev theme-screens\`\n`;

  const bodyBlocks = themes.map((theme) => {
    const issuesBlock = theme.issues.join("\n");
    return `
### \`${theme.name}\`
| Tree view | File View |
| :---: | :---: |
| <img style="min-width:400px;" src="/${assetsFolderPath}${theme.name}-1.png" width="400"> | <img style="min-width:400px;" src="/${assetsFolderPath}${theme.name}.png" width="400"> |

${issuesBlock}`;
  });

  const writeRes = await safeAsync(Bun.write(themeMarkdownPath, header + bodyBlocks.join("\n")));
  if (!writeRes.ok) return err(writeRes.error);

  return ok(undefined);
}

async function main(themeMarkdownPath: string, assetsFolderPath: string) {
  const themesRes = await loadThemes();
  if (!themesRes.ok) {
    console.error("❌ Failed to load themes:", themesRes.error.message);
    process.exit(1);
  }
  const themes = themesRes.value;
  console.log(`✅ Loaded ${themes.length} themes.`);

  const genRes = await generateThemeScreenshots(themes, assetsFolderPath);
  if (!genRes.ok) {
    console.error("❌ Failed to generate screenshots:", genRes.error.message);
    process.exit(1);
  }

  const mdRes = await generateReadme(themes, themeMarkdownPath, assetsFolderPath);
  if (!mdRes.ok) {
    console.error("❌ Failed to write Markdown:", mdRes.error.message);
    process.exit(1);
  }

  console.log("✅ Documentation generated successfully!");
}

main("docs/themes.md", "docs/assets/themes/").catch(console.error);
