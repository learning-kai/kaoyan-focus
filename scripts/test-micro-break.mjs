/**
 * 随机微休息：前端规则 + 前后端一致性 + 接线校验。
 *
 * 调度算法本身（随机时刻表、专注算法分段、触发判定）的测试在 Rust：
 *   cargo test --lib micro_break
 * 这里校验前端规则与后端的取值范围一致，以及各环节确实接上了。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { transform } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

async function loadModule() {
  const source = await readSource('src/utils/microBreak.ts');
  const { code } = await transform(source, { format: 'esm', loader: 'ts', target: 'node20' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const {
  DEFAULT_MICRO_BREAK_SETTINGS,
  MICRO_BREAK_CUE_EVENT,
  MICRO_BREAK_INTERVAL_PRESETS,
  MICRO_BREAK_MAX_INTERVAL_SECONDS,
  MICRO_BREAK_METHOD_PRESET,
  MICRO_BREAK_MIN_INTERVAL_SECONDS,
  MICRO_BREAK_REST_MAX_SECONDS,
  MICRO_BREAK_REST_MIN_SECONDS,
  MICRO_BREAK_REST_PRESETS,
  describeMicroBreak,
  findIntervalPreset,
  microBreakSecondsLeft,
  normalizeMicroBreakSettings,
} = await loadModule();

// ---------- 默认值：对应方法里的「3 到 5 分钟随机提示，闭眼 10 秒」 ----------
assert.equal(DEFAULT_MICRO_BREAK_SETTINGS.enabled, false, '默认关闭，由用户主动开启');
assert.equal(DEFAULT_MICRO_BREAK_SETTINGS.min_interval_seconds, 180);
assert.equal(DEFAULT_MICRO_BREAK_SETTINGS.max_interval_seconds, 300);
assert.equal(DEFAULT_MICRO_BREAK_SETTINGS.rest_seconds, 10);
assert.equal(DEFAULT_MICRO_BREAK_SETTINGS.adaptive, true);
assert.equal(findIntervalPreset(DEFAULT_MICRO_BREAK_SETTINGS)?.id, '3-5', '默认值应命中 3–5 分钟预设');
assert.ok(MICRO_BREAK_REST_PRESETS.includes(10));

// ---------- 90/20 预设 ----------
assert.equal(MICRO_BREAK_METHOD_PRESET.focus_minutes, 90);
assert.equal(MICRO_BREAK_METHOD_PRESET.break_minutes, 20);
assert.equal(MICRO_BREAK_METHOD_PRESET.long_break_minutes, 20, '长休与短休一致，每轮之后都是 20 分钟');
assert.ok(MICRO_BREAK_METHOD_PRESET.focus_minutes <= 120, '必须落在番茄时长上限 120 分钟内');
assert.ok(MICRO_BREAK_METHOD_PRESET.break_minutes <= 60, '必须落在短休上限 60 分钟内');
assert.equal(MICRO_BREAK_METHOD_PRESET.study_minutes % MICRO_BREAK_METHOD_PRESET.focus_minutes, 0, '学习模式总时长应是整数轮');

// ---------- normalize ----------
assert.deepEqual(normalizeMicroBreakSettings(null), DEFAULT_MICRO_BREAK_SETTINGS);
assert.deepEqual(normalizeMicroBreakSettings({}), DEFAULT_MICRO_BREAK_SETTINGS);
const swapped = normalizeMicroBreakSettings({ min_interval_seconds: 600, max_interval_seconds: 120 });
assert.equal(swapped.min_interval_seconds, 120, '区间颠倒时交换');
assert.equal(swapped.max_interval_seconds, 600);
const clamped = normalizeMicroBreakSettings({ min_interval_seconds: 1, max_interval_seconds: 99999, rest_seconds: 999, volume: -3, enabled: 'yes' });
assert.equal(clamped.min_interval_seconds, MICRO_BREAK_MIN_INTERVAL_SECONDS);
assert.equal(clamped.max_interval_seconds, MICRO_BREAK_MAX_INTERVAL_SECONDS);
assert.equal(clamped.rest_seconds, MICRO_BREAK_REST_MAX_SECONDS);
assert.equal(clamped.volume, 0);
assert.equal(clamped.enabled, false, '非布尔 enabled 视为关闭');
assert.equal(normalizeMicroBreakSettings({ rest_seconds: Number.NaN }).rest_seconds, 10, 'NaN 回落默认值');
assert.equal(normalizeMicroBreakSettings({ adaptive: false }).adaptive, false);

for (const preset of MICRO_BREAK_INTERVAL_PRESETS) {
  assert.ok(preset.min_interval_seconds < preset.max_interval_seconds, `${preset.id} 必须是随机区间`);
  assert.deepEqual(
    normalizeMicroBreakSettings({ ...DEFAULT_MICRO_BREAK_SETTINGS, ...preset }).min_interval_seconds,
    preset.min_interval_seconds,
    `${preset.id} 必须落在合法范围`,
  );
}
for (const seconds of MICRO_BREAK_REST_PRESETS) {
  assert.ok(seconds >= MICRO_BREAK_REST_MIN_SECONDS && seconds <= MICRO_BREAK_REST_MAX_SECONDS);
}

// ---------- 文案与倒计时 ----------
assert.equal(describeMicroBreak(DEFAULT_MICRO_BREAK_SETTINGS), '未开启');
assert.equal(describeMicroBreak({ ...DEFAULT_MICRO_BREAK_SETTINGS, enabled: true }), '每 3–5 分钟随机提示，闭眼 10 秒');
assert.equal(
  describeMicroBreak({ ...DEFAULT_MICRO_BREAK_SETTINGS, enabled: true, min_interval_seconds: 240, max_interval_seconds: 240 }),
  '每 4 分钟提示，闭眼 10 秒',
);
assert.equal(microBreakSecondsLeft(10_000, 0), 10);
assert.equal(microBreakSecondsLeft(10_000, 9_100), 1, '最后不足 1 秒显示 1');
assert.equal(microBreakSecondsLeft(10_000, 12_000), 0, '不会出现负数');

// ---------- 前后端一致 ----------
const rust = await readSource('src-tauri/src/commands/micro_break.rs');
const rustConst = (name) => {
  const match = rust.match(new RegExp(`const ${name}: i64 = ([^;]+);`));
  assert.ok(match, `Rust 缺少常量 ${name}`);
  return Function(`return (${match[1].replace(/_/g, '')})`)();
};
assert.equal(rustConst('MIN_INTERVAL_FLOOR_SECONDS'), MICRO_BREAK_MIN_INTERVAL_SECONDS, '间隔下限前后端一致');
assert.equal(rustConst('MAX_INTERVAL_CEIL_SECONDS'), MICRO_BREAK_MAX_INTERVAL_SECONDS, '间隔上限前后端一致');
assert.equal(rustConst('REST_SECONDS_MIN'), MICRO_BREAK_REST_MIN_SECONDS, '闭眼下限前后端一致');
assert.equal(rustConst('REST_SECONDS_MAX'), MICRO_BREAK_REST_MAX_SECONDS, '闭眼上限前后端一致');
assert.ok(rust.includes(`"${MICRO_BREAK_CUE_EVENT}"`), '事件名前后端一致');
assert.ok(/min_interval_seconds: 3 \* 60/.test(rust) && /max_interval_seconds: 5 \* 60/.test(rust) && /rest_seconds: 10/.test(rust), 'Rust 默认值应与前端一致');

// ---------- 接线 ----------
const lib = await readSource('src-tauri/src/lib.rs');
assert.ok(lib.includes('pub mod micro_break;'), 'lib.rs 必须声明 micro_break 模块');
assert.ok(lib.includes('commands::micro_break::get_micro_break_settings'), '必须注册读取命令');
assert.ok(lib.includes('commands::micro_break::save_micro_break_settings'), '必须注册保存命令');

const tick = await readSource('src-tauri/src/commands/focus/study_mode.rs');
assert.ok(tick.includes('micro_break::tick_micro_break(app, &study_state)'), '后台 tick 必须调用微休息判定');

const app = await readSource('src/App.tsx');
assert.ok(app.includes('useMicroBreakListener()'), 'App 必须在顶层监听微休息事件');
assert.ok(app.includes('<MicroBreakOverlay />'), 'App 必须挂载闭眼提示');

const focusPage = await readSource('src/pages/FocusPage.tsx');
assert.ok(focusPage.includes('<MicroBreakPanel'), '专注页必须提供微休息设置');

const coordinator = await readSource('src/services/microBreakCoordinator.ts');
assert.ok(!coordinator.includes('setInterval'), '提示时刻由后端判定，前端协调器不能自己轮询计时');

const widget = await readSource('src/pages/FocusWidgetPage.tsx');
assert.ok(!widget.includes('microBreak'), '专注悬浮窗不监听微休息，避免双窗口重复放提示音');

console.log('micro break tests passed');
