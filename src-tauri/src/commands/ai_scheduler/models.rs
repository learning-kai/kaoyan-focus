//! AI 智能日程规划的共享数据结构。
//!
//! 字段命名统一使用 snake_case，与 `src/types/aiScheduler.ts` 一一对应，
//! 因此**不需要** `#[serde(rename_all = ...)]`。

use serde::{Deserialize, Serialize};

// ── 默认值常量（见 docs/AI智能日程规划实现方案.md §3.2 / §5.4） ──

pub const DEFAULT_PROVIDER_PRESET: &str = "deepseek";
/// `api.openai.com` 在中国大陆通常不可直连，DeepSeek 可直连，因此默认它。
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_STRUCTURED_OUTPUT_MODE: &str = "auto";
pub const DEFAULT_TIMEOUT_SECONDS: i64 = 60;
pub const DEFAULT_MAX_RETRIES: i64 = 2;
pub const DEFAULT_MAX_TOKENS: i64 = 2048;
/// `max_tokens` 上限。DeepSeek 的合法区间是 1–8192，超出会直接 400。
pub const MAX_TOKENS_CEILING: i64 = 8192;
pub const MIN_MAX_TOKENS: i64 = 256;
pub const MAX_TIMEOUT_SECONDS: i64 = 300;
pub const MIN_TIMEOUT_SECONDS: i64 = 5;
pub const MAX_RETRIES_CEILING: i64 = 5;
pub const DEFAULT_TEMPERATURE: f64 = 0.2;
pub const DEFAULT_BLOCK_MINUTES: i64 = 45;
pub const DEFAULT_MIN_BREAK_MINUTES: i64 = 10;
pub const DEFAULT_MAX_DAILY_MINUTES: i64 = 480;
pub const DEFAULT_DAILY_TARGET_MINUTES: i64 = 360;
pub const DEFAULT_WINDOW_START_MINUTE: i64 = 8 * 60;
pub const DEFAULT_WINDOW_END_MINUTE: i64 = 22 * 60;

// ── 错误码（见 §6.1） ──

pub const ERR_MISSING_API_KEY: &str = "missing_api_key";
pub const ERR_UNAUTHORIZED: &str = "unauthorized";
pub const ERR_FORBIDDEN: &str = "forbidden";
pub const ERR_BAD_REQUEST: &str = "bad_request";
pub const ERR_RATE_LIMITED: &str = "rate_limited";
pub const ERR_QUOTA_EXCEEDED: &str = "quota_exceeded";
pub const ERR_TIMEOUT: &str = "timeout";
pub const ERR_NETWORK: &str = "network";
pub const ERR_SERVER_ERROR: &str = "server_error";
pub const ERR_INVALID_RESPONSE: &str = "invalid_response";
// 下面两个错误码在前端 `AiSchedulerErrorCode` 联合类型里已声明，Rust 侧要到
// S3（apply 写入冲突）与 S5/S6（草案不存在）才产生，此处先行声明以保持前后端契约一致。
#[allow(dead_code)]
pub const ERR_CONFLICT: &str = "conflict";
#[allow(dead_code)]
pub const ERR_NOT_FOUND: &str = "not_found";
pub const ERR_DB_ERROR: &str = "db_error";

/// 降级说明。`kind = "none"` 表示没有降级。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiDegradedInfo {
    pub kind: String,
    pub description: String,
}

/// 结构化错误信封。
///
/// **必须序列化成 JSON 字符串后作为 `Result::Err` 返回**：`src/services/tauriInvoke.ts`
/// 的 `normalizeTauriError` 对非 `Error` 对象会退化成 `String(reason)`，直接返回结构体会
/// 在前端变成 `"[object Object]"`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSchedulerError {
    pub code: String,
    /// 直接可展示给用户的中文文案。
    pub message: String,
    pub retryable: bool,
    pub retry_after_seconds: Option<f64>,
    pub degraded: Option<AiDegradedInfo>,
}

impl AiSchedulerError {
    pub fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable,
            retry_after_seconds: None,
            degraded: None,
        }
    }

    pub fn with_retry_after(mut self, seconds: f64) -> Self {
        self.retry_after_seconds = Some(seconds);
        self
    }

    /// 标记已降级。S2 阶段尚无调用点（降级发生在 S4 的排期链路），先保留以免 S4 重复改动契约。
    #[allow(dead_code)]
    pub fn with_degraded(mut self, kind: &str, description: impl Into<String>) -> Self {
        self.degraded = Some(AiDegradedInfo {
            kind: kind.to_string(),
            description: description.into(),
        });
        self
    }

    /// 序列化为 JSON 字符串信封；序列化失败时回落到一个安全的固定信封。
    pub fn to_envelope(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                r#"{{"code":"{ERR_DB_ERROR}","message":"未知错误","retryable":false,"retry_after_seconds":null,"degraded":null}}"#
            )
        })
    }
}

