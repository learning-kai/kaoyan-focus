/**
 * 正计时「休息理由」的取值规则 + 前后端持久化接线校验。
 *
 * 三部分：
 * 1. 用 esbuild 把 src/utils/breakReason.ts 转成 ESM 后直接跑纯函数断言；
 * 2. 前端接线：专注页弹窗、悬浮小组件、API 参数；
 * 3. 后端接线：命令签名、校验、写库字段与迁移。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { transform } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

async function loadModule(relativePath) {
  const source = await readSource(relativePath);
  const { code } = await transform(source, { format: 'esm', loader: 'ts', target: 'node20' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const { BREAK_REASON_MAX_CHARS, normalizeBreakReason } = await loadModule('src/utils/breakReason.ts');

// ---------- 取值规则 ----------
assert.equal(BREAK_REASON_MAX_CHARS, 60, '上限应与后端 BREAK_REASON_MAX_CHARS 一致（60 字）');

assert.deepEqual(normalizeBreakReason('眼睛酸了，起来走一走'), {
  ok: true,
  value: '眼睛酸了，起来走一走',
  message: null,
});
assert.equal(normalizeBreakReason('  喝水  ').value, '喝水', '应去掉首尾空白');
assert.equal(normalizeBreakReason('').ok, false, '空串不通过');
assert.equal(normalizeBreakReason('   ').ok, false, '纯空格不通过');
assert.match(normalizeBreakReason('').message, /理由/, '空值提示应引导填写理由');
assert.equal(normalizeBreakReason(null).ok, false);
assert.equal(normalizeBreakReason(undefined).ok, false);
assert.equal(normalizeBreakReason(42).ok, false, '非字符串一律按空处理');

const exactLimit = '休'.repeat(BREAK_REASON_MAX_CHARS);
assert.equal(normalizeBreakReason(exactLimit).ok, true, '刚好到达上限应通过');
const overLimit = '休'.repeat(BREAK_REASON_MAX_CHARS + 1);
assert.equal(normalizeBreakReason(overLimit).ok, false, '超过上限不通过');
assert.equal([...normalizeBreakReason(overLimit).value].length, BREAK_REASON_MAX_CHARS, '超限时截断到上限');
assert.equal(normalizeBreakReason('😀'.repeat(BREAK_REASON_MAX_CHARS)).ok, true, '按字符数而不是码元计数');

// ---------- 前端接线 ----------
const focusApi = await readSource('src/services/focusApi.ts');
assert.match(focusApi, /export function takeManualBreak\(breakSeconds: number, reason: string\)/, 'API 必须要求传理由');
assert.match(focusApi, /invokeCommand<StudyModeState>\('take_manual_break', \{\s*breakSeconds,\s*reason,/, '命令参数应包含 reason');

const focusTypes = await readSource('src/types/focus.ts');
assert.match(focusTypes, /break_reason: string \| null;/, '状态与记录类型都应带 break_reason');
assert.equal((focusTypes.match(/break_reason: string \| null;/g) ?? []).length, 2, 'StudyModeState 与 FocusSession 都要有');

const focusPage = await readSource('src/pages/FocusPage.tsx');
assert.match(focusPage, /import \{ BREAK_REASON_MAX_CHARS, normalizeBreakReason \} from '\.\.\/utils\/breakReason';/, '专注页应复用统一规则');
assert.match(focusPage, /takeManualBreak\(minutes \* 60, reason\.value\)/, '开始休息时应把理由传给后端');
assert.match(focusPage, /休息理由（必填，最多 \{BREAK_REASON_MAX_CHARS\} 字）/, '弹窗应说明理由必填');
assert.match(focusPage, /maxLength=\{BREAK_REASON_MAX_CHARS\}/, '输入框应限制字数');
assert.match(focusPage, /setBreakReasonError\(reason\.message\)/, '缺理由时应就地提示');
assert.match(focusPage, /studyState\.break_reason/, '休息阶段应读取后端持久化的理由');
assert.ok(!/setManualBreakReason/.test(focusPage), '不应再保留只存在于内存的旧状态');
assert.match(focusPage, /break_reason: null,/, 'idle 状态字面量应补齐新字段');

const widgetPage = await readSource('src/pages/FocusWidgetPage.tsx');
assert.match(widgetPage, /takeManualBreak\(minutes \* 60, reason\.value\)/, '悬浮小组件同样要传理由');
assert.match(widgetPage, /isBreakReasonPrompting/, '小组件应先就地收理由再开始休息');
assert.match(widgetPage, /aria-label="休息理由"/, '小组件理由输入应有可访问名称');
assert.match(widgetPage, /break_reason: null,/, '小组件 idle 状态字面量应补齐新字段');

const finderBar = await readSource('src/pages/FinderBarWidgetPage.tsx');
assert.match(finderBar, /break_reason: null,/, 'Finder 栏小组件 idle 状态字面量应补齐新字段');

// ---------- 后端接线 ----------
const studyMode = await readSource('src-tauri/src/commands/focus/study_mode.rs');
assert.match(studyMode, /pub fn take_manual_break\(\s*app: AppHandle,\s*state: State<'_, AppState>,\s*break_seconds: i64,\s*reason: Option<String>,\s*\)/, '命令应新增 reason 参数');
assert.match(studyMode, /fn normalize_break_reason\(/, '应有理由校验函数');
assert.match(studyMode, /const BREAK_REASON_MAX_CHARS: usize = 60;/, '后端上限应与前端一致');
assert.match(studyMode, /break_reason = \?8,/, '开始休息时应落库理由');
assert.match(studyMode, /paused_at = NULL, break_reason = NULL,/, '跳过休息后应清空本轮理由');
assert.match(studyMode, /finish_running_focus_session_with_reason\(/, '专注记录应带上理由');

const models = await readSource('src-tauri/src/commands/focus/models.rs');
assert.match(models, /pub break_reason: Option<String>,/, '对外的 StudyModeState 应暴露理由');
assert.match(models, /break_reason: Option<String>,/, '学习模式记录应保存休息理由');

const dbRust = await readSource('src-tauri/src/storage/db.rs');
assert.match(dbRust, /phase TEXT NOT NULL,\s*break_reason TEXT,/, 'study_modes 建表应包含理由字段');
assert.match(dbRust, /add_column_if_missing\(connection, "study_modes", "break_reason", "TEXT"\)\?;/, '老库应补 study_modes.break_reason');
assert.match(dbRust, /add_column_if_missing\(connection, "focus_sessions", "break_reason", "TEXT"\)\?;/, '老库应补 focus_sessions.break_reason');

const stateRust = await readSource('src-tauri/src/commands/focus/state.rs');
assert.match(stateRust, /break_reason = NULL,/, '休息结束回到专注时应清空本轮理由');
assert.match(stateRust, /break_reason: record\.break_reason\.clone\(\),/, '状态映射应带出理由');
assert.equal((stateRust.match(/timer_kind, break_reason/g) ?? []).length, 3, '三处 study_modes 查询都要带上新列');

const helpersRust = await readSource('src-tauri/src/commands/focus/helpers.rs');
assert.equal((helpersRust.match(/break_reason: row\.get\(\d+\)\?/g) ?? []).length, 3, '学习模式与两条 focus_sessions 行映射都要读理由');

console.log('Countup break reason assertions passed');
