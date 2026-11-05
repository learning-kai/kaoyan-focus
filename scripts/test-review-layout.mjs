/**
 * 输入控件与列表文字的布局契约（第一性原理版）。
 *
 * 这里守护的是「分层语义」，不是某个具体像素值：
 *
 *   1. 单行控件的高度语义 = 一行文字高（--control-height）。
 *   2. 多行控件（textarea）的高度语义 = 能写几行，绝不能套单行的高度。
 *   3. 省略号（ellipsis）只能由「这一行确实放不下」触发，
 *      不能因为「元素在某类容器里」就无差别加上。
 *
 * 曾经的三个真实故障，全都是违反了上面某一条：
 *   - 复盘只能看两行：单行控件分组把 textarea 一起圈进去，多行被压成 36px。
 *   - 科目名变成「数学…」：一条 `.soft-panel strong` 兜底规则给所有标题
 *     加了 overflow:hidden + min-width:0，flex 里就被压到内容宽度以下。
 *   - 日历标题整行消失：左栏宽度写死 230px，右侧按钮按内容吃宽度，
 *     标题列被挤成 0px。
 *
 * 断言「分层正确」而不是「某个值等于 168px」，这样具体数值可以自由调整，
 * 但架构再也不能退回旧样子。真实渲染验证见 `npm run test:ui-responsive`。
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

/** 去掉 CSS 注释，避免注释里的文字干扰解析。 */
function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, '');
}

/** 收集某个选择器在所有规则块中的声明（同一选择器可能出现在多条规则里）。 */
function ruleBody(source, selector) {
  const clean = stripComments(source);
  const lines = clean.split('\n');
  const bodies = [];
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index].trim();
    // 选择器可能与其他选择器并列（`.a,` / `.b {`），只要本行以它开头即可。
    if (line !== selector && line !== `${selector} {` && !line.startsWith(`${selector},`)) continue;
    const rest = lines.slice(index).join('\n');
    const open = rest.indexOf('{');
    const end = rest.indexOf('}', open);
    if (open < 0 || end < 0) continue;
    bodies.push(rest.slice(open + 1, end));
  }
  return bodies.length ? bodies.join('\n') : null;
}

/** 找到包含某个选择器的那组「共用声明」规则，返回其选择器列表与声明体。 */
function groupContaining(source, selector) {
  const clean = stripComments(source);
  const blocks = [...clean.matchAll(/(^[^{}\n][^{}]*?)\{([^{}]*)\}/gm)];
  for (const block of blocks) {
    const selectors = block[1];
    if (!selectors.split(',').some((item) => item.trim() === selector)) continue;
    return { selectors: selectors.split(',').map((item) => item.trim()), body: block[2] };
  }
  return null;
}

const appleHig = read('src/apple-hig.css');
const styles = read('src/styles.css');
const page = read('src/pages/ReviewPage.tsx');

// ---------- 契约 1：单行控件分组不许再圈进 textarea ----------
const singleLineGroup = groupContaining(appleHig, '.text-input');
assert(singleLineGroup !== null, 'apple-hig.css 应存在单行控件样式组（.text-input …）');
if (singleLineGroup) {
  assert(
    singleLineGroup.body.includes('min-height: var(--control-height)'),
    '单行控件组应当使用 --control-height 作为高度',
  );
  assert(
    !singleLineGroup.selectors.includes('textarea'),
    '单行控件组不能包含 textarea：多行控件套单行高度就是「只能看两行」的根因',
  );
  assert(
    !singleLineGroup.selectors.includes('.review-textarea'),
    '单行控件组不能包含 .review-textarea，同理',
  );
}

// ---------- 契约 2：多行控件有独立的高度语义 ----------
const multiLine = ruleBody(appleHig, "textarea:not([rows='1'])");
assert(multiLine !== null, 'apple-hig.css 必须为多行 textarea 单独定义规则（与单行分开）');
if (multiLine) {
  assert(
    !/min-height:\s*var\(--control-height\)/.test(multiLine),
    '多行 textarea 不能再用 --control-height：那是单行的高度语义',
  );
  const minPx = /min-height:\s*(\d+)px/.exec(multiLine);
  assert(minPx !== null, '多行 textarea 应以 px 给出 min-height（可写好几行）');
  if (minPx) {
    assert(
      Number(minPx[1]) >= 60,
      `多行 textarea 的 min-height 至少要能放下三行正文（当前 ${minPx[1]}px）`,
    );
  }
}

// 复盘属于长文写作，应当比普通多行输入更高，且有上限。
// 同一选择器可能出现多条规则（共享外观 + 专属尺寸），取其中最大的 min-height 为准。
const reviewRule = ruleBody(appleHig, '.review-textarea');
assert(reviewRule !== null, 'apple-hig.css 应为 .review-textarea 定义长文规格');
if (reviewRule) {
  const minValues = [...reviewRule.matchAll(/min-height:\s*(\d+)px/g)].map((m) => Number(m[1]));
  const maxValues = [...reviewRule.matchAll(/max-height:\s*(\d+)px/g)].map((m) => Number(m[1]));
  assert(minValues.length > 0, '.review-textarea 需要 px 级 min-height');
  assert(maxValues.length > 0, '.review-textarea 需要 px 级 max-height：自动增高必须有上限');
  if (minValues.length && maxValues.length) {
    const minPx = Math.max(...minValues);
    const maxPx = Math.max(...maxValues);
    assert(
      minPx >= 150,
      `.review-textarea 至少 150px 高（当前 ${minPx}px）`,
    );
    assert(
      maxPx > minPx,
      '.review-textarea 的 max-height 必须大于 min-height，否则自动增高无意义',
    );
  }
}

