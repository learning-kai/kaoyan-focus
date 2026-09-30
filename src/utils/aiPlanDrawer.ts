/**
 * AI 排期抽屉的纯前端规则：快捷补充说明、抽屉偏好、草案摘要、日期标题、分段标签、
 * 加载提示与错误分流。
 *
 * 只 import 类型，不依赖 React / Tauri，`scripts/test-ai-plan-drawer.mjs` 可以直接测。
 * 排期算法本身（空档、可行性修复、校验）的测试在 Rust：`cargo test ai_scheduler`。
 */

import type {
  AiApplyResult,
  AiPlanItem,
  AiPlanProposal,
  AiPlanWarning,
  AiSchedulerError,
} from '../types/aiScheduler';

/** 与后端 `MAX_EXTRA_INSTRUCTION_CHARS` 一致。 */
export const MAX_EXTRA_INSTRUCTION_CHARS = 200;
/** 与后端 `MAX_FEEDBACK_CHARS` 一致。 */
export const MAX_FEEDBACK_CHARS = 200;

export const HORIZON_OPTIONS: readonly number[] = [1, 2, 3, 5, 7];

/** 生成前的快捷补充说明：点一下追加，再点一下去掉。 */
export const QUICK_PROMPTS: readonly string[] = [
  '上午先做数学',
  '晚上背单词',
  '难的放高效时段',
  '多留休息时间',
  '今天少排一点',
];

/** 草案出来后的快捷反馈。 */
export const QUICK_FEEDBACK: readonly string[] = [
  '整体往后挪一点',
  '数学挪到下午',
  '今天少排一点',
  '没排上的尽量排进去',
];

const PROMPT_SEPARATOR = '；';

function splitPrompts(text: string): string[] {
  return text
    .split(/[；;\n]/)
    .map((part) => part.trim())
    .filter(Boolean);
}

/** 快捷说明是否已在补充说明里。 */
export function hasQuickPrompt(text: string, prompt: string): boolean {
  return splitPrompts(text).includes(prompt);
}

/**
 * 切换快捷说明：已有就去掉，没有就追加。追加后会超出长度上限时原样返回——
 * 宁可不加，也不去截断用户自己写的内容。
 */
export function toggleQuickPrompt(
  text: string,
  prompt: string,
  maxChars = MAX_EXTRA_INSTRUCTION_CHARS,
): string {
  const parts = splitPrompts(text);
  if (parts.includes(prompt)) {
    return parts.filter((part) => part !== prompt).join(PROMPT_SEPARATOR);
  }
  const next = [...parts, prompt].join(PROMPT_SEPARATOR);
  return next.length > maxChars ? text : next;
}

/** 追加后会超长的快捷说明置灰；已选中的始终可以点掉。 */
export function isQuickPromptBlocked(
  text: string,
  prompt: string,
  maxChars = MAX_EXTRA_INSTRUCTION_CHARS,
): boolean {
  return !hasQuickPrompt(text, prompt) && toggleQuickPrompt(text, prompt, maxChars) === text;
}

export type AiPlanDrawerPrefs = {
  horizonDays: number;
  respectPriority: boolean;
};

export const DEFAULT_AI_PLAN_DRAWER_PREFS: AiPlanDrawerPrefs = {
  horizonDays: 1,
  respectPriority: true,
};

export const AI_PLAN_DRAWER_PREFS_STORAGE_KEY = 'kaoyan-focus-ai-plan-drawer';

/**
 * 解析 localStorage 里的抽屉偏好。只记「规划范围」和「按优先级」：「重新安排 AI 日程」会删旧块，
 * 每次都要重新确认；补充说明是一次性的（长期偏好在设置里）。两者都不持久化。
 */
export function parseAiPlanDrawerPrefs(raw: string | null): AiPlanDrawerPrefs {
  const fallback = { ...DEFAULT_AI_PLAN_DRAWER_PREFS };
  if (!raw) {
    return fallback;
  }
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fallback;
  }
  if (!value || typeof value !== 'object') {
    return fallback;
  }
  const candidate = value as Record<string, unknown>;
  const horizon = Number(candidate.horizonDays);
  return {
    horizonDays: HORIZON_OPTIONS.includes(horizon) ? horizon : fallback.horizonDays,
    respectPriority:
      typeof candidate.respectPriority === 'boolean'
        ? candidate.respectPriority
        : fallback.respectPriority,
  };
}

