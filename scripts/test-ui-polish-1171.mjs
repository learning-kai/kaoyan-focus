import { readFileSync, writeFileSync, copyFileSync, unlinkSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(process.cwd());
const read = (rel) => readFileSync(resolve(root, rel), 'utf8');

let failed = false;
const fail = (msg) => {
  console.error('FAIL:', msg);
  failed = true;
};

const schedulePage = read('src/pages/SchedulePage.tsx');
const styles = read('src/styles.css');
const hig = read('src/apple-hig.css');

// ---- Task1: timeline complete near schedule-block-actions ----
const actionsIdx = schedulePage.indexOf('schedule-block-actions');
if (actionsIdx < 0) fail('missing schedule-block-actions');
else {
  const window = schedulePage.slice(actionsIdx, actionsIdx + 1200);
  if (!window.includes('handleCompleteTodayItem')) {
    fail('schedule-block-actions window missing handleCompleteTodayItem');
  }
  if (!window.includes('source_today_item_id')) {
    fail('schedule-block-actions window missing source_today_item_id guard');
  }
  if (!window.includes('stopPropagation')) {
    fail('timeline complete missing stopPropagation');
  }
  if (!window.includes('标记完成') && !window.includes("恢复为未完成")) {
    fail('timeline complete missing aria labels');
  }
}
if (!schedulePage.includes('completeTodayPlanItem')) {
  fail('SchedulePage missing completeTodayPlanItem import/use');
}
if (!schedulePage.includes('is-complete-toggle')) {
  fail('SchedulePage missing is-complete-toggle class');
}

// ---- Task2: card radius + overflow ----
if (!/border-radius:\s*var\(--radius-card/.test(styles) || !styles.includes('.schedule-block')) {
  // more precise: schedule-block block uses radius-card
}
{
  const m = styles.match(/\.schedule-block\s*\{[\s\S]*?\}/);
  if (!m) fail('styles missing .schedule-block rule');
  else {
    if (!/border-radius:\s*var\(--radius-card/.test(m[0])) fail('styles .schedule-block not using --radius-card');
    if (!/overflow:\s*hidden/.test(m[0])) fail('styles .schedule-block missing overflow:hidden');
    if (!/min-width:\s*0/.test(m[0])) fail('styles .schedule-block missing min-width:0');
  }
}
if (!/border-radius:\s*var\(--radius-card,\s*12px\)\s*!important/.test(hig) &&
    !/\.schedule-page\s+\.schedule-block\s*\{[\s\S]*?border-radius:\s*var\(--radius-card/.test(hig)) {
  fail('apple-hig schedule-block not on radius-card');
}
// content cards should not force 4px/6px on schedule-block
if (/\.schedule-page\s+\.schedule-block\s*\{[^}]*border-radius:\s*10px/.test(hig)) {
  fail('apple-hig still forces schedule-block 10px');
}
if (!hig.includes('text-overflow: ellipsis') || !hig.includes('.schedule-block strong')) {
  fail('apple-hig missing schedule-block text ellipsis rules');
}

// ---- Task3: countdown enlarged ----
if (/clamp\(\s*72px\s*,\s*14vw\s*,\s*120px\s*\)/.test(styles)) {
  fail('old focus-clock-zone clamp(72px,14vw,120px) still present');
}
if (/clamp\(\s*52px\s*,\s*8vw\s*,\s*96px\s*\)/.test(styles)) {
  fail('old timer-orbit clamp(52px,8vw,96px) still present');
}
if (!/clamp\(\s*88px\s*,\s*16vw\s*,\s*140px\s*\)/.test(styles)) {
  fail('focus-clock-zone strong missing clamp(88px,16vw,140px)');
}
if (!/clamp\(\s*64px\s*,\s*10vw\s*,\s*112px\s*\)/.test(styles)) {
  fail('timer-orbit strong missing clamp(64px,10vw,112px)');
}

// ---- reverse verification helpers (opt-in via env) ----
if (process.env.UI_POLISH_REVERSE === '1') {
  // This mode is used manually in PROGRESS; if set, force fail after printing expected
  fail('UI_POLISH_REVERSE forced failure for reverse verification');
}

if (failed) {
  process.exit(1);
}
console.log('UI polish 1.17.1 assertions passed');
