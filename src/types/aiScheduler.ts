/**
 * AI 智能日程规划的类型契约。
 *
 * 与 `src-tauri/src/commands/ai_scheduler/models.rs` 一一对应，字段名为 snake_case。
 */

export type AiPriority = 'high' | 'medium' | 'low';

export type AiTimeWindow = {
  /** 1 = 周一 … 7 = 周日 */
  weekday: number;
  /** 距 00:00 的分钟数，0..1440，5 分钟对齐 */
  start_minute: number;
  end_minute: number;
};

export type AiMealWindow = {
  kind: string;
  start_minute: number;
  end_minute: number;
};

export type AiPlannerPreferences = {
  auto_meals: boolean;
  adaptive_durations: boolean;
  rest_style: 'gentle' | 'balanced' | 'focused' | string;
  daily_target_minutes: number;
  memory_note: string;
  meal_windows: AiMealWindow[];
};

export type AiProviderPresetKey = 'deepseek' | 'openai' | 'custom';

/**
 * 结构化输出档位。
 * DeepSeek 的 Chat Completions 只支持 `json_object`；`json_schema` 仅 OpenAI 支持。
 * `auto` 由 `test_ai_scheduler_connection` 探测后回写为确定值并缓存。
 */
export type AiStructuredOutputMode = 'auto' | 'json_schema' | 'json_object';

export type AiSchedulerSettings = {
  enabled: boolean;
  provider_preset: AiProviderPresetKey | string;
  base_url: string;
  model: string;
  structured_output_mode: AiStructuredOutputMode | string;
  /** 由 `GET /models` 拉取，供下拉选择 */
  available_models: string[];
  /** 保存时写入；读取时恒为空串 */
  api_key: string;
  /** 回显密钥是否已配置的唯一字段；前端不得依赖 `api_key` */
  api_key_configured: boolean;
  timeout_seconds: number;
  max_retries: number;
  max_tokens: number;
  temperature: number;
  /** DeepSeek V4 默认开思考模式，此时 temperature 失效且答案在 reasoning_content */
  disable_thinking: boolean;
  available_windows: AiTimeWindow[];
  peak_windows: AiTimeWindow[];
  default_block_minutes: number;
  min_break_minutes: number;
  max_daily_minutes: number;
  send_notes: boolean;
  /** 用户是否已确认过「数据出境字段」声明；只在首次启用时弹确认框 */
  privacy_acknowledged: boolean;
  planner_preferences: AiPlannerPreferences;
};

/**
 * 服务商预设对照表。选中预设即自动填充 `base_url` / `model` / `structured_output_mode`，
 * 用户仍可手改。
 *
 * 注意：`deepseek-chat` 与 `deepseek-reasoner` 已于 2026/07/24 弃用，不要作为默认值。
 */
export const AI_PROVIDER_PRESETS: Array<{
  key: AiProviderPresetKey;
  label: string;
  base_url: string;
  model: string;
  structured_output_mode: AiStructuredOutputMode;
  /** 选择该预设时展示的提示 */
  hint: string | null;
}> = [
  {
    key: 'deepseek',
    label: 'DeepSeek（推荐，国内可直连）',
    base_url: 'https://api.deepseek.com',
    model: 'deepseek-v4-flash',
    structured_output_mode: 'json_object',
    hint: null,
  },
  {
    key: 'openai',
    label: 'OpenAI',
    base_url: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
    structured_output_mode: 'json_schema',
    hint: 'api.openai.com 在中国大陆通常无法直连，需要自备代理或改用下方的自定义接口地址。',
  },
  {
    key: 'custom',
    label: '自定义 / 中转网关',
    base_url: '',
    model: '',
    structured_output_mode: 'auto',
    hint: '任何兼容 POST /chat/completions 的网关都能接入；选它之后请自行填写接口地址与模型名。',
  },
];

export function findAiProviderPreset(key: string) {
  return AI_PROVIDER_PRESETS.find((preset) => preset.key === key) ?? null;
}

export type AiConnectionTestResult = {
  ok: boolean;
  model: string;
  latency_ms: number;
  message: string;
  structured_output_mode: AiStructuredOutputMode | string;
  available_models: string[];
};

export type AiPlanRequest = {
  /** 规划起始日期，同时也是**队列来源日期**：只排这一天的队列 */
  target_date: string;
  /** 1..7 */
  horizon_days: number;
  /** 参与排期的队列条目（`today_plan_items.id`，**不是**清单任务 id）；null = 当天队列全部条目 */
  queue_item_ids: number[] | null;
  /** null = 全部分类 */
  category_keys: string[] | null;
  respect_priority: boolean;
  /**
   * 旧字段，新前端恒传 true：锁定块与手动块从来都会保留。
   * 想重排 AI 之前写进日历的安排，用 `replace_ai_blocks`。
   */
  keep_locked_blocks: boolean;
  /**
   * 重新安排已写入日历、但还没开始的 AI 日程（未锁定、未完成），写入时旧块会被替换。
   * 默认 false：已在日历上的队列条目不再重复排期。
   */
  replace_ai_blocks: boolean;
  /** 自然语言补充（≤200 字） */
  extra_instruction: string | null;
  /**
   * AI 失败时是否允许改用本地启发式。默认 false：失败就报错，
   * 绝不悄悄用本地结果冒充 AI 排期；只有用户点了「改用本地排期」才置 true。
   */
  allow_local_fallback: boolean;
};

