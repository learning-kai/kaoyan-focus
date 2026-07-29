import { spawn, spawnSync } from 'node:child_process';
import { access, readdir, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runResponsiveMatrix } from './ui-responsive-browser.mjs';

const root = resolve(process.cwd());
const port = Number(process.env.UI_RESPONSIVE_PORT || 4176);
const baseUrl = process.env.UI_RESPONSIVE_BASE_URL || `http://127.0.0.1:${port}`;
const npxCommand = process.platform === 'win32' ? 'npx.cmd' : 'npx';
let browser;
let server;

async function waitForServer() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (server && server.exitCode !== null) throw new Error(`Vite exited early (${server.exitCode})`);
    try {
      const response = await fetch(baseUrl);
      if (response.ok) return;
    } catch {
      // Startup race.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }
  throw new Error(`Vite did not become ready at ${baseUrl}`);
}

async function findPlaywrightEntry() {
  const cacheRoot = process.env.npm_config_cache
    || (process.platform === 'win32'
      ? join(process.env.LOCALAPPDATA || homedir(), 'npm-cache')
      : join(homedir(), '.npm'));
  const npxRoot = join(cacheRoot, '_npx');
  const candidates = [];

  try {
    for (const entry of await readdir(npxRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const moduleEntry = join(npxRoot, entry.name, 'node_modules', 'playwright', 'index.mjs');
      try {
        await access(moduleEntry);
        candidates.push({ moduleEntry, modified: (await stat(moduleEntry)).mtimeMs });
      } catch {
        // The npx cache contains many unrelated packages.
      }
    }
  } catch {
    return null;
  }

  candidates.sort((left, right) => right.modified - left.modified);
  return candidates[0]?.moduleEntry || null;
}

async function loadChromium() {
  let playwrightEntry = await findPlaywrightEntry();
  if (!playwrightEntry) {
    const installed = spawnSync(
      npxCommand,
      ['--yes', '--package', '@playwright/cli', 'playwright-cli', '--version'],
      { cwd: root, encoding: 'utf8', env: process.env, shell: process.platform === 'win32' },
    );
    if (installed.status !== 0) {
      throw new Error(`Unable to prepare Playwright:\n${installed.stderr || installed.stdout}`);
    }
    playwrightEntry = await findPlaywrightEntry();
  }
  if (!playwrightEntry) throw new Error('Playwright was installed but its runtime entry was not found.');
  return (await import(pathToFileURL(playwrightEntry).href)).chromium;
}

async function browserLaunchOptions() {
  if (process.env.UI_RESPONSIVE_BROWSER) {
    await access(process.env.UI_RESPONSIVE_BROWSER);
    return { headless: true, executablePath: process.env.UI_RESPONSIVE_BROWSER };
  }
  if (process.platform === 'win32') {
    try {
      await access('C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe');
      return { headless: true, channel: 'chrome' };
    } catch {
      return { headless: true, channel: 'msedge' };
    }
  }
  return { headless: true };
}

try {
  if (!process.env.UI_RESPONSIVE_BASE_URL) {
    server = spawn(process.execPath, [
      resolve(root, 'node_modules/vite/bin/vite.js'),
      'preview',
      '--host', '127.0.0.1',
      '--port', String(port),
      '--strictPort',
    ], {
      cwd: root,
      env: process.env,
      detached: process.platform === 'win32',
      windowsHide: true,
      stdio: 'ignore',
    });
    server.unref();
  }
  await waitForServer();
  const chromium = await loadChromium();
  browser = await chromium.launch(await browserLaunchOptions());
  const requestedScope = process.env.UI_RESPONSIVE_SCOPE || 'all';
  const scopes = requestedScope === 'all' ? ['main', 'focus', 'navigation', 'drawers', 'widget'] : [requestedScope];
  let failed = false;
  for (const currentScope of scopes) {
    const context = await browser.newContext({ viewport: { width: 960, height: 680 } });
    const page = await context.newPage();
    const result = await runResponsiveMatrix(page, {
      baseUrl,
      fixturePath: resolve(root, 'scripts/ui-responsive-fixture.mjs'),
      outputDir: resolve(root, 'output/playwright/responsive'),
      reverseValidation: process.env.UI_RESPONSIVE_REVERSE === '1',
      progress: process.env.UI_RESPONSIVE_PROGRESS !== '0',
      scope: currentScope,
      screenshots: process.env.UI_RESPONSIVE_SCREENSHOTS === '1',
    });
    console.log(`[ui-responsive/${currentScope}] ${result.marker}`);
    if (result.failures.length) {
      failed = true;
      for (const failure of result.failures) console.error(`- ${failure}`);
    }
    await context.close();
  }
  console.log(failed ? '__UI_RESPONSIVE_FAIL__' : '__UI_RESPONSIVE_PASS__');
  if (failed) process.exitCode = 1;
} catch (error) {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  if (server && !server.killed) server.kill();
}