/// 每天的可安排时段。`weekday` 用 1 = 周一 … 7 = 周日。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiTimeWindow {
    pub weekday: i64,
    pub start_minute: i64,
    pub end_minute: i64,
}

/// 管家模式的固定生活安排。它们是排期的硬约束，同时会作为日程条目展示。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiMealWindow {
    pub kind: String,
    pub start_minute: i64,
    pub end_minute: i64,
}

fn default_meal_windows() -> Vec<AiMealWindow> {
    vec![
        AiMealWindow {
            kind: "早餐".to_string(),
            start_minute: 7 * 60,
            end_minute: 7 * 60 + 40,
        },
        AiMealWindow {
            kind: "午餐".to_string(),
            start_minute: 12 * 60,
            end_minute: 13 * 60,
        },
        AiMealWindow {
            kind: "晚餐".to_string(),
            start_minute: 18 * 60,
            end_minute: 19 * 60,
        },
    ]
}

fn default_rest_style() -> String {
    "balanced".to_string()
}

/// 只保存用户真正想长期记住的排期偏好，不保存模型返回内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiPlannerPreferences {
    pub auto_meals: bool,
    pub adaptive_durations: bool,
    pub rest_style: String,
    pub daily_target_minutes: i64,
    pub memory_note: String,
    pub meal_windows: Vec<AiMealWindow>,
}

impl Default for AiPlannerPreferences {
    fn default() -> Self {
        Self {
            auto_meals: true,
            adaptive_durations: true,
            rest_style: default_rest_style(),
            daily_target_minutes: DEFAULT_DAILY_TARGET_MINUTES,
            memory_note: String::new(),
            meal_windows: default_meal_windows(),
        }
    }
}

/// 服务商预设。选中即自动填充 `base_url` / `model` / `structured_output_mode`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiProviderPreset {
    pub key: &'static str,
    pub base_url: &'static str,
    pub model: &'static str,
    pub structured_output_mode: &'static str,
}

pub const PROVIDER_PRESETS: [AiProviderPreset; 3] = [
    AiProviderPreset {
        key: "deepseek",
        base_url: "https://api.deepseek.com",
        model: "deepseek-v4-flash",
        // DeepSeek 的 Chat Completions 不支持 json_schema，只支持 json_object。
        structured_output_mode: "json_object",
    },
    AiProviderPreset {
        key: "openai",
        base_url: "https://api.openai.com/v1",
        model: "gpt-4o-mini",
        structured_output_mode: "json_schema",
    },
    AiProviderPreset {
        key: "custom",
        base_url: "",
        model: "",
        // 自建 / 中转网关能力未知，交给连通性测试探测。
        structured_output_mode: "auto",
    },
];

pub fn provider_preset(key: &str) -> Option<AiProviderPreset> {
    PROVIDER_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.key == key)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
// 容器级 default：旧版本写入的 JSON 缺字段时按默认值补齐，避免升级后读取直接失败。
#[serde(default)]
pub struct AiSchedulerSettings {
    pub enabled: bool,
    pub provider_preset: String,
    pub base_url: String,
    pub model: String,
    /// `auto` | `json_schema` | `json_object`；`auto` 由连通性测试探测后回写。
    pub structured_output_mode: String,
    /// 由 `GET /models` 拉取，供下拉选择，避免用户手打模型名写错。
    #[serde(default)]
    pub available_models: Vec<String>,
    /// 保存时写入；读取时**恒为空串**，前端不得依赖此字段。
    #[serde(default)]
    pub api_key: String,
    /// 回显密钥是否已配置的唯一字段（对齐 `email.rs` 的 `password_configured`）。
    #[serde(default)]
    pub api_key_configured: bool,
    pub timeout_seconds: i64,
    pub max_retries: i64,
    pub max_tokens: i64,
    pub temperature: f64,
    /// DeepSeek V4 默认开思考模式，此时 `temperature` 失效且答案在 `reasoning_content`。
    pub disable_thinking: bool,
    pub available_windows: Vec<AiTimeWindow>,
    pub peak_windows: Vec<AiTimeWindow>,
    pub default_block_minutes: i64,
    pub min_break_minutes: i64,
    pub max_daily_minutes: i64,
    /// 是否把任务备注一并发给模型。默认 true（用户已确认）。
    pub send_notes: bool,
    /// 用户是否已确认过「数据出境字段」声明。用于只在首次启用时弹确认框。
    pub privacy_acknowledged: bool,
    /// 自动排期的高层偏好。旧版本配置缺失时使用 `AiPlannerPreferences::default()`。
    #[serde(default)]
    pub planner_preferences: AiPlannerPreferences,
}

