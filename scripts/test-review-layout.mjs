/**
 * 复盘页可写性回归校验（静态契约）。
 *
 * 修过的线上问题：复盘页三个输入框每栏只能看到两行字。
 * 根因是 `apple-hig.css` 里那组「输入控件统一高度」的选择器同时列了
 * `textarea` 和 `.review-textarea`，而它在 `styles.css` 之后加载，
 * 把 `styles.css` 的 `.review-textarea { min-height: 132px }` 压回
 * `--control-height`（36px）。36px 减去上下 padding 只剩约两行正文。
 *
 * 这里守住契约本身（高度、自动增高、布局比例），真实渲染的像素级
 * 验证见 `npm run test:ui-responsive`。
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(process.cwd());
const read = (relativePath) => readFileSync(resolve(root, relativePath), 'utf8').replace(/\r\n/g, '\n');

let failed = false;
function assert(condition, message) {
  if (!condition) {
    console.error(`FAIL: ${message}`);
    failed = true;
  }
}

/** 取某个选择器块的声明体，例如 `.review-textarea { ... }`；只认行首的选择器，避免命中注释里的同名文字。 */
function ruleBody(source, selector) {
  const lines = source.split('\n');
  const target = `${selector} {`;
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trimEnd() !== target && lines[index].trim() !== target) continue;
    const rest = lines.slice(index).join('\n');
    const end = rest.indexOf('}');
    if (end < 0) return null;
    return rest.slice(0, end + 1);
  }
  return null;
}

const appleHig = read('src/apple-hig.css');
const styles = read('src/styles.css');
const page = read('src/pages/ReviewPage.tsx');

// ---------- 1. apple-hig 的「统一输入框高度」不许把复盘框压扁 ----------
// 定位那组会命中 textarea 的规则，确认它把 textarea 和 .review-textarea 一起包了进来。
const controlGroupStart = appleHig.indexOf('input[type=\'search\'],');
assert(controlGroupStart > 0, '未找到 apple-hig 的输入控件高度分组');
const controlGroup = appleHig.slice(controlGroupStart, appleHig.indexOf('}', controlGroupStart) + 1);
assert(
  controlGroup.includes('textarea,'),
  '输入控件分组应仍包含 textarea（其它页面依赖它）',
);
assert(
  controlGroup.includes('min-height: var(--control-height)'),
  '输入控件分组应把高度压到 --control-height',
);

// 关键：这条 36px 规则之后必须有专门把复盘框抬回多行高度的规则。
const reviewOverride = ruleBody(appleHig, '.review-textarea');
assert(reviewOverride !== null, 'apple-hig.css 必须为 .review-textarea 单独声明高度，覆盖 36px 的控件高度');
const overrideMin = reviewOverride && /min-height:\s*(\d+)px/.exec(reviewOverride);
assert(overrideMin !== null, 'apple-hig.css 的 .review-textarea 必须给出 px 级 min-height');
if (overrideMin) {
  assert(
    Number(overrideMin[1]) >= 160,
    `.review-textarea 的 min-height 至少 160px（当前 ${overrideMin[1]}px）：36px 只够两行`,
  );
}

// 覆盖规则必须排在那组 36px 规则之后，否则同权重下会被盖回去。
const overrideIndex = appleHig.indexOf('.review-textarea {');
assert(
  overrideIndex > controlGroupStart,
  '.review-textarea 的高度覆盖必须写在控件分组之后，否则会被 36px 盖掉',
);

// ---------- 2. styles.css 同源声明保持一致 ----------
const stylesReview = ruleBody(styles, '.review-textarea');
assert(stylesReview !== null, 'styles.css 应保留 .review-textarea 规则');
const stylesMin = stylesReview && /min-height:\s*(\d+)px/.exec(stylesReview);
assert(stylesMin !== null, 'styles.css 的 .review-textarea 应以 px 给出 min-height');
if (stylesMin && overrideMin) {
  assert(
    Number(stylesMin[1]) >= 160,
    `styles.css 的 min-height 至少 160px（当前 ${stylesMin[1]}px）`,
  );
}
assert(
  /max-height:\s*\d+px/.test(stylesReview ?? ''),
  '复盘框必须设 max-height：自动增高需要有上限，否则长文会把页面撑爆',
);
assert(
  /overflow-y:\s*auto/.test(stylesReview ?? ''),
  '复盘框必须允许滚动，达到 max-height 后仍可继续写',
);
assert(
  /resize:\s*vertical/.test(stylesReview ?? ''),
  '复盘框应保留手动纵向拖拽',
);

// ---------- 3. 输入框随内容自动长高 ----------
assert(
  page.includes('textarea.style.height = `${textarea.scrollHeight}px`'),
  '复盘输入框必须按 scrollHeight 自动长高：固定小框会把长复盘逼到反复滚动',
);
assert(
  /textarea\.style\.height = 'auto'/.test(page),
  '自动增高前必须先把高度重置为 auto，否则内容变短时不会回落',
);
const heightEffect = page.slice(page.indexOf('textarea.style.height'), page.indexOf('rows={6}'));
assert(
  /\}, \[value\]\)/.test(page),
  '自动增高必须依赖 value，AI 填稿和载入历史复盘时同样要撑开',
);
assert(
  page.includes('ref={textareaRef}'),
  '自动增高需要 textarea 的 ref',
);
assert(
  !/rows=\{2\}/.test(page),
  '复盘输入框不能用 rows={2}：常见窗口下就只显示两行',
);

// ---------- 4. 布局给编辑区让出主要宽度 ----------
const gridRules = [...styles.matchAll(/\.review-grid \{[^}]*\}/g)].map((match) => match[0]);
assert(gridRules.length >= 1, '未找到 .review-grid 规则');
const wideGrid = gridRules.find((rule) => rule.includes('minmax(0, 1.55fr)'));
assert(
  wideGrid !== undefined,
  '宽屏下编辑区应拿到更大的比例（1.55fr），复盘页的主体是写东西而不是看概览',
);
assert(
  /minmax\(260px, 0\.68fr\)/.test(wideGrid ?? ''),
  '宽屏下概览栏应收窄到 0.68fr，把宽度让给编辑区',
);

// 移动端仍是单列，不能被上面的比例改坏。
assert(
  /@media \(max-width: \d+px\) \{[\s\S]*?\.review-grid \{\s*grid-template-columns: 1fr;/.test(styles),
  '窄屏下 .review-grid 必须回落成单列',
);

// ---------- 5. 三个字段都走同一个组件，避免只修好其中一栏 ----------
for (const id of ['daily-summary', 'daily-blockers', 'daily-focus', 'weekly-summary', 'weekly-blockers', 'weekly-focus']) {
  assert(page.includes(`id="${id}"`), `缺少复盘字段 ${id}`);
}
const reviewFieldCount = (page.match(/<ReviewField /g) ?? []).length;
assert(
  reviewFieldCount === 6,
  `六个复盘字段都要用 ReviewField（当前 ${reviewFieldCount} 个），否则自动增高只对部分栏目生效`,
);

if (failed) {
  console.error('\n复盘页布局契约校验失败。');
  process.exit(1);
}

console.log('复盘页布局契约校验通过：');
console.log('  - 复盘输入框高度不再被 apple-hig 的 36px 控件高度压扁（≥160px，可滚动、可拖拽）');
console.log('  - 输入框随内容自动长高，长复盘不再被锁在小框里');
console.log('  - 宽屏下编辑区拿到主导宽度，概览栏让位');
console.log('  - 日/周各三个字段全部走同一组件');
