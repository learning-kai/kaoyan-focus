/**
 * AI 复盘起草（日 / 周）的接线与解析规则校验。
 *
 * 三部分：
 * 1. Rust 源码级断言：命令注册、就绪检查、提示词字段、兜底逻辑、数据库口径；
 * 2. 前端接线断言：按钮、API、类型、样式；
 * 3. 不变式断言：草稿不落库、不新增表、复用 AI 排期配置。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

const reviewAi = await readSource('src-tauri/src/commands/review_ai.rs');
const libRs = await readSource('src-tauri/src/lib.rs');
const clientRs = await readSource('src-tauri/src/commands/ai_scheduler/client.rs');

// ---------- 后端：命令与模块 ----------
assert.match(libRs, /pub mod review_ai;/, '模块应在 lib.rs 注册');
assert.match(
  libRs,
  /commands::review_ai::draft_review_with_ai,/,
  '命令应注册进 invoke_handler',
);
assert.match(
  reviewAi,
  /pub fn draft_review_with_ai\(\s*app: AppHandle,\s*scope: String,\s*review_date: Option<String>,/,
  '命令签名应接收 scope 与可选日期',
);
assert.match(reviewAi, /#\[tauri::command\]/, '必须是 tauri command');

// ---------- 后端：日 / 周两条路径都要有 ----------
assert.match(reviewAi, /ReviewScope::Daily => load_daily_facts/, '日复盘应有独立的数据装载');
assert.match(reviewAi, /ReviewScope::Weekly => load_weekly_facts/, '周复盘应有独立的数据装载');
assert.match(reviewAi, /fn week_start_monday\(/, '周复盘应把任意日期归一到周一');
assert.match(reviewAi, /ReviewScope::Daily => \("今日总结", "明日重点"\)/, '日复盘的字段名');
assert.match(reviewAi, /ReviewScope::Weekly => \("本周总结", "下周重点"\)/, '周复盘的字段名');

// ---------- 后端：复用 AI 排期配置，不新增密钥 ----------
assert.match(
  reviewAi,
  /ai_settings::current_settings\(&connection\)/,
  '应复用 AI 排期的配置读取',
);
assert.match(
  reviewAi,
  /ai_settings::load_api_key\(&connection\)/,
  '应复用 AI 排期的密钥托管',
);
assert.match(reviewAi, /client::chat_text\(/, '应走新增的文本补全通道');
assert.match(
  clientRs,
  /pub fn chat_text\(/,
  'client 应提供 chat_text',
);
assert.match(
  clientRs,
  /pub fn chat_text[\s\S]*?classify_status\(status, &raw, retry_after\)/,
  'chat_text 应复用状态码分类',
);
assert.doesNotMatch(
  reviewAi,
  /api_key:\s*String/,
  '复盘模块不应自己持有密钥字段',
);

// ---------- 后端：就绪检查覆盖四种失败 ----------
assert.match(reviewAi, /AI 复盘尚未开启/, '未开启应有提示');
assert.match(reviewAi, /privacy_acknowledged/, '应检查隐私确认');
assert.match(reviewAi, /ERR_MISSING_API_KEY/, '缺密钥应给出专用错误码');
assert.match(reviewAi, /接口地址与模型名/, '缺地址或模型应有提示');

// ---------- 后端：解析容错与兜底 ----------
assert.match(reviewAi, /fn parse_sections\(/, '应有解析函数');
assert.match(reviewAi, /replace\("\*\*", ""\)/, '应剔除 markdown 加粗');
assert.match(reviewAi, /fn apply_fallbacks\(/, '应有本地兜底');
assert.match(reviewAi, /fn local_summary\(/, '总结应有本地兜底');
assert.match(reviewAi, /fn local_blockers\(/, '卡点应有本地兜底');
assert.match(reviewAi, /fn local_focus\(/, '重点应有本地兜底');
assert.match(reviewAi, /MAX_SECTION_CHARS/, '应有单段长度上限');
assert.match(reviewAi, /break_reasons/, '应把休息理由纳入素材');

// ---------- 后端：不落库、不建表 ----------
assert.doesNotMatch(reviewAi, /INSERT INTO|UPDATE .* SET|DELETE FROM/, '草稿不应写库');
assert.doesNotMatch(reviewAi, /CREATE TABLE|ALTER TABLE/, '不应新增或修改表结构');
// 只检查代码本体：模块头注释里提到这两个命令名是为了说明设计取舍。
const reviewAiCode = reviewAi.slice(reviewAi.indexOf('use super::ai_scheduler'));
assert.doesNotMatch(
  reviewAiCode,
  /save_daily_review|save_weekly_review/,
  '草稿应由前端走既有保存命令，后端不越权落库',
);

// ---------- 后端：测试覆盖 ----------
assert.match(reviewAi, /fn parse_sections_accepts_full_width_colon_and_bold/, '应测全角冒号与加粗');
assert.match(reviewAi, /fn parse_sections_accepts_ascii_colon_and_bullets/, '应测半角冒号与列表');
assert.match(reviewAi, /fn apply_fallbacks_fills_missing_sections_from_local_facts/, '应测兜底');
assert.match(reviewAi, /fn week_start_monday_normalizes_any_day_in_week/, '应测周归一');
assert.match(reviewAi, /fn scope_parse_rejects_unknown_values/, '应测非法 scope');

// ---------- 前端：类型与 API ----------
const reviewTypes = await readSource('src/types/review.ts');
assert.match(reviewTypes, /export type AiReviewDraft = \{/, '应定义 AiReviewDraft');
assert.match(reviewTypes, /context_notes: string\[\];/, '应回传用到的数据摘要');
assert.match(reviewTypes, /export type ReviewAiScope = 'daily' \| 'weekly';/, '应定义 scope 联合类型');

const reviewApi = await readSource('src/services/reviewApi.ts');
assert.match(
  reviewApi,
  /export function draftReviewWithAi\(scope: ReviewAiScope, reviewDate\?: string \| null\)/,
  '应暴露 draftReviewWithAi',
);
assert.match(reviewApi, /invokeCommand<AiReviewDraft>\('draft_review_with_ai'/, '命令名应与后端一致');
assert.match(reviewApi, /parseAiSchedulerError/, '错误应解码成既有信封结构');

// ---------- 前端：页面接线 ----------
const reviewPage = await readSource('src/pages/ReviewPage.tsx');
assert.match(reviewPage, /draftReviewWithAi/, '页面应调用起草 API');
assert.match(reviewPage, /async function handleDraftWithAi\(/, '应有起草处理函数');
assert.match(reviewPage, /让 AI 起草/, '应有可见入口');
assert.match(reviewPage, /正在起草/, '应有进行中状态');
assert.match(reviewPage, /用 AI 草稿覆盖/, '已有内容时应先确认');
assert.match(reviewPage, /tomorrowFocus: result\.focus/, '日复盘应写入明日重点');
assert.match(reviewPage, /nextWeekFocus: result\.focus/, '周复盘应写入下周重点');
assert.match(reviewPage, /setAiNote\(/, '应回显用了哪个模型和哪些数据');
assert.match(reviewPage, /draftingAi/, '应有防重复点击状态');
// AI 草稿只能填输入框，不能替用户保存。
assert.doesNotMatch(
  reviewPage.slice(
    reviewPage.indexOf('async function handleDraftWithAi'),
    reviewPage.indexOf('async function handleSave'),
  ),
  /saveDailyReview|saveWeeklyReview|await handleSave/,
  '起草不应自动保存',
);

// ---------- 前端：样式 ----------
const styles = await readSource('src/styles.css');
assert.match(styles, /\.review-ai-note \{/, '应有 AI 说明样式');
assert.match(styles, /\.review-ai-action \{/, '应有 AI 按钮样式');
assert.match(styles, /\.review-ai-action:disabled \{/, '应有禁用态');

console.log('AI review draft assertions passed');
