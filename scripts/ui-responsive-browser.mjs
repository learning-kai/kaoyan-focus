import { mkdir } from 'node:fs/promises';

const MAIN_SIZES = [
  { width: 960, height: 680 },
  { width: 1100, height: 760 },
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
];

const MAIN_PAGES = [
  { name: 'focus', entry: '.start-ritual-button' },
  { name: 'checklist', entry: '.checklist-categories-panel, .today-plan-panel' },
  { name: 'schedule', entry: '.schedule-grid-shell, .schedule-toolbar' },
  { name: 'whitelist', entry: '.whitelist-tabs' },
  { name: 'review', entry: '.review-mode-toggle' },
  { name: 'stats', entry: '.stats-hero-grid, .stats-toolbar-grid' },
  { name: 'alarm', entry: '.alarm-shell .page-header' },
  { name: 'settings', entry: '[aria-label="学习中显示前台规则开关"]' },
];

const ACTIVE_STATES = [
  { name: 'focus', query: 'uiPhase=focus', required: ['打开今日任务', '打开今日日历', '暂停计时', '启用前台规则', '刷新前台状态', '结束学习'] },
  { name: 'paused', query: 'uiPhase=focus&paused=1', required: ['打开今日任务', '打开今日日历', '继续计时', '启用前台规则', '刷新前台状态', '结束学习'] },
  { name: 'awaiting_break', query: 'uiPhase=awaiting_break', required: ['打开今日任务', '打开今日日历', '暂停计时', '确认开始短休息', '跳过休息，开始下一轮专注', '启用前台规则', '刷新前台状态', '结束学习'] },
  { name: 'break', query: 'uiPhase=break', required: ['打开今日任务', '打开今日日历', '跳过休息，开始下一轮专注', '启用前台规则', '刷新前台状态', '结束学习'] },
  { name: 'strict', query: 'uiPhase=focus&mode=strict', required: ['打开今日任务', '打开今日日历', '暂停计时', '启用前台规则', '刷新前台状态'] },
];

const WIDGET_CASES = [
  { name: 'collapsed-square', width: 36, height: 36, query: 'dock=collapsed&edge=right', required: ['.focus-widget-collapsed-tab', '.focus-widget-dot'] },
  { name: 'collapsed-bar', width: 172, height: 36, query: 'dock=collapsed&edge=top', required: ['.focus-widget-collapsed-tab', '.focus-widget-collapsed-tab strong'] },
  { name: 'peek', width: 240, height: 172, query: 'dock=peek&edge=right', required: ['.focus-widget-time', '.focus-widget-subject', '.focus-widget-meta-row', '.focus-widget-actions', '.focus-widget-progress-row'] },
  { name: 'expanded', width: 280, height: 144, query: 'dock=floating', required: ['.focus-widget-time', '.focus-widget-subject', '.focus-widget-meta-row', '.focus-widget-actions', '.focus-widget-progress-row'] },
];

function sizeLabel(size) {
  return `${size.width}x${size.height}`;
}