export function serializeAiPlanDrawerPrefs(prefs: AiPlanDrawerPrefs): string {
  return JSON.stringify(parseAiPlanDrawerPrefs(JSON.stringify(prefs)));
}

export function formatMinute(minute: number): string {
  const safe = Math.max(0, Math.min(24 * 60, minute));
  const hours = Math.floor(safe / 60);
  const minutes = safe % 60;
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

export function formatDuration(minutes: number): string {
  if (minutes <= 0) {
    return '0 分钟';
  }
  if (minutes < 60) {
    return `${minutes} 分钟`;
  }
  const hours = minutes / 60;
  return Number.isInteger(hours) ? `${hours} 小时` : `${hours.toFixed(1)} 小时`;
}

export type ProposalSummary = {
  /** 模型的一句话总结；没有时用条数与时长拼一句 */
  headline: string;
  studyMinutes: number;
  targetMinutes: number;
  /** 0..1；没有每日目标时为 null */
  progress: number | null;
  progressLabel: string;
  scheduledCount: number;
  unscheduledCount: number;
  adjustedCount: number;
  replaceableCount: number;
  alreadyScheduledCount: number;
};

/** 抽屉顶部的摘要卡片。 */
export function summarizeProposal(proposal: AiPlanProposal): ProposalSummary {
  const stats = proposal.stats;
  const studyMinutes =
    stats.study_minutes ??
    proposal.items
      .filter((item) => item.kind !== 'meal')
      .reduce((total, item) => total + (item.end_minute - item.start_minute), 0);
  const targetMinutes = Math.max(0, stats.target_minutes ?? 0);
  const scheduledCount = stats.scheduled_count;
  const unscheduledCount = proposal.unscheduled.length;

  let fallback = `排了 ${scheduledCount} 条，共 ${formatDuration(studyMinutes)}`;
  if (unscheduledCount > 0) {
    fallback += `；${unscheduledCount} 条没排上`;
  }
  const progress = targetMinutes > 0 ? Math.min(1, studyMinutes / targetMinutes) : null;

  return {
    headline: proposal.summary?.trim() || fallback,
    studyMinutes,
    targetMinutes,
    progress,
    progressLabel:
      targetMinutes > 0
        ? `学习 ${formatDuration(studyMinutes)} / 目标 ${formatDuration(targetMinutes)}`
        : `学习 ${formatDuration(studyMinutes)}`,
    scheduledCount,
    unscheduledCount,
    adjustedCount: stats.adjusted_count ?? 0,
    replaceableCount: proposal.replaceable_block_count ?? 0,
    alreadyScheduledCount: proposal.already_scheduled?.length ?? 0,
  };
}

const WEEKDAY_LABELS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

/** `YYYY-MM-DD` → UTC 零点。只用来算星期与加减天数，避开时区。非法日期返回 null。 */
function parseDateKey(dateKey: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateKey);
  if (!match) {
    return null;
  }
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return date.toISOString().slice(0, 10) === dateKey ? date : null;
}

