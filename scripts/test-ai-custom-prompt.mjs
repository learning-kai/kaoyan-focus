/**
 * AI 排期「自定义提示词」的接线校验。
 *
 * 关注三件事：
 * 1. 字段真的存在并能长期保存（前后端类型、上限、规范化）；
 * 2. 提示词真的被注入，且位置正确、有边界标记；
 * 3. 空白提示词不污染提示词（不产生空标记）。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

const models = await readSource('src-tauri/src/commands/ai_scheduler/models.rs');
const settings = await readSource('src-tauri/src/commands/ai_scheduler/settings.rs');
const prompt = await readSource('src-tauri/src/commands/ai_scheduler/prompt.rs');
const panel = await readSource('src/pages/settings/AiSchedulerPanel.tsx');
const types = await readSource('src/types/aiScheduler.ts');

// ---------- 后端：字段与上限 ----------
assert.match(
  models,
  /pub custom_prompt: String,/,
  '偏好结构应包含 custom_prompt',
);
assert.match(
  models,
  /pub const MAX_CUSTOM_PROMPT_CHARS: usize = 8000;/,
  '应有明确的上限常量',
);
assert.match(
  models,
  /custom_prompt: String::new\(\),/,
  '默认值应为空串',
);
// 与既有的「长期记忆」并存，不能互相替代。
assert.match(models, /pub memory_note: String,/, '长期记忆字段应保留');

// ---------- 后端：规范化 ----------
assert.match(
  settings,
  /settings\.planner_preferences\.custom_prompt = settings/,
  '保存时应规范化 custom_prompt',
);
assert.match(
  settings,
  /MAX_CUSTOM_PROMPT_CHARS/,
  '规范化应使用该上限而不是硬编码数字',
);

// ---------- 后端：注入 ----------
assert.match(prompt, /let custom_prompt = preferences\.custom_prompt\.trim\(\);/, '应读取并去空白');
assert.match(prompt, /\[用户自定义排期规则\]/, '应有起始标记');
assert.match(prompt, /\[用户自定义排期规则结束\]/, '应有结束标记');
assert.match(prompt, /\{custom_prompt\}/, '应把用户规则整段插入');
assert.match(
  prompt,
  /不得违反硬性规则/,
  '应声明用户规则不能越过硬性约束',
);
// 规则必须排在 memory_note 之前，模型先读完整规则再看零散偏好。
const customAt = prompt.indexOf('let custom_prompt = preferences.custom_prompt.trim();');
const memoryAt = prompt.indexOf('长期偏好：{}');
assert.ok(customAt > 0 && memoryAt > 0 && customAt < memoryAt, '自定义规则应在长期记忆之前注入');

// ---------- 后端：测试覆盖 ----------
assert.match(prompt, /fn custom_prompt_is_injected_with_boundaries/, '应测注入与边界');
assert.match(prompt, /fn custom_prompt_is_omitted_when_blank/, '应测空白不注入');
assert.match(prompt, /fn custom_prompt_keeps_line_breaks_and_markdown/, '应测换行与 Markdown 原样保留');

// ---------- 前端：类型 ----------
assert.match(types, /custom_prompt: string;/, '前端偏好类型应包含 custom_prompt');
assert.match(
  types,
  /export const AI_CUSTOM_PROMPT_MAX_CHARS = 8000;/,
  '前端上限应与后端一致',
);

// ---------- 前端：界面 ----------
assert.match(panel, /自定义排期提示词/, '设置面板应有该输入区');
assert.match(
  panel,
  /custom_prompt: event\.target\.value/,
  '输入应写回 settings.planner_preferences.custom_prompt',
);
assert.match(
  panel,
  /maxLength=\{AI_CUSTOM_PROMPT_MAX_CHARS\}/,
  '输入框应限制长度',
);
assert.match(panel, /value=\{settings\.planner_preferences\.custom_prompt\}/, '应受控展示已存内容');
assert.match(panel, /rows=\{10\}/, '应给足可视高度，便于写长规则');
// 两个输入区并存，且说明分工。
assert.match(panel, /长期记忆/, '长期记忆入口应保留');
assert.match(panel, /长期记忆写一两句话的偏好提醒/, '应说明两者的分工');

// ---------- 前端：样式 ----------
const styles = await readSource('src/styles.css');
assert.match(styles, /\.ai-custom-prompt \{/, '应有自定义提示词样式');
assert.match(styles, /\.ai-custom-prompt-hint \{/, '应有说明文字样式');
assert.match(styles, /white-space: pre-wrap;/, '应保留用户输入的换行排版');

console.log('AI custom prompt assertions passed');
