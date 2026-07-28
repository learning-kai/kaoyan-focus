import { readFileSync, writeFileSync, copyFileSync, unlinkSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const root = process.cwd();
const read = (rel) => readFileSync(rel, 'utf8');

let failed = false;
const fail = (msg) => {
  console.error('FAIL:', msg);
  failed = true;
};

const sp = read('src/pages/SchedulePage.tsx');
const st = read('src/styles.css');
const hig = read('src/apple-hig.css');

const assert = (cond, msg) => { if (!cond) fail(msg); };

// Task1: timeline complete
assert(sp.includes('handleCompleteTodayItem'), 'no handleCompleteTodayItem');
assert(sp.includes('source_today_item_id'), 'no source_today_item_id');
assert(sp.includes('stopPropagation'), 'no stopPropagation');
assert(sp.includes('is-complete-toggle'), 'no is-complete-toggle');

// Task2-3: button/card
assert(/--radius-control/.test(hig) || /border-radius: var\(--radius-control/.test(st), 'no radius-control');
assert(/min-height: 36/.test(st) || /min-height: 36/.test(hig), 'no 36 height');
assert(/overflow: hidden/.test(st) && /min-width: 0/.test(st), 'overflow not present');
assert(/min-height: 48/.test(st) || /schedule-block.*min-height: 48/.test(hig), 'no 48 min-height');

// Task4: countdown
assert(/clamp\(104px, 18vw, 160px\)/.test(st), 'no 104px countdown');
assert(/clamp\(76px, 12vw, 128px\)/.test(st), 'no 76px orbit');
assert(/clamp\(103px, 24vmin, 264px\)/.test(st), 'no 103px fullscreen');

if (failed) {
  console.error('polish assertions failed');
  process.exit(1);
}
console.log('UI polish 1172 assertions passed');
