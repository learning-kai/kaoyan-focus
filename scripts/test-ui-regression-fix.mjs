import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(process.cwd());
const read = (rel) => readFileSync(resolve(root, rel), 'utf8');

let failed = false;
const fail = (msg) => {
  console.error(msg);
  failed = true;
};

const hig = read('src/apple-hig.css');
const styles = read('src/styles.css');
const schedulePage = read('src/pages/SchedulePage.tsx');

// Task1: drawer-enter only on is-open rules
const bareDrawerEnter = [...hig.matchAll(/^[^{\n][^{]*\{[^}]*drawer-enter[^}]*\}/gms)];
// Simpler: any line with drawer-enter that is not keyframes and whose preceding selector block lacks is-open
const lines = hig.split(/\r?\n/);
for (let i = 0; i < lines.length; i++) {
  const line = lines[i];
  if (!line.includes('drawer-enter')) continue;
  if (line.includes('@keyframes') || line.includes('/*')) continue;
  // look backward for selector
  let j = i;
  while (j >= 0 && !lines[j].includes('{')) j--;
  // go further back to collect selector lines
  let k = j - 1;
  const selector = [];
  while (k >= 0 && lines[k].trim() !== '' && !lines[k].includes('}')) {
    selector.unshift(lines[k].trim());
    k--;
  }
  const sel = selector.join(' ');
  if (!sel.includes('is-open')) {
    fail(`drawer-enter bound without is-open near line ${i + 1}: ${sel || lines[i]}`);
  }
}

// Closed drawer still has hide mechanics in styles
for (const cls of ['.schedule-drawer', '.today-plan-drawer']) {
  if (!styles.includes(cls)) fail(`missing ${cls} in styles.css`);
}
if (!/transform:\s*translateX\(calc\(100%/.test(styles)) {
  fail('styles.css missing closed translateX hide');
}
if (!styles.includes('pointer-events: none')) {
  fail('styles.css missing pointer-events none for closed drawers');
}
// hig closed reinforcement
if (!hig.includes('.schedule-drawer:not(.is-open)') || !hig.includes('.today-plan-drawer:not(.is-open)')) {
  fail('apple-hig missing :not(.is-open) closed drawer rules');
}
if (!/opacity:\s*0\s*!important/.test(hig)) {
  fail('apple-hig closed drawer missing opacity 0');
}

// Task2: no page-shell padding 0 !important
if (/page-shell[\s\S]{0,200}padding:\s*0\s*!important/.test(hig)) {
  // more precise: the shell group
  const m = hig.match(/\.page-shell,[\s\S]*?\{([\s\S]*?)\}/);
  if (m && /padding:\s*0\s*!important/.test(m[1])) {
    fail('page-shell still has padding:0 !important');
  }
}
if (/clamp\(\s*24px\s*,\s*2\.4vw\s*,\s*30px\s*\)/.test(hig)) {
  fail('h2 still uses oversized clamp(24px, 2.4vw, 30px)');
}
// main cards use radius-card
if (!hig.includes('border-radius: var(--radius-card') && !hig.includes('border-radius: var(--radius-card,')) {
  fail('main cards not using --radius-card');
}

// Task3: schedule complete
if (!schedulePage.includes('completeTodayPlanItem')) {
  fail('SchedulePage missing completeTodayPlanItem');
}
if (!schedulePage.includes('handleCompleteTodayItem')) {
  fail('SchedulePage missing handleCompleteTodayItem');
}
if (!schedulePage.includes('标记完成') && !schedulePage.includes("aria-label={item.completed ? '恢复为未完成' : '标记完成'}")) {
  fail('SchedulePage missing complete control aria');
}
if (!schedulePage.includes('is-completed')) {
  fail('SchedulePage missing is-completed class binding');
}

if (failed) {
  process.exit(1);
}

console.log('UI regression fix assertions passed');