export function shiftDateKey(dateKey: string, days: number): string {
  const date = parseDateKey(dateKey);
  if (!date) {
    return dateKey;
  }
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

/** 本机今天。与后端 `context::local_clock` 同一口径：本地时区，不是 UTC。 */
export function localDateKey(now: Date = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  return `${now.getFullYear()}-${month}-${day}`;
}

/** 「09-30 周三 · 今天」。 */
export function dateHeading(dateKey: string, today?: string): string {
  const date = parseDateKey(dateKey);
  if (!date) {
    return dateKey;
  }
  const label = `${dateKey.slice(5)} ${WEEKDAY_LABELS[date.getUTCDay()]}`;
  if (!today) {
    return label;
  }
  if (dateKey === today) {
    return `${label} · 今天`;
  }
  return dateKey === shiftDateKey(today, 1) ? `${label} · 明天` : label;
}

/** 拆段条目的「第 1/2 段」标签，key 为 `AiPlanItem.id`。按日期、开始时间编号；三餐不参与。 */
export function segmentLabels(items: AiPlanItem[]): Map<string, string> {
  const groups = new Map<number, AiPlanItem[]>();
  for (const item of items) {
    if (item.kind === 'meal' || item.source_today_item_id == null) {
      continue;
    }
    const bucket = groups.get(item.source_today_item_id);
    if (bucket) {
      bucket.push(item);
    } else {
      groups.set(item.source_today_item_id, [item]);
    }
  }
  const labels = new Map<string, string>();
  for (const bucket of groups.values()) {
    if (bucket.length < 2) {
      continue;
    }
    const ordered = [...bucket].sort((left, right) =>
      left.schedule_date === right.schedule_date
        ? left.start_minute - right.start_minute
        : left.schedule_date.localeCompare(right.schedule_date),
    );
    ordered.forEach((item, index) => {
      labels.set(item.id, `第 ${index + 1}/${ordered.length} 段`);
    });
  }
  return labels;
}

/**
 * 删掉一条后剩下的草案条目。拆段条目删任意一段就整条删掉：用户的意思是「今天不做这件事」，
 * 而不是「少做一段」。某天只剩三餐时由后端一并去掉。
 */
export function removeItemFromDraft(items: AiPlanItem[], target: AiPlanItem): AiPlanItem[] {
  const queueId = target.kind === 'meal' ? null : target.source_today_item_id;
  return items.filter((item) => {
    if (item.id === target.id) {
      return false;
    }
    return queueId == null || item.kind === 'meal' || item.source_today_item_id !== queueId;
  });
}

/** 被本地可行性修复挪动 / 补排过的条目 id。 */
export function adjustedItemIds(warnings: AiPlanWarning[]): Set<string> {
  const ids = new Set<string>();
  for (const warning of warnings) {
    if (warning.code === 'adjusted' && warning.item_id) {
      ids.add(warning.item_id);
    }
  }
  return ids;
}

/**
 * 时间轴上方统一展示的提示：没挂到草案条目上的（超上限、模型编造 id 等），
 * 以及所挂条目已不在草案里的。同一句话只出现一次。
 */
export function generalWarnings(warnings: AiPlanWarning[], items: AiPlanItem[]): AiPlanWarning[] {
  const itemIds = new Set(items.map((item) => item.id));
  const seen = new Set<string>();
  return warnings.filter((warning) => {
    if (warning.item_id && itemIds.has(warning.item_id)) {
      return false;
    }
    if (seen.has(warning.message)) {
      return false;
    }
    seen.add(warning.message);
    return true;
  });
}

/**
 * 等待模型时的提示。`title` 只在阶段变化时改变（放进 aria-live，避免每秒播报一次），
 * `detail` 带秒数，只做视觉展示。
 */
export function loadingHint(
  elapsedSeconds: number,
  revising: boolean,
): { title: string; detail: string } {
  const waited = `已等待 ${Math.max(0, Math.floor(elapsedSeconds))} 秒`;
  if (elapsedSeconds < 10) {
    return {
      title: revising ? 'AI 正在按反馈调整…' : 'AI 正在排期…',
      detail: `${waited}，通常 10–30 秒`,
    };
  }
  if (elapsedSeconds < 30) {
    return { title: '模型还在思考，稍等一下', detail: `${waited}，条目越多越久` };
  }
  return {
    title: '这次比平时慢',
    detail: `${waited}；超过设置里的超时时间会自动报错，届时可以重试或改用本地排期`,
  };
}

/**
 * 本地排期能救的错误：模型调用本身失败了。缺 Key、日期已过、没东西可排这类问题，
 * 本地排也一样过不去，不给这个按钮。
 */
const LOCAL_FALLBACK_CODES = new Set<string>([
  'network',
  'timeout',
  'rate_limited',
  'quota_exceeded',
  'server_error',
  'invalid_response',
  'unauthorized',
  'forbidden',
]);

export function canFallbackToLocal(error: AiSchedulerError | null): boolean {
  return error !== null && LOCAL_FALLBACK_CODES.has(error.code);
}

export function engineLabel(engine: string, degraded: boolean, model: string): string {
  if (engine === 'llm') {
    return model ? `AI 排期 · ${model}` : 'AI 排期';
  }
  return degraded ? '本地兜底排期（AI 不可用时的降级结果）' : '本地排期（未使用 AI）';
}

/** 写入结果提示的色调：全部写入且没有提示时是 info，否则 warn。 */
export function applyResultTone(result: AiApplyResult): 'info' | 'warn' {
  return result.status === 'applied' && result.warnings.length === 0 ? 'info' : 'warn';
}