export async function runResponsiveMatrix(page, options) {
  const { baseUrl, fixturePath, outputDir, reverseValidation = false, screenshots = false, progress = false, scope = 'all' } = options;
  const scopes = new Set(String(scope).split(',').map((item) => item.trim()).filter(Boolean));
  const allow = (name) => scopes.has('all') || scopes.has(name);
  const failures = [];
  const rows = [];
  const pageErrors = [];

  page.setDefaultTimeout(3_000);
  page.setDefaultNavigationTimeout(10_000);

  await mkdir(outputDir, { recursive: true });
  await page.context().addInitScript({ path: fixturePath });
  page.on('pageerror', (error) => pageErrors.push(error.message));

  const check = (condition, label, detail = '') => {
    if (!condition) failures.push(detail ? `${label}: ${detail}` : label);
  };

  const note = (message) => {
    if (progress) console.log(`[ui-responsive] ${message}`);
  };

  const waitForFonts = (targetPage = page) => targetPage.evaluate(() => Promise.race([
    document.fonts?.ready,
    new Promise((resolveFonts) => setTimeout(resolveFonts, 500)),
  ])).catch(() => {});

  const settleLayout = async (targetPage = page) => {
    try {
      await targetPage.evaluate(() => new Promise((resolveLayout) => {
        requestAnimationFrame(() => requestAnimationFrame(resolveLayout));
      }));
    } catch (error) {
      if (!/Execution context was destroyed|navigation|closed/i.test(error instanceof Error ? error.message : String(error))) {
        throw error;
      }
      await targetPage.waitForLoadState('domcontentloaded').catch(() => {});
      await targetPage.evaluate(() => new Promise((resolveLayout) => {
        requestAnimationFrame(() => requestAnimationFrame(resolveLayout));
      })).catch(() => {});
    }
  };

  const gotoReady = async (target, selector) => {
    await page.goto(target, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await page.waitForSelector(selector, { state: 'visible', timeout: 3_000 });
    await waitForFonts();
    await settleLayout();
    const crashed = await page.locator('.app-error-screen').count();
    if (crashed) throw new Error((await page.locator('.app-error-screen').innerText()).replace(/\s+/g, ' '));
    if (reverseValidation) {
      await page.evaluate(() => {
        const sentinel = document.createElement('div');
        sentinel.dataset.uiResponsiveReverse = 'true';
        sentinel.style.cssText = 'position:absolute;left:0;top:0;width:calc(100vw + 32px);height:1px;';
        document.body.append(sentinel);
      });
    }
  };

  const boxIsInViewport = async (selector) => page.locator(selector).first().evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    return rect.width > 0 && rect.height > 0
      && rect.left >= -1 && rect.top >= -1
      && rect.right <= innerWidth + 1 && rect.bottom <= innerHeight + 1
      && style.display !== 'none' && style.visibility !== 'hidden' && style.pointerEvents !== 'none';
  });

  const overflowSnapshot = async (selectors) => page.evaluate((requestedSelectors) => {
    const measure = (element, selector) => element ? {
      selector,
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
      clientHeight: element.clientHeight,
      scrollHeight: element.scrollHeight,
      horizontalOverflow: element.scrollWidth - element.clientWidth,
      verticalOverflow: element.scrollHeight - element.clientHeight,
      overflowX: getComputedStyle(element).overflowX,
      overflowY: getComputedStyle(element).overflowY,
    } : { selector, missing: true };
    return [
      measure(document.documentElement, 'document'),
      measure(document.body, 'body'),
      ...requestedSelectors.map((selector) => measure(document.querySelector(selector), selector)),
    ];
  }, selectors);

  await page.setViewportSize(MAIN_SIZES[0]);
  for (const pageCase of allow('main') ? MAIN_PAGES : []) {
    note(`page/${pageCase.name}`);
    try {
      await gotoReady(`${baseUrl}/?uiPhase=idle&theme=light#${pageCase.name}`, '.app-shell .page-transition section');
    } catch (error) {
      failures.push(`page/${pageCase.name}: ${error instanceof Error ? error.message : String(error)}`);
      continue;
    }
    for (const size of MAIN_SIZES) {
      await page.setViewportSize(size);
      await settleLayout();
      if (pageCase.name === 'settings') {
        await page.locator(pageCase.entry).scrollIntoViewIfNeeded();
        await settleLayout();
      }
      const label = `page/${pageCase.name}/${sizeLabel(size)}`;
      try {
        const snapshot = await overflowSnapshot(['.app-shell', '.main-panel', '.page-transition', '.main-panel .page-transition section']);
        for (const item of snapshot) {
          check(!item.missing, `${label} missing ${item.selector}`);
          check(item.missing || item.horizontalOverflow <= 1, `${label} horizontal overflow ${item.selector}`, JSON.stringify(item));
        }
        const nav = page.locator('.sidebar .nav-item');
        check(await nav.count() === 8, `${label} navigation count`, `actual=${await nav.count()}`);
        check(await page.locator(`.nav-item[aria-current="page"]`).count() === 1, `${label} active navigation marker`);
        check(await boxIsInViewport(pageCase.entry), `${label} primary work entry not visible`, pageCase.entry);
        if (pageCase.name === 'settings') {
          const switchContainerStyle = await page.locator(pageCase.entry).evaluate((input) => {
            const style = getComputedStyle(input.closest('label'));
            return {
              backgroundColor: style.backgroundColor,
              backgroundImage: style.backgroundImage,
              borderWidth: style.borderWidth,
              boxShadow: style.boxShadow,
            };
          });
          check(
            switchContainerStyle.backgroundColor === 'rgba(0, 0, 0, 0)'
              && switchContainerStyle.backgroundImage === 'none'
              && switchContainerStyle.borderWidth === '0px'
              && switchContainerStyle.boxShadow === 'none',
            `${label} foreground switch container must stay transparent`,
            JSON.stringify(switchContainerStyle),
          );
        }
        const unknown = await page.evaluate(() => [...new Set(window.__UI_FIXTURE_UNKNOWN_COMMANDS__ || [])]);
        check(unknown.length === 0, `${label} fixture commands missing`, unknown.join(', '));
        rows.push(`${label} overflow=0 entry=visible nav=8`);
        if (screenshots && pageCase.name === 'settings' && size.width === 960 && size.height === 680) {
          await page.screenshot({ path: `${outputDir}/settings-960x680.png`, animations: 'disabled' });
        }
      } catch (error) {
        failures.push(`${label}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }

  await page.setViewportSize(MAIN_SIZES[0]);
  for (const state of allow('focus') ? ACTIVE_STATES : []) {
    note(`focus/${state.name}`);
    try {
      await gotoReady(`${baseUrl}/?${state.query}&theme=light#focus`, '.focus-active-shell');
    } catch (error) {
      failures.push(`focus/${state.name}: ${error instanceof Error ? error.message : String(error)}`);
      continue;
    }
    for (const size of MAIN_SIZES) {
      await page.setViewportSize(size);
      await settleLayout();
      const label = `focus/${state.name}/${sizeLabel(size)}`;
      try {
        const snapshot = await overflowSnapshot(['.app-shell', '.main-panel', '.page-transition', '.focus-active-shell']);
        for (const item of snapshot) {
          check(!item.missing, `${label} missing ${item.selector}`);
          check(item.missing || item.horizontalOverflow <= 1, `${label} horizontal overflow ${item.selector}`, JSON.stringify(item));
          check(item.missing || item.verticalOverflow <= 1 || (['.main-panel', '.page-transition', '.focus-active-shell'].includes(item.selector) && ['clip', 'hidden'].includes(item.overflowY)), `${label} vertical scroll ${item.selector}`, JSON.stringify(item));
        }
        const nav = page.locator('.sidebar .nav-item');
        check(await nav.count() === 8, `${label} navigation count`, `actual=${await nav.count()}`);
        check(await boxIsInViewport('.sidebar'), `${label} sidebar not visible`);
        check(await boxIsInViewport('.focus-clock-zone strong'), `${label} countdown not visible`);
        for (const ariaLabel of state.required) {
          const selector = `[aria-label="${ariaLabel}"]`;
          check(await page.locator(selector).count() === 1, `${label} missing control`, ariaLabel);
          if (await page.locator(selector).count()) check(await boxIsInViewport(selector), `${label} clipped control`, ariaLabel);
        }
        const unknown = await page.evaluate(() => [...new Set(window.__UI_FIXTURE_UNKNOWN_COMMANDS__ || [])]);
        check(unknown.length === 0, `${label} fixture commands missing`, unknown.join(', '));
        rows.push(`${label} overflow=0 scroll=0 controls=${state.required.length} nav=8`);
      } catch (error) {
        failures.push(`${label}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }

  if (allow('focus')) {
    note('focus/foreground-rule-toggle');
    try {
      await page.setViewportSize(MAIN_SIZES[0]);
      await gotoReady(`${baseUrl}/?uiPhase=focus&theme=light#focus`, '.focus-active-shell');
      const foregroundRuleSwitch = page.locator('[aria-label="启用前台规则"]');
      check(await foregroundRuleSwitch.isChecked(), 'focus/foreground-rule-toggle initially enabled');
      await foregroundRuleSwitch.uncheck();
      await settleLayout();
      check(!(await foregroundRuleSwitch.isChecked()), 'focus/foreground-rule-toggle did not turn off');
      check(await page.getByText('前台规则已关闭', { exact: true }).count() > 0, 'focus/foreground-rule-toggle status did not update');

      await gotoReady(`${baseUrl}/?uiPhase=focus&mode=strict&theme=light#focus`, '.focus-active-shell');
      const strictForegroundRuleSwitch = page.locator('[aria-label="启用前台规则"]');
      check(await strictForegroundRuleSwitch.isChecked(), 'focus/strict foreground rules not enabled');
      check(await strictForegroundRuleSwitch.isDisabled(), 'focus/strict foreground rule switch not locked');

      await gotoReady(`${baseUrl}/?uiPhase=focus&showForegroundRuleToggle=0&theme=light#focus`, '.focus-active-shell');
      check(await page.locator('[aria-label="启用前台规则"]').count() === 0, 'focus/hidden foreground rule switch still visible');
      check(await page.locator('.live-badge').count() === 1, 'focus/hidden foreground rule status missing');
      rows.push('focus/foreground-rule-toggle normal=mutable strict=locked');
    } catch (error) {
      failures.push(`focus/foreground-rule-toggle: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  if (allow('navigation')) {
    await page.setViewportSize(MAIN_SIZES[0]);
    note('focus/navigation-click');
    try {
      await gotoReady(`${baseUrl}/?uiPhase=focus&theme=light#focus`, '.focus-active-shell');
    } catch (error) {
      failures.push(`focus/navigation-click: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  for (const size of allow('navigation') ? MAIN_SIZES : []) {
    await page.setViewportSize(size);
    await settleLayout();
    const navLabel = `focus/navigation-click/${sizeLabel(size)}`;
    try {
      if (await page.locator('.focus-active-shell').count() === 0) {
        await page.locator('.nav-item[aria-keyshortcuts="Alt+1"]').click();
        await page.waitForSelector('.focus-active-shell', { state: 'visible' });
      }
      await page.locator('.nav-item[aria-keyshortcuts="Alt+2"]').click();
      await page.waitForSelector('.checklist-shell', { state: 'visible' });
      check((await page.title()).startsWith('清单'), `${navLabel} click did not navigate`, await page.title());
      rows.push(`${navLabel} passed`);
    } catch (error) {
      failures.push(`${navLabel}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  if (allow('drawers')) {
    await page.setViewportSize(MAIN_SIZES[0]);
  }
  try {
    if (!allow('drawers')) throw new Error('__SKIP_DRAWERS__');
    note('focus/drawers');
    await gotoReady(`${baseUrl}/?uiPhase=focus&theme=light#focus`, '.focus-active-shell');
    for (const drawerCase of [
      { open: '打开今日任务', drawer: '.today-plan-drawer', close: '关闭今日任务' },
      { open: '打开今日日历', drawer: '.schedule-drawer', close: '关闭日历' },
    ]) {
      await page.locator(`[aria-label="${drawerCase.open}"]`).click();
      await page.waitForSelector(`${drawerCase.drawer}.is-open`, { state: 'visible' });
      await page.waitForTimeout(260);
      await settleLayout();
      const overlay = await page.locator(`${drawerCase.drawer}.is-open`).evaluate((element) => {
        const rect = element.getBoundingClientRect();
        return { position: getComputedStyle(element).position, left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, viewport: [innerWidth, innerHeight] };
      });
      check(overlay.position === 'fixed', `focus/drawer ${drawerCase.drawer} is not fixed`, JSON.stringify(overlay));
      check(overlay.left >= 0 && overlay.top >= 0 && overlay.right <= overlay.viewport[0] + 1 && overlay.bottom <= overlay.viewport[1] + 1, `focus/drawer ${drawerCase.drawer} clipped`, JSON.stringify(overlay));
      await page.locator(`[aria-label="${drawerCase.close}"]`).click();
        await page.waitForSelector(`${drawerCase.drawer}:not(.is-open)`, { state: 'attached' });
      await page.waitForTimeout(260);
      await settleLayout();
      const closed = await overflowSnapshot(['.app-shell', '.main-panel', '.page-transition', '.focus-active-shell']);
      check(closed.every((item) => !item.missing && item.horizontalOverflow <= 1 && item.verticalOverflow <= 1), `focus/drawer ${drawerCase.drawer} closed overflow`, JSON.stringify(closed));
    }
    rows.push('focus/drawers/960x680 fixed-overlay closed-overflow=0');
    if (screenshots) {
      await page.screenshot({ path: `${outputDir}/focus-active-960x680.png`, animations: 'disabled' });
    }
  } catch (error) {
    if ((error instanceof Error ? error.message : String(error)) !== '__SKIP_DRAWERS__') {
      failures.push(`focus/drawers/960x680: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  const runWidgetCases = async (widgetPage, dpiLabel) => {
    for (const widgetCase of WIDGET_CASES) {
      note(`widget/${widgetCase.name}/${dpiLabel}`);
      await widgetPage.setViewportSize({ width: widgetCase.width, height: widgetCase.height });
      const label = `widget/${widgetCase.name}/${widgetCase.width}x${widgetCase.height}/${dpiLabel}`;
      try {
        await widgetPage.goto(`${baseUrl}/?windowLabel=focus-widget&uiPhase=focus&${widgetCase.query}`, { waitUntil: 'domcontentloaded' });
        await widgetPage.waitForSelector('.focus-widget-shell', { state: 'visible' });
        await waitForFonts(widgetPage);
        await settleLayout(widgetPage);
        const snapshot = await widgetPage.evaluate(() => {
          const selectors = ['html', 'body', '#root', '.focus-widget-shell', '.focus-widget-panel'];
          return selectors.map((selector) => {
            const element = document.querySelector(selector);
            return element ? { selector, clientWidth: element.clientWidth, scrollWidth: element.scrollWidth, clientHeight: element.clientHeight, scrollHeight: element.scrollHeight } : { selector, missing: true };
          });
        });
        for (const item of snapshot) {
          if (item.missing && item.selector === '.focus-widget-panel' && widgetCase.name.startsWith('collapsed')) continue;
          check(!item.missing, `${label} missing ${item.selector}`);
          check(item.missing || item.scrollWidth - item.clientWidth <= 1, `${label} horizontal overflow ${item.selector}`, JSON.stringify(item));
          check(item.missing || item.scrollHeight - item.clientHeight <= 1, `${label} vertical overflow ${item.selector}`, JSON.stringify(item));
        }
        for (const selector of widgetCase.required) {
          const count = await widgetPage.locator(selector).count();
          check(count > 0, `${label} missing widget content`, selector);
          if (count) {
            const visible = await widgetPage.locator(selector).first().evaluate((element) => {
              const rect = element.getBoundingClientRect();
              return rect.width > 0 && rect.height > 0 && rect.left >= -1 && rect.top >= -1 && rect.right <= innerWidth + 1 && rect.bottom <= innerHeight + 1;
            });
            check(visible, `${label} clipped widget content`, selector);
          }
        }
        const unknown = await widgetPage.evaluate(() => [...new Set(window.__UI_FIXTURE_UNKNOWN_COMMANDS__ || [])]);
        check(unknown.length === 0, `${label} fixture commands missing`, unknown.join(', '));
        rows.push(`${label} overflow=0 content=${widgetCase.required.length}`);
        if (screenshots && (widgetCase.name === 'peek' || widgetCase.name === 'expanded')) {
          await widgetPage.screenshot({ path: `${outputDir}/widget-${widgetCase.name}-${dpiLabel}.png`, animations: 'disabled' });
        }
      } catch (error) {
        failures.push(`${label}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  };

  if (allow('widget')) await runWidgetCases(page, '100dpi');
  const browser = allow('widget') ? page.context().browser() : null;
  if (browser) {
    const dpiContext = await browser.newContext({ deviceScaleFactor: 1.5, viewport: { width: 280, height: 144 } });
    await dpiContext.addInitScript({ path: fixturePath });
    const dpiPage = await dpiContext.newPage();
    dpiPage.setDefaultTimeout(3_000);
    dpiPage.setDefaultNavigationTimeout(10_000);
    await runWidgetCases(dpiPage, '150dpi');
    await dpiContext.close();
  } else if (allow('widget')) {
    failures.push('widget/150dpi: browser context unavailable');
  }

  check(pageErrors.length === 0, 'browser page errors', [...new Set(pageErrors)].join(' | '));
  return {
    marker: failures.length === 0 ? '__UI_RESPONSIVE_PASS__' : '__UI_RESPONSIVE_FAIL__',
    failures,
    rows,
    summary: {
      mainPageCases: MAIN_SIZES.length * MAIN_PAGES.length,
      activeFocusCases: MAIN_SIZES.length * ACTIVE_STATES.length,
      navigationClicks: MAIN_SIZES.length,
      drawerCases: 2,
      widgetCases: WIDGET_CASES.length * 2,
      pageErrors: pageErrors.length,
    },
  };
}