// 尺寸只应有一处权威定义，避免两个文件各写一套互相打架。
const stylesReview = ruleBody(styles, '.review-textarea');
assert(stylesReview !== null, 'styles.css 应保留 .review-textarea 的视觉规则');
if (stylesReview) {
  assert(
    !/min-height\s*:/.test(stylesReview) && !/max-height\s*:/.test(stylesReview),
    'styles.css 不应重复定义 .review-textarea 的尺寸：尺寸已统一由 apple-hig.css 给出',
  );
  assert(/resize:\s*vertical/.test(stylesReview), '复盘框应保留手动纵向拖拽');
  assert(/overflow-y:\s*auto/.test(stylesReview), '复盘框达到 max-height 后必须可滚动');
}

// ---------- 契约 3：省略号只能由「放不下」触发，不能由容器类型触发 ----------
// 旧写法：一条 .soft-panel strong 兜底规则给所有标题加死 nowrap + min-width:0。
const allStrongNowrap = /\.soft-panel strong[\s\S]{0,400}?white-space:\s*nowrap/.test(appleHig);
assert(
  !allStrongNowrap,
  '不能再有「.soft-panel strong 一律 nowrap + 省略」的兜底规则：那会把 flex 里的标题压到内容宽度以下',
);

// 日程块标题必须「有上限」，不能无限撑破色块。
// 早期要求死守 nowrap 单行，但实测 82px 宽的紧凑卡片会把
// 「数学强化阶段高等数学…」压成「数…」，用户看不到任务名。
// 现在改为最多两行（line-clamp），既完整可读又不会撑破卡片。
assert(
  /\.schedule-block strong[\s\S]{0,600}?(-webkit-)?line-clamp:\s*2/.test(appleHig),
  '日程块标题应限制最多两行（line-clamp），既能读到任务名又不会撑破色块',
);
assert(
  /\.schedule-block strong[\s\S]{0,600}?white-space:\s*normal/.test(appleHig),
  '日程块标题应允许折行，而不是一律单行省略',
);

// 列表类标题应允许换行显示完整名称。
assert(
  /\.schedule-task-row strong[\s\S]{0,600}?overflow-wrap:\s*anywhere/.test(appleHig)
    || /\.list-row strong[\s\S]{0,600}?overflow-wrap:\s*anywhere/.test(appleHig),
  '列表行标题应允许换行/断词，而不是一律省略',
);

// ---------- 契约 4：日历左栏不能写死到装不下标题 ----------
const scheduleGrid = ruleBody(styles, '.schedule-grid-shell');
assert(scheduleGrid !== null, 'styles.css 应存在 .schedule-grid-shell 栅格定义');
// 宽屏时左栏必须有更大的档位，否则窗口变宽只有中间列在长。
assert(
  /@media \(min-width: 1400px\)[\s\S]{0,300}\.schedule-grid-shell/.test(styles),
  '宽屏需要为日历左栏提供更宽的栅格档位：固定 230px 会把标题挤成 0px',
);
const compactRow = ruleBody(appleHig, '.schedule-task-row');
assert(compactRow !== null, 'apple-hig.css 应定义 .schedule-task-row 布局');
assert(
  !/grid-template-columns:\s*minmax\(0, 1fr\) auto/.test(compactRow),
  '今日待排行不能用 `1fr auto`：auto 列按内容吃宽度，左栏窄时标题会被压成 0px',
);

// ---------- 契约 5：复盘字段自动增高 ----------
assert(
  page.includes('textarea.style.height = `${textarea.scrollHeight}px`'),
  '复盘输入框必须按 scrollHeight 自动长高',
);
assert(
  /textarea\.style\.height = 'auto'/.test(page),
  '自动增高前必须重置为 auto，否则内容变短不会回落',
);
assert(/\}, \[value\]\)/.test(page), '自动增高必须依赖 value，AI 填稿与载入历史时也要撑开');
assert(!/rows=\{2\}/.test(page), '复盘输入框不能用 rows={2}');
assert((page.match(/<ReviewField /g) ?? []).length === 6, '日/周各三个字段都要走 ReviewField');

if (failed) {
  console.error('\n输入控件与列表文字的布局契约校验失败。');
  process.exit(1);
}

console.log('布局契约校验通过（分层语义）：');
console.log('  - 单行控件用 --control-height；多行 textarea 有独立高度语义，不再被压成单行');
console.log('  - 复盘长文框有 min/max 高度、可滚动可拖拽，尺寸只在 apple-hig.css 定义一处');
console.log('  - 省略号由「放不下」触发，不再按容器类型无差别 nowrap');
console.log('  - 日历左栏在宽屏下有更宽档位，今日待排行不把标题挤成 0px');
console.log('  - 复盘字段随内容自动增高');
