/**
 * 「主操作必须在第一屏」契约测试（真实渲染）。
 *
 * 这一条是页面布局的第一性约束：用户打开一个页面，最想点的那个按钮
 * 必须不滚动就能点到。违反它，页面再好看也是失败的。
 *
 * 之前真实踩过的坑，全都是这一条被违反：
 *   - 专注页：「开始学习」和右侧 1100px 高的配置栏在同一 grid 行，
 *     默认 stretch 把左栏拉高，按钮被推到 716px，1280x720 下直接被切掉。
 *   - 复盘页：「保存复盘」在三个长输入框之后，960x680 下要下滚 969px。
 *   - 设置页：「保存设置」在整张长表单末尾，要下滚 1034px。
 *   - 闹钟页：5 个短字段全挤成单列，「添加闹钟」超出视口 30px。
 *
 * 判据不是「在视口内」，而是「中心点上最上层元素就是它自己」——
 * 后者才能同时抓住「在视口外」和「被别的元素盖住」两种情况。
 */
import { spawn } from 'node:child_process';
import { access, mkdir, readdir, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const port = Number(process.env.CTA_AUDIT_PORT || 4311);
const baseUrl = `http://127.0.0.1:${port}`;

// 每个页面的主操作候选选择器（第一个命中的就用）。
const PAGES = [
  { name: 'focus', label: '开始学习', candidates: ['.start-ritual-button'] },
  { name: 'checklist', label: '清单主操作', candidates: ['.today-plan-panel .primary-action', '.checklist-shell .primary-action'] },
  { name: 'review', label: '保存复盘', candidates: ['.review-actions .primary-action'] },
  { name: 'alarm', label: '添加闹钟', candidates: ['.alarm-shell .primary-action'] },
  { name: 'settings', label: '保存设置', candidates: ['.settings-shell .primary-action'] },
];

// 只测笔记本上真实会遇到的窗口尺寸；超宽屏留白多，不存在这类问题。
const SIZES = [
  { width: 960, height: 680 },
  { width: 1280, height: 720 },
  { width: 1366, height: 768 },
  { width: 1440, height: 900 },
];

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
        // npx 缓存里有很多无关包。
      }
    }
  } catch {
    return null;
  }
  return candidates[0] ?? null;
}

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

let failed = false;
const failures = [];

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
  const rows = [];

  for (const spec of PAGES) {
    const context = await browser.newContext({ viewport: SIZES[1] });
    const page = await context.newPage();
    await context.addInitScript({ path: resolve(root, 'scripts/ui-responsive-fixture.mjs') });

    try {
      await page.goto(`${baseUrl}/?uiPhase=idle&theme=light#${spec.name}`, { waitUntil: 'networkidle' });
      await page.waitForSelector('.app-shell .page-transition section', { timeout: 10_000 });
      await page.waitForTimeout(600);
    } catch (reason) {
      failures.push(`${spec.name}: 页面加载失败 ${reason instanceof Error ? reason.message : String(reason)}`);
      await context.close();
      continue;
    }

    for (const size of SIZES) {
      await page.setViewportSize(size);
      await page.waitForTimeout(320);

      const probe = await page.evaluate((candidates) => {
        let element = null;
        for (const candidate of candidates) {
          const found = document.querySelector(candidate);
          if (found) { element = found; break; }
        }
        if (!element) return { missing: true };

        const rect = element.getBoundingClientRect();
        const cx = rect.left + rect.width / 2;
        const cy = rect.top + rect.height / 2;
        const hit = document.elementFromPoint(cx, cy);

        return {
          top: Math.round(rect.top),
          bottom: Math.round(rect.bottom),
          viewportHeight: innerHeight,
          // 真正的判据：中心点上最上层是它自己（未被遮挡、未在视口外）。
          clickable: hit ? element.contains(hit) || hit === element : false,
          needScroll: rect.bottom > innerHeight ? Math.round(rect.bottom - innerHeight) : 0,
          label: (element.getAttribute('aria-label') || element.textContent || '').trim().slice(0, 16),
        };
      }, spec.candidates);

      rows.push({ page: spec.name, size: `${size.width}x${size.height}`, ...probe });

      if (probe.missing) {
        failures.push(`${spec.name}@${size.width}x${size.height}: 找不到主操作元素`);
      } else if (!probe.clickable) {
        failures.push(
          `${spec.name}@${size.width}x${size.height}: 「${spec.label}」不在第一屏可点位置`
          + (probe.needScroll ? `（需下滚 ${probe.needScroll}px）` : '（被遮挡）'),
        );
      }
    }
    await context.close();
  }

  await browser.close();
  await mkdir(resolve(root, 'output'), { recursive: true });
  await writeFile(resolve(root, 'output/cta-audit.json'), JSON.stringify(rows, null, 2), 'utf8');
} finally {
  try { process.kill(-server.pid); } catch { /* 已退出 */ }
}

if (failures.length) {
  console.error('主操作可见性校验失败：');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('主操作可见性校验通过：');
console.log('  - 专注/清单/复盘/闹钟/设置的主操作在 960x680 ~ 1440x900 下都能直接点到');
console.log('  - 判据为中心点命中测试，可同时发现「在视口外」与「被遮挡」');
