/**
 * 设置 → 集成 → AI 排期面板回归测试（真实渲染）。
 *
 * 这个面板出过两类真实故障：
 *
 * 1. **展开即整页白屏。** 后端返回的 `planner_preferences` 只要缺 `meal_windows`
 *    字段，渲染时的 `meal_windows.map(...)` 就抛
 *    `TypeError: Cannot read properties of undefined`，React 错误边界接管，
 *    整个设置页变成「应用加载失败」。一个可选字段缺失不该让用户失去整页。
 *
 * 2. **长表单被塞进两列网格的一个格子。** `.settings-grid` 是两列，
 *    飞书/CalDAV/邮件/AI 排期四个面板各占一格。AI 排期是一整段纵向长表单
 *    （服务商、密钥、可用时段、三餐、长期记忆、8000 字自定义提示词…），
 *    挤在 424px 宽的格子里展开后有 1900px 高，同一行的三个短面板被拉到
 *    同样高度，整页出现大片空白。它应当 `grid-column: 1 / -1` 独占整行。
 *
 * 判据只锁定「不该发生什么」，不锁定具体字号/间距，避免以后调样式就误报。
 */
import { spawn } from 'node:child_process';
import { access, readdir } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const port = Number(process.env.AI_PANEL_PORT || 4321);
const baseUrl = `http://127.0.0.1:${port}`;

const SIZES = [
  { width: 960, height: 680 },
  { width: 1280, height: 720 },
  { width: 1440, height: 900 },
];

const server = spawn(process.execPath, [
  resolve(root, 'node_modules/vite/bin/vite.js'),
  'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort',
], {
  cwd: root,
  env: process.env,
  detached: process.platform === 'win32',
  windowsHide: true,
  stdio: 'ignore',
});
server.unref();

const failures = [];

async function waitForServer() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(baseUrl);
      if (response.ok) return;
    } catch {
      // 启动竞态。
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`vite preview did not become ready at ${baseUrl}`);
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
        candidates.push(moduleEntry);
      } catch {
        // 缓存里有很多无关包。
      }
    }
  } catch {
    return null;
  }
  return candidates[0] ?? null;
}

try {
  await waitForServer();
  const entry = await findPlaywrightEntry();
  if (!entry) {
    console.error('跳过：未找到 Playwright（先运行 npm run test:ui-responsive 准备环境）。');
    process.exit(0);
  }

  const { chromium } = await import(pathToFileURL(entry).href);
  const launchOptions = { headless: true };
  if (process.platform === 'win32') {
    try {
      await access('C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe');
      launchOptions.channel = 'chrome';
    } catch {
      launchOptions.channel = 'msedge';
    }
  }
  const browser = await chromium.launch(launchOptions);

  for (const size of SIZES) {
    const context = await browser.newContext({ viewport: size });
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', (error) => pageErrors.push(error.message));
    await context.addInitScript({ path: resolve(root, 'scripts/ui-responsive-fixture.mjs') });

    const label = `${size.width}x${size.height}`;
    try {
      await page.goto(`${baseUrl}/?uiPhase=idle&theme=light#settings`, { waitUntil: 'networkidle' });
      await page.waitForSelector('.settings-shell', { timeout: 10_000 });
      await page.waitForTimeout(600);

      await page.locator('#settings-tab-integrations').click();
      await page.waitForTimeout(600);

      // 第 4 个折叠面板就是 AI 排期（飞书 / CalDAV / 邮件 / AI 排期）
      const aiToggle = page.locator('.settings-collapse-button').nth(3);
      if (await aiToggle.count() === 0) {
        failures.push(`${label}: 找不到 AI 排期面板的展开按钮`);
        await context.close();
        continue;
      }
      await aiToggle.click();
      await page.waitForTimeout(1200);

      const result = await page.evaluate(() => {
        const crashed = document.querySelectorAll('.app-error-screen').length > 0;
        const panel = [...document.querySelectorAll('.command-panel')]
          .find((node) => /AI 智能日程规划/.test(node.textContent || ''));
        const grid = document.querySelector('.settings-grid');
        const prompt = document.querySelector('.ai-custom-prompt');

        const panelRect = panel?.getBoundingClientRect();
        const gridRect = grid?.getBoundingClientRect();
        const promptStyle = prompt ? getComputedStyle(prompt) : null;
        const promptLineHeight = promptStyle ? parseFloat(promptStyle.lineHeight) || 20 : 0;

        return {
          crashed,
          hasPanel: Boolean(panel),
          // 面板宽度应当接近整个网格宽度（独占整行）
          fullRow: Boolean(panelRect && gridRect)
            && Math.abs(panelRect.width - gridRect.width) < 4,
          panelWidth: panelRect ? Math.round(panelRect.width) : 0,
          gridWidth: gridRect ? Math.round(gridRect.width) : 0,
          // 自定义提示词是 8000 字的规则文档，至少要看得到 8 行
          promptLines: prompt
            ? Math.floor((prompt.clientHeight - 24) / promptLineHeight)
            : 0,
          // 面板内部宽度不应超过网格（否则横向溢出）
          overflowsHorizontally: grid ? grid.scrollWidth - grid.clientWidth > 2 : false,
        };
      });

      if (pageErrors.length) {
        failures.push(`${label}: 展开时抛出 ${pageErrors.length} 个 JS 错误 → ${pageErrors[0].slice(0, 120)}`);
      }
      if (result.crashed) {
        failures.push(`${label}: 展开 AI 排期面板后整页崩溃（应用加载失败）`);
      }
      if (!result.hasPanel) {
        failures.push(`${label}: 展开后找不到 AI 排期面板`);
      }
      if (result.hasPanel && !result.fullRow) {
        failures.push(
          `${label}: AI 排期面板未独占整行（${result.panelWidth}px / 网格 ${result.gridWidth}px）`,
        );
      }
      if (result.promptLines < 8) {
        failures.push(
          `${label}: 自定义提示词输入框只有 ${result.promptLines} 行可见，长规则文档写不进去`,
        );
      }
      if (result.overflowsHorizontally) {
        failures.push(`${label}: AI 排期面板横向溢出`);
      }
    } catch (reason) {
      failures.push(`${label}: ${reason instanceof Error ? reason.message : String(reason)}`);
    } finally {
      await context.close();
    }
  }

  await browser.close();
} finally {
  try { process.kill(-server.pid); } catch { /* 已退出 */ }
}

if (failures.length) {
  console.error('AI 排期设置面板校验失败：');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('AI 排期设置面板校验通过：');
console.log('  - 展开后不崩溃、无 JS 错误（缺可选字段不会白屏）');
console.log('  - 面板独占整行，不再被两列网格压成 424px');
console.log('  - 自定义提示词输入框有足够行数可写长规则');
console.log('  - 无横向溢出');