export type AiPlanItem = {
  /** 前端稳定 key，后端生成 */
  id: string;
  /** 队列条目的清单来源；手动加进队列的条目为 null */
  source_task_id: number | null;
  /** 队列条目 id（`today_plan_items.id`）：本条排期派生自哪一条 */
  source_today_item_id: number | null;
  schedule_date: string;
  start_minute: number;
  end_minute: number;
  /** 以下四项由后端按 item_id 从队列条目回填，模型不返回 */
  title: string;
  category_key: string;
  subject_id: number | null;
  priority: AiPriority;
  /** 模型给的排期理由，≤40 字 */
  rationale: string | null;
  /** 用户拖动过 */
  manually_adjusted: boolean;
  /** 与哪些已有 schedule_block 冲突 */
  conflict_with: number[];
  kind?: 'study' | 'meal' | string;
};

export type AiPlanWarningCode =
  | 'no_window'
  | 'over_capacity'
  | 'due_risk'
  | 'conflict'
  | 'unknown_task'
  | 'schema_repaired'
  | 'truncated'
  | 'snapshot_drift'
  | 'duplicate'
  /** 模型给的时间不可行（撞了三餐 / 已有日程 / 已过去 / 超上限）或漏排，已挪到最近空档 */
  | 'adjusted';

export type AiPlanWarning = {
  code: AiPlanWarningCode | string;
  message: string;
  /** 指向队列条目 id（`today_plan_items.id`），**不是**清单任务 id */
  queue_item_id: number | null;
  /** 指向 `AiPlanItem.id`，用于把告警挂到具体条目上 */
  item_id: string | null;
};

/**
 * 「排不下」的条目。
 *
 * 后端在 `stats.unscheduled_count` 之外单独给出明细，否则抽屉只能显示一个数字，
 * 用户无法判断是容量不足还是截止日已过。
 */
export type AiUnscheduledEntry = {
  /** `today_plan_items.id` */
  item_id: number;
  /** 后端按 item_id 回填，前端不必再查一次队列 */
  title: string;
  reason: string;
};

/** 已经在日历上、因此这次没有重复排期的队列条目。 */
export type AiAlreadyScheduledEntry = {
  /** `today_plan_items.id` */
  item_id: number;
  title: string;
  /** 例如「09-30 14:00 已在日历（手动安排）」 */
  detail: string;
};

export type AiPlanStats = {
  /** 学习条目数（拆段只算一条，不含三餐） */
  scheduled_count: number;
  unscheduled_count: number;
  /** 全部条目总时长（含三餐） */
  total_minutes: number;
  /** 超出单日学习上限的分钟数 */
  overflow_minutes: number;
  /** 学习总时长（不含三餐） */
  study_minutes: number;
  /** 每日目标 × 可排天数 */
  target_minutes: number;
  /** 被本地可行性修复挪动 / 补排的条目数 */
  adjusted_count: number;
};

export type AiProposalStatus = 'draft' | 'applied' | 'discarded' | 'expired';

export type AiProposalScope = 'day' | 'range' | 'window';

/** 草案来源引擎。S3 阶段恒为 `local_heuristic`，S4 接入模型后可能出现 `llm`。 */
export type AiPlanEngine = 'llm' | 'local_heuristic';

export type AiPlanProposal = {
  id: number;
  target_date: string;
  horizon_days: number;
  status: AiProposalStatus | string;
  scope: AiProposalScope | string;
  scope_window_start: number | null;
  scope_window_end: number | null;
  engine: AiPlanEngine | string;
  /** 走了降级路径（模型不可用 → 本地启发式）时为 true */
  degraded: boolean;
  model: string;
  created_at: string;
  items: AiPlanItem[];
  warnings: AiPlanWarning[];
  unscheduled: AiUnscheduledEntry[];
  stats: AiPlanStats;
  /** 模型（或本地规则）对整份安排的一句话说明；旧草案为 null */
  summary: string | null;
  /** 已在日历上、这次没有重复排期的条目 */
  already_scheduled: AiAlreadyScheduledEntry[];
  /** 写入时会被替换掉的旧 AI 日程数（勾选「重新安排」时才可能非零） */
  replaceable_block_count: number;
};

export type AiSchedulerErrorCode =
  | 'missing_api_key'
  | 'unauthorized'
  | 'forbidden'
  | 'rate_limited'
  | 'quota_exceeded'
  | 'timeout'
  | 'network'
  | 'server_error'
  | 'bad_request'
  | 'invalid_response'
  | 'conflict'
  | 'not_found'
  | 'db_error';

export type AiDegradedInfo = {
  kind: 'local_heuristic' | 'none' | string;
  description: string;
};

export type AiSchedulerError = {
  code: AiSchedulerErrorCode | string;
  /** 用户可读中文 */
  message: string;
  retryable: boolean;
  retry_after_seconds: number | null;
  degraded: AiDegradedInfo | null;
};

export type ScheduleChangeEvent = {
  kind:
    | 'task_created'
    | 'task_updated'
    | 'task_deleted'
    | 'task_completed'
    | 'due_changed'
    | 'block_moved'
    | 'block_deleted'
    | 'block_created';
  occurrence_date: string;
  task_id: number | null;
  block_id: number | null;
  delta_minutes: number | null;
};

export type AiApplyOptions = {
  /** 是否覆盖冲突块，默认 false */
  overwrite_conflicts: boolean;
  /** 是否跳过 ai_locked 块，默认 true */
  skip_locked: boolean;
};

export type AiApplyResult = {
  status: 'applied' | 'partial' | 'failed';
  created_count: number;
  skipped_count: number;
  conflicted_count: number;
  message: string;
  created_block_ids: number[];
  /** 逐条跳过/降级的具体原因，用于写入后展示「为什么少了这几条」 */
  warnings: AiPlanWarning[];
  /** 被新安排替换（删除）的旧 AI 日程数 */
  replaced_count: number;
};
