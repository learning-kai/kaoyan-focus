/**
 * 番茄专注时长的取值规则 + 偏好保存链路校验。
 *
 * 两部分：
 * 1. 用 esbuild 把 src/utils/focusDuration.ts 转成 ESM 后直接跑纯函数断言；
 * 2. 对 FocusPage / 设置前后端做源码级接线断言，防止改动时漏掉某一环。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { transform } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

async function loadFocusDurationModule() {
  const source = await readSource('src/utils/focusDuration.ts');
  const { code } = await transform(source, { format: 'esm', loader: 'ts', target: 'node20' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const {
  FOCUS_MINUTES_FALLBACK,
  FOCUS_MINUTES_MAX,
  FOCUS_MINUTES_MIN,
  FOCUS_PRESET_MINUTES,
  clampFocusMinutes,
  formatFocusCountdown,
  formatFocusDurationLabel,
  isFocusPresetMinutes,
  parseFocusMinutes,
  validateFocusMinutes,
} = await loadFocusDurationModule();

// ---------- 有效范围 ----------
assert.equal(FOCUS_MINUTES_MIN, 1, '下限应为 1 分钟');
assert.equal(FOCUS_MINUTES_MAX, 120, '上限应与设置页 default_focus_minutes 一致（120 分钟）');
assert.ok(FOCUS_PRESET_MINUTES.includes(25) && FOCUS_PRESET_MINUTES.includes(45), '预设应包含 25 和 45 分钟');
assert.ok(
  FOCUS_PRESET_MINUTES.every((minutes) => minutes >= FOCUS_MINUTES_MIN && minutes <= FOCUS_MINUTES_MAX),
  '所有预设都必须落在有效区间内',
);

// ---------- clampFocusMinutes ----------
assert.equal(clampFocusMinutes(25), 25);
assert.equal(clampFocusMinutes(0), FOCUS_MINUTES_MIN, '小于下限时收敛到下限');
assert.equal(clampFocusMinutes(121), FOCUS_MINUTES_MAX, '大于上限时收敛到上限');
assert.equal(clampFocusMinutes(45.9), 45, '小数向下取整');
assert.equal(clampFocusMinutes(Number.NaN), FOCUS_MINUTES_FALLBACK, 'NaN 回落到默认时长');
assert.equal(clampFocusMinutes(Number.POSITIVE_INFINITY), FOCUS_MINUTES_FALLBACK, 'Infinity 回落到默认时长');
assert.ok(Number.isInteger(clampFocusMinutes(33.2)), '钳制后一定是整数');

// ---------- parseFocusMinutes ----------
assert.equal(parseFocusMinutes('25'), 25);
assert.equal(parseFocusMinutes(' 45 '), 45, '允许首尾空格');
assert.equal(parseFocusMinutes(60), 60, '数字直接接受');
assert.equal(parseFocusMinutes('007'), 7, '前导零按 7 分钟处理');
assert.equal(parseFocusMinutes(''), null, '空串不接受');
assert.equal(parseFocusMinutes('   '), null, '纯空格不接受');
assert.equal(parseFocusMinutes('abc'), null, '非数字不接受');
assert.equal(parseFocusMinutes('2.5'), null, '小数不接受');
assert.equal(parseFocusMinutes('-5'), null, '负数不接受');
assert.equal(parseFocusMinutes('0'), null, '0 分钟不接受');
assert.equal(parseFocusMinutes('121'), null, '超过上限不接受');
assert.equal(parseFocusMinutes(null), null);
assert.equal(parseFocusMinutes(undefined), null);

// ---------- validateFocusMinutes ----------
assert.deepEqual(validateFocusMinutes('25'), { ok: true, value: 25, message: null });
assert.deepEqual(validateFocusMinutes(45), { ok: true, value: 45, message: null });
assert.equal(validateFocusMinutes('').ok, false, '空值不通过');
assert.match(validateFocusMinutes('').message, /填写/, '空值提示应引导用户填写');
assert.equal(validateFocusMinutes('abc').ok, false, '非数字不通过');
assert.equal(validateFocusMinutes('200').ok, false, '越界不通过');
assert.equal(validateFocusMinutes('200').value, FOCUS_MINUTES_MAX, '越界时给出可落的合法值');
assert.equal(validateFocusMinutes('0').value, FOCUS_MINUTES_MIN, '低于下限时给出下限');
assert.equal(validateFocusMinutes(null).ok, false);

// ---------- 预设判断与文案 ----------
assert.equal(isFocusPresetMinutes(25), true);
assert.equal(isFocusPresetMinutes(37), false);
assert.equal(formatFocusDurationLabel(45), '45 分钟');
assert.equal(formatFocusDurationLabel(60), '1 小时');
assert.equal(formatFocusDurationLabel(90), '1 小时 30 分钟');
assert.equal(formatFocusDurationLabel(0), '1 分钟', '非法值先钳制再格式化');
assert.equal(formatFocusCountdown(25), '25:00');
assert.equal(formatFocusCountdown(60), '1:00:00');
assert.equal(formatFocusCountdown(90), '1:30:00');

// ---------- 接线：专注页 ----------
const focusPage = await readSource('src/pages/FocusPage.tsx');
assert.match(focusPage, /import FocusDurationPicker from '\.\.\/components\/focus\/FocusDurationPicker';/, '专注页应引入时长选择器');
assert.match(focusPage, /<FocusDurationPicker/, '专注页应渲染时长选择器');
assert.ok(!/<PresetSelect label="番茄时长"/.test(focusPage), '旧的番茄时长下拉应已被选择器取代');
assert.match(focusPage, /function handleFocusMinutesChange\(/, '应有时长变更处理函数');
assert.match(focusPage, /function handleRememberFocusDurationChange\(/, '应有“记住默认”开关处理函数');
assert.match(focusPage, /function resolveFocusMinutes\(\)/, '应提供统一的时长守卫');
assert.match(focusPage, /function persistFocusPreference\(/, '应能把偏好写回设置');
for (const handler of ['handleStart', 'handleStartScheduleBlock', 'handleQuickScheduleNextTask']) {
  const section = focusPage.slice(focusPage.indexOf(`function ${handler}(`), focusPage.indexOf(`function ${handler}(`) + 1200);
  assert.match(section, /resolveFocusMinutes\(\)/, `${handler} 开始前应先校验番茄时长`);
  assert.match(section, /effectiveFocusMinutes/, `${handler} 应使用校验后的时长`);
}
assert.match(focusPage, /setFocusMinutes\(clampFocusMinutes\(settings\.default_focus_minutes\)\)/, '读取设置时应把历史值收敛到合法区间');
assert.match(focusPage, /setRememberFocusDuration\(settings\.remember_focus_duration\)/, '初始化时应读取“记住默认”偏好');

// ---------- 接线：设置项 ----------
const settingsTypes = await readSource('src/types/settings.ts');
assert.match(settingsTypes, /remember_focus_duration: boolean;/, '设置类型应包含 remember_focus_duration');

const settingsPage = await readSource('src/pages/SettingsPage.tsx');
assert.match(settingsPage, /remember_focus_duration: true,/, '设置页默认值应为记住');

const basicPanel = await readSource('src/pages/settings/BasicSettingsPanel.tsx');
assert.match(basicPanel, /记住番茄专注时长/, '学习节奏面板应提供“记住番茄专注时长”开关');
assert.match(basicPanel, /updateSettings\(\{ remember_focus_duration: event\.target\.checked \}\)/, '开关应写回设置');

const settingsRust = await readSource('src-tauri/src/commands/settings.rs');
assert.match(settingsRust, /const REMEMBER_FOCUS_DURATION_KEY: &str = "remember_focus_duration";/, 'Rust 应定义设置键');
assert.match(settingsRust, /pub remember_focus_duration: bool,/, 'Rust 结构体应包含该字段');
assert.match(settingsRust, /remember_focus_duration: true,/, 'Rust 默认值应为记住');
assert.match(settingsRust, /REMEMBER_FOCUS_DURATION_KEY,\n\s+defaults\.remember_focus_duration,/, '读取设置应带回默认值');
assert.match(settingsRust, /if normalized\.remember_focus_duration \{/, '保存设置应落库');

// ---------- 接线：样式 ----------
const styles = await readSource('src/styles.css');
assert.match(styles, /\.focus-duration-picker \{/, '样式应包含选择器容器');
assert.match(styles, /\.focus-duration-chip\.active \{/, '样式应包含选中态');
assert.match(styles, /\.focus-duration-error \{/, '样式应包含错误提示');

console.log('Focus duration preference assertions passed');