fn default_weekdays_window() -> Vec<AiTimeWindow> {
    (1..=7)
        .map(|weekday| AiTimeWindow {
            weekday,
            start_minute: DEFAULT_WINDOW_START_MINUTE,
            end_minute: DEFAULT_WINDOW_END_MINUTE,
        })
        .collect()
}

impl Default for AiSchedulerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_preset: DEFAULT_PROVIDER_PRESET.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            structured_output_mode: DEFAULT_STRUCTURED_OUTPUT_MODE.to_string(),
            available_models: Vec::new(),
            api_key: String::new(),
            api_key_configured: false,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            max_retries: DEFAULT_MAX_RETRIES,
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
            disable_thinking: true,
            available_windows: default_weekdays_window(),
            peak_windows: Vec::new(),
            default_block_minutes: DEFAULT_BLOCK_MINUTES,
            min_break_minutes: DEFAULT_MIN_BREAK_MINUTES,
            max_daily_minutes: DEFAULT_MAX_DAILY_MINUTES,
            send_notes: true,
            privacy_acknowledged: false,
            planner_preferences: AiPlannerPreferences::default(),
        }
    }
}

/// 连通性测试结果。一次性完成三件事：验证可用、探测结构化输出档位、拉取模型列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConnectionTestResult {
    pub ok: bool,
    pub model: String,
    pub latency_ms: i64,
    pub message: String,
    /// 探测后确定的档位（已回写 `AiSchedulerSettings::structured_output_mode`）。
    pub structured_output_mode: String,
    pub available_models: Vec<String>,
}

// ── S3：排期请求 / 草案 / 上下文快照（见 §3.2 §3.3 §4） ──

/// `horizon_days` 合法区间。
pub const MIN_HORIZON_DAYS: i64 = 1;
pub const MAX_HORIZON_DAYS: i64 = 7;
/// `extra_instruction` 的字符上限（§3.2）。
pub const MAX_EXTRA_INSTRUCTION_CHARS: usize = 200;
/// 草案在多少天后自动过期（§6.4）。启动清理时使用。
pub const PROPOSAL_EXPIRE_DAYS: i64 = 3;

// 草案警告码（§5.5 / §6.4）。前后端共用同一组字面量。
pub const WARN_NO_WINDOW: &str = "no_window";
pub const WARN_OVER_CAPACITY: &str = "over_capacity";
pub const WARN_DUE_RISK: &str = "due_risk";
pub const WARN_CONFLICT: &str = "conflict";
pub const WARN_UNKNOWN_TASK: &str = "unknown_task";
/// S4 模型路径：`json_object` 档返回的结构有偏差、被修补后才解析成功。
pub const WARN_SCHEMA_REPAIRED: &str = "schema_repaired";
/// S4 模型路径：`finish_reason = length`，回答被截断。
pub const WARN_TRUNCATED: &str = "truncated";
pub const WARN_SNAPSHOT_DRIFT: &str = "snapshot_drift";
/// 同一条目在同一天被排了多次，只保留第一条（§6.4）。原方案只提到「专用 warning」而未命名。
pub const WARN_DUPLICATE: &str = "duplicate";

// 草案状态与来源引擎。
pub const PROPOSAL_STATUS_DRAFT: &str = "draft";
pub const PROPOSAL_STATUS_APPLIED: &str = "applied";
pub const PROPOSAL_STATUS_DISCARDED: &str = "discarded";
pub const PROPOSAL_STATUS_EXPIRED: &str = "expired";
/// S4 起：模型路径产出 `llm`；本地启发式只在显式降级时使用。
pub const ENGINE_LLM: &str = "llm";
pub const ENGINE_LOCAL_HEURISTIC: &str = "local_heuristic";
pub const SCOPE_DAY: &str = "day";
/// S6 的 `replan` 会把草案范围收窄到「受影响的日期段」或「单个时间窗」，
/// 两个取值先声明，避免届时要同时改前后端契约。
#[allow(dead_code)]
pub const SCOPE_RANGE: &str = "range";
#[allow(dead_code)]
pub const SCOPE_WINDOW: &str = "window";

fn default_true() -> bool {
    true
}

/// `preview_ai_schedule` 的入参。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPlanRequest {
    /// 规划起始日期，同时也是**队列来源日期**：只排这一天的队列。
    pub target_date: String,
    /// 1..7，越界会被钳制（见 `normalize_plan_request`）。
    pub horizon_days: i64,
    /// `None` = 该队列的全部条目。值为 `today_plan_items.id`，**不是** `checklist_tasks.id`。
    #[serde(default)]
    pub queue_item_ids: Option<Vec<i64>>,
    /// `None` = 全部分类。
    #[serde(default)]
    pub category_keys: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub respect_priority: bool,
    #[serde(default = "default_true")]
    pub keep_locked_blocks: bool,
    #[serde(default)]
    pub extra_instruction: Option<String>,
    /// AI 调用失败时是否允许改用本地启发式。
    ///
    /// 默认 `false`：失败就报错，**绝不悄悄用本地结果冒充 AI 排期**。
    /// 前端在错误卡片上放一个「改用本地排期」按钮，只有用户点了才会带 `true` 重发。
    #[serde(default)]
    pub allow_local_fallback: bool,
}

impl Default for AiPlanRequest {
    fn default() -> Self {
        Self {
            target_date: String::new(),
            horizon_days: 1,
            queue_item_ids: None,
            category_keys: None,
            respect_priority: true,
            keep_locked_blocks: true,
            extra_instruction: None,
            allow_local_fallback: false,
        }
    }
}

/// 草案中的一条排期。`title` / `category_key` / `subject_id` / `priority` 均由后端按
/// `source_today_item_id` 从队列条目回填——模型只回 `item_id` 与时间，杜绝编造（§2.1 原则 1）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPlanItem {
    /// 前端稳定 key，后端生成（`{date}-{start}-{item_id}`）。
    pub id: String,
    /// 队列条目的清单来源；手动条目为 `None`。仅作展示与追溯，写库链路不依赖它。
    pub source_task_id: Option<i64>,
    /// 队列条目 id：本条排期是从哪条队列条目派生的。`Option` 只为兼容旧快照，
    /// 新草案里恒为 `Some`。
    pub source_today_item_id: Option<i64>,
    pub schedule_date: String,
    pub start_minute: i64,
    pub end_minute: i64,
    pub title: String,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub priority: String,
    /// 模型给的排期理由，≤40 字。本地启发式路径下为 `None`。
    pub rationale: Option<String>,
    /// 用户在预览里拖动过（S5 起使用）。
    pub manually_adjusted: bool,
    /// 与哪些已有 `schedule_blocks.id` 冲突。
    pub conflict_with: Vec<i64>,
    /// `study` 为学习任务，`meal` 为管家自动插入的生活安排。
    #[serde(default = "default_plan_item_kind")]
    pub kind: String,
}

fn default_plan_item_kind() -> String {
    "study".to_string()
}

impl AiPlanItem {
    pub fn duration_minutes(&self) -> i64 {
        (self.end_minute - self.start_minute).max(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPlanWarning {
    pub code: String,
    pub message: String,
    /// 指向 `today_plan_items.id`（排期单位）。字段名刻意不叫 `task_id`：它**不是**
    /// `checklist_tasks.id`，混用会让人误去清单表里查。
    pub queue_item_id: Option<i64>,
    /// 指向 `AiPlanItem.id`（前端稳定 key），用于把告警挂到具体条目上。
    pub item_id: Option<String>,
}

impl AiPlanWarning {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            queue_item_id: None,
            item_id: None,
        }
    }

    pub fn for_queue_item(mut self, queue_item_id: Option<i64>) -> Self {
        self.queue_item_id = queue_item_id;
        self
    }

    pub fn for_item(mut self, item_id: Option<String>) -> Self {
        self.item_id = item_id;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPlanStats {
    pub scheduled_count: i64,
    pub unscheduled_count: i64,
    pub total_minutes: i64,
    /// 超出单日容量上限的分钟数，用于前端提示「有 N 分钟排不下」。
    pub overflow_minutes: i64,
}

/// 「排不下」的条目。原方案 §3.2 只在 stats 里给了计数，但抽屉必须能列出**哪些**
/// 条目没排上及其原因，否则用户无法判断是容量不足还是截止日已过。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnscheduledEntry {
    /// `today_plan_items.id`
    pub item_id: i64,
    /// 后端按 `item_id` 回填，避免前端再查一次队列。
    pub title: String,
    pub reason: String,
}

/// 返回给前端的草案。`items` / `warnings` 从库里的 JSON 列解析而来。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlanProposal {
    pub id: i64,
    pub target_date: String,
    pub horizon_days: i64,
    pub status: String,
    pub scope: String,
    pub scope_window_start: Option<i64>,
    pub scope_window_end: Option<i64>,
    pub engine: String,
    pub degraded: bool,
    pub model: String,
    pub created_at: String,
    pub items: Vec<AiPlanItem>,
    pub warnings: Vec<AiPlanWarning>,
    pub unscheduled: Vec<UnscheduledEntry>,
    pub stats: AiPlanStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiApplyOptions {
    #[serde(default)]
    pub overwrite_conflicts: bool,
    #[serde(default = "default_true")]
    pub skip_locked: bool,
}

impl Default for AiApplyOptions {
    fn default() -> Self {
        Self {
            overwrite_conflicts: false,
            skip_locked: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiApplyResult {
    /// `applied` | `partial` | `failed`
    pub status: String,
    pub created_count: i64,
    pub skipped_count: i64,
    pub conflicted_count: i64,
    pub message: String,
    pub created_block_ids: Vec<i64>,
    /// 逐条跳过/降级的**具体原因**（任务已删除、不在当前可用时段、与哪条日程冲突…）。
    ///
    /// 仅有计数时用户无从判断该改哪里，因此这里把 `apply` 阶段的告警一并带回；
    /// 字段是追加的，旧前端忽略它也不会出错。
    pub warnings: Vec<AiPlanWarning>,
}

/// 日程变动事件。S6 的 `replan_ai_schedule_after_change` 用它定位受影响窗口。
///
/// S3 阶段尚无生产者：前端 `ScheduleChangeEvent` 与命令签名已就位，
/// 待 S6 补上 `replan.rs` 后即被消费。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleChangeEvent {
    pub kind: String,
    pub occurrence_date: String,
    pub task_id: Option<i64>,
    pub block_id: Option<i64>,
    pub delta_minutes: Option<i64>,
}

// ── 上下文快照：既喂给模型，也用于 apply 阶段的漂移检测（§3.3） ──

/// 参与排期的**队列条目**（`today_plan_items` 的一行）。
///
/// 排期单位刻意是队列条目而不是清单任务：用户把任务加进「今日 / 计划队列」表达的是
/// 「今天要做这些」，那才是排期的输入。队列里手动新建的临时条目（`source_task_id = None`）
/// 同样是队列成员，必须一起排。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextQueueItem {
    /// `today_plan_items.id` —— 本次排期的唯一标识，也是模型唯一可以引用的 id。
    pub item_id: i64,
    /// 队列条目的清单来源；手动新建的临时条目为 `None`。
    #[serde(default)]
    pub source_task_id: Option<i64>,
    pub title: String,
    /// 取值域：politics | english | math | major | general
    pub category_key: String,
    /// 显示名，从 settings 键 `checklist_category_names` 解析而来，供 prompt 使用。
    pub category_label: String,
    pub subject_id: Option<i64>,
    pub priority: String,
    /// 0 表示未估时，排期时取 `default_block_minutes`。
    pub estimated_minutes: i64,
    pub due_date: Option<String>,
    /// 仅 `send_notes = true` 时填充（§7 隐私边界）。
    pub note: Option<String>,
}

impl ContextQueueItem {
    /// 实际占用的时长：未估时任务回落为默认时长（§6.4）。
    pub fn effective_minutes(&self, default_block_minutes: i64) -> i64 {
        if self.estimated_minutes > 0 {
            self.estimated_minutes
        } else {
            default_block_minutes.max(5)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    pub block_id: i64,
    pub date: String,
    pub start_minute: i64,
    pub end_minute: i64,
    pub title: String,
    /// `ai_locked` 或用户手动块 → 视为硬约束，不参与重排。
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanContext {
    pub generated_at: String,
    pub horizon_days: i64,
    /// horizon 的具体起止日期。原方案 §3.3 未列出，但 prompt 与漂移检测都需要。
    pub horizon_start: String,
    pub horizon_end: String,
    /// 排期来源：`target_date` 那天的队列条目。
    ///
    /// 旧快照里这个字段叫 `tasks`，加 `default` 是为了让库里已经存在的草案快照仍能解析。
    #[serde(default, alias = "tasks")]
    pub queue_items: Vec<ContextQueueItem>,
    pub existing_blocks: Vec<ContextBlock>,
    /// 按 weekday 存储；展开到具体日期由 `expand_windows` 负责。
    pub available_windows: Vec<AiTimeWindow>,
    pub peak_windows: Vec<AiTimeWindow>,
    pub min_break_minutes: i64,
    pub max_daily_minutes: i64,
    pub default_block_minutes: i64,
    /// 仅包含排期所需的管家偏好；不包含 API Key。
    #[serde(default)]
    pub planner_preferences: AiPlannerPreferences,
}

impl PlanContext {
    pub fn item_by_id(&self, item_id: i64) -> Option<&ContextQueueItem> {
        self.queue_items.iter().find(|item| item.item_id == item_id)
    }
}

// ── 排期候选（模型输出 / 本地启发式输出共用同一形态，然后走同一个校验器） ──

/// 模型返回的原始条目。**不含** title / category —— 那些由 `validator` 按 `item_id` 回填。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawPlanItem {
    /// `today_plan_items.id`
    pub item_id: i64,
    pub date: String,
    pub start_minute: i64,
    pub end_minute: i64,
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawUnscheduledItem {
    /// `today_plan_items.id`
    pub item_id: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// 模型返回的完整结构（§5.3 的 response schema）。
///
/// 本地启发式排期器也产出这个结构，从而与 AI 路径共用 `validator::validate`，
/// 保证两条路径的合法性判定完全一致。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RawPlanResponse {
    #[serde(default)]
    pub items: Vec<RawPlanItem>,
    #[serde(default)]
    pub unscheduled: Vec<RawUnscheduledItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_openable_out_of_the_box() {
        let settings = AiSchedulerSettings::default();
        // 默认必须是可直连的 DeepSeek，否则「开箱可用」不成立。
        assert_eq!(settings.provider_preset, "deepseek");
        assert_eq!(settings.base_url, "https://api.deepseek.com");
        assert_eq!(settings.model, "deepseek-v4-flash");
        // 档位默认为 auto，由连通性测试探测后回写为确定值（见 §5.4）。
        assert_eq!(
            settings.structured_output_mode,
            DEFAULT_STRUCTURED_OUTPUT_MODE
        );
        assert!(settings.disable_thinking);
        assert!(settings.send_notes);
        assert_eq!(settings.available_windows.len(), 7);
        assert!(settings.api_key.is_empty());
        assert!(!settings.api_key_configured);
    }

    #[test]
    fn deepseek_preset_declares_json_object_not_json_schema() {
        // DeepSeek 的 Chat Completions 不支持 json_schema；写错会让首次请求白挨一个 400。
        let preset = provider_preset("deepseek").unwrap();
        assert_eq!(preset.structured_output_mode, "json_object");
        assert_ne!(
            preset.structured_output_mode,
            DEFAULT_STRUCTURED_OUTPUT_MODE
        );
    }

    #[test]
    fn provider_preset_lookup_covers_all_keys() {
        assert_eq!(
            provider_preset("deepseek").unwrap().model,
            "deepseek-v4-flash"
        );
        assert_eq!(
            provider_preset("openai").unwrap().base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(
            provider_preset("custom").unwrap().structured_output_mode,
            "auto"
        );
        assert!(provider_preset("nope").is_none());
    }

    #[test]
    fn error_envelope_is_valid_json_and_keeps_code() {
        let error = AiSchedulerError::new(ERR_RATE_LIMITED, "请求过于频繁", true)
            .with_retry_after(2.0)
            .with_degraded("local_heuristic", "已改用本地启发式排期");
        let decoded: AiSchedulerError =
            serde_json::from_str(&error.to_envelope()).expect("envelope must be valid json");
        assert_eq!(decoded.code, ERR_RATE_LIMITED);
        assert!(decoded.retryable);
        assert_eq!(decoded.retry_after_seconds, Some(2.0));
        assert_eq!(decoded.degraded.unwrap().kind, "local_heuristic");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = AiSchedulerSettings::default();
        let raw = serde_json::to_string(&settings).expect("serialize settings");
        let decoded: AiSchedulerSettings =
            serde_json::from_str(&raw).expect("deserialize settings");
        assert_eq!(decoded, settings);
    }
}
