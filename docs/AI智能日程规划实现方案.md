# AI 智能日程规划功能 · 实现方案

> 目标：在现有清单 / 日历能力之上，接入 ChatGPT，把清单中的待办一键排布到日历时间轴；生成结果先预览、可微调，确认后才写入；日程变动后可一键重排受影响时段；API Key 安全存储，失败与异常有明确降级与重试。

---

## 0. 结论速览

| 项 | 结论 |
| --- | --- |
| 架构落点 | 复用现有 Tauri 命令层 + SQLite，新增 `commands/ai_scheduler/` 子模块，不引入新框架 |
| API Key 存储 | **直接复用 `src-tauri/src/credential.rs` 的 Windows DPAPI 加密**（`dpapi:v1:` 前缀），不新建加密方案 |
| 预览机制 | 新增独立表 `ai_plan_proposals`，草案与正式日程物理分离，未确认绝不写 `schedule_blocks` |
| 防模型幻觉 | LLM **只输出 `item_id` + 起止分钟 + 理由**；标题 / 分类 / 科目由后端按 `item_id` 回填（排期单位是队列条目，见 §12.3） |
| 降级方案 | 内置**确定性本地启发式排期器**，AI 不可用时功能不失效，只是质量降级 |
| 需要新增列 | `checklist_tasks` / `today_plan_items` 各 2 列（`priority`、`estimated_minutes`）+ `checklist_tasks.ai_pinned`；`schedule_blocks` 3 列（`source_task_id`、`source_proposal_id`、`ai_locked`） |
| 新增命令 | 10 个（设置 3 + 草案 5 + 重排 1 + 查询 1） |
| 接口协议 | **OpenAI 兼容 `POST /chat/completions`**，`Authorization: Bearer <key>`。DeepSeek / 通义 / 各类中转网关通用 |
| 默认服务商 | **DeepSeek**（`https://api.deepseek.com`，国内直连可用），模型 `deepseek-v4-flash` |
| 结构化输出 | DeepSeek 的 Chat Completions **只支持 `json_object`，不支持 `json_schema`**；由连通性测试探测并缓存实际可用档位 |

---

## 1. 现状核对（基于已读代码）

| 现有能力 | 位置 | 复用方式 |
| --- | --- | --- |
| Windows DPAPI 加密读写 | `src-tauri/src/credential.rs` → `set_secret` / `get_secret` / `set_secret_if_changed` / `secret_configured` | **直接用于 API Key**，无需自研加密 |
| 密钥「只回布尔不回明文」范式 | `commands/email.rs`（`password_configured`）、`commands/caldav.rs`（第 1410–1501 行） | 照抄 `api_key_configured` 模式 |
| 设置持久化 | `settings` 表（key / value / updated_at） | 配置项直接加 key，无需建表 |
| 清单任务模型 | `checklist_tasks`（id, board_scope, subject_id, column_id, title, note, due_date, sort_order, completed, …） | **缺 `priority` / `estimated_minutes`，需迁移** |
| 分类标签 | `checklist_tasks.board_scope` → `ChecklistCategoryKey`（`checklist.rs:856` `map_board_scope_to_category_key`） | 分类键是 **`politics` / `english` / `math` / `major` / `general` 五个固定值**，显示名存于 settings 键 `checklist_category_names`（默认 政治/英语/数学/专业课/通用）。**`checklist_columns` 只是每个 `board_scope` 下自动建的一行占位分组（`checklist.rs:193`），不承载分类语义，不要误用** |
| 日历时间轴 | `schedule_blocks`（schedule_date, start_minute, end_minute, subject_id, category_key, status, …） | 已是分钟粒度，AI 输出可**直接落库** |
| 冲突标记 | `commands/schedule.rs::mark_conflicts`（第 916 行） | 提炼为共享校验器 |
| HTTP 客户端范式 | `commands/feishu/auth.rs::http_client`（30s 超时）、`commands/caldav.rs`（45s + 10s connect） | 新 client 照此写法，`reqwest` blocking + rustls 已在 `Cargo.toml` |
| 兼容迁移助手 | `storage/db.rs::add_column_if_missing`（第 734 行） | 加列迁移直接调用 |
| 命令注册 | `src-tauri/src/lib.rs` → `generate_handler!`（第 456 行起） | 新命令登记于此 |
| 导航 | `src/types/navigation.ts` 的 `AppPage` + `src/navigation.tsx` 的 `pages` | 若新增页面需同步改两处 |
| 同步触发 | `commands/schedule.rs::trigger_shared_sync` | 写入日程后调用，保证对象存储 / 飞书 / CalDAV 同步 |

**结论：本功能是「加列 + 加子模块 + 加抽屉」，不需要动现有架构。**

---

## 2. 总体设计

### 2.1 四条不可让步的设计原则

1. **AI 不产出实体，只产出索引。**
   LLM 返回 `{item_id, date, start_minute, end_minute, rationale}`；标题、分类、科目、截止日全部由后端用 `item_id` 从数据库回填。这样模型无法编造条目、无法篡改标题，也让 schema 校验变得简单。

2. **草案与日程物理隔离。**
   全部 AI 结果落在 `ai_plan_proposals`（草稿表），用户点「确认」才在事务里写 `schedule_blocks`。满足「先预览、可微调、确认后写入」的硬要求，也天然支持「放弃」。

3. **合法性由确定性代码保证，AI 只做偏好增强。**
   「不重叠 / 在可用时段内 / 不排到截止日之后 / 单日不超容量」全部由 Rust 校验器与本地启发式排期器兜底。AI 的价值在于语义权衡（哪个任务更该占高效时段、同类是否连排）。**这条保证了降级路径可用**。

4. **重排只动受影响窗口。**
   窗口外的时间块视为硬约束，窗口内才交给 AI 重排。避免「改一个任务、全天被推翻」。

### 2.2 数据流

```
清单任务(含优先级/耗时/截止日/分类)
        │
        ├─→ context.rs  收集上下文快照
        │      ├ 未完成任务 + 属性
        │      ├ 用户可用时段 / 每日高效时段
        │      ├ 已有 schedule_blocks（7 天 horizon）
        │      └ 当日容量、最小间隔、默认时长
        │
        ├─→ prompt.rs   组装 System + User Prompt（分钟制整数，无时区歧义）
        │
        ├─→ client.rs   OpenAI Chat Completions（json_schema 严格模式）
        │      └ 失败 → 重试 → 仍失败 → 降级到 planner::plan_locally()
        │
        ├─→ validator.rs 逐条硬校验，非法条目丢弃并记 warning
        │
        └─→ planner.rs  写入 ai_plan_proposals（status = draft）
                  │
                  ▼
        AiPlanDrawer.tsx  预览时间轴（可拖拽微调 / 可删条目 / 可重新生成）
                  │
                  ▼  用户点「写入日历」
        apply.rs 二次校验（快照漂移 + 重叠 + 时段合法性）
                  │  事务写入 schedule_blocks（带 source_proposal_id）
                  ▼
        proposal.status = applied → trigger_shared_sync()
```

---

## 3. 数据结构设计

### 3.1 SQLite 迁移（`src-tauri/src/storage/db.rs`）

现有表加列（走 `add_column_if_missing`）：

```sql
-- checklist_tasks
ALTER TABLE checklist_tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium';      -- high | medium | low
ALTER TABLE checklist_tasks ADD COLUMN estimated_minutes INTEGER NOT NULL DEFAULT 0; -- 0 = 未估时，取 default_block_minutes
ALTER TABLE checklist_tasks ADD COLUMN ai_pinned INTEGER NOT NULL DEFAULT 0;         -- 1 = 用户钉住，不参与自动排期

-- today_plan_items（同样需要，因为它可脱离清单任务独立创建）
ALTER TABLE today_plan_items ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium';
ALTER TABLE today_plan_items ADD COLUMN estimated_minutes INTEGER NOT NULL DEFAULT 0;

-- schedule_blocks
ALTER TABLE schedule_blocks ADD COLUMN source_task_id INTEGER;                       -- 直指 checklist_tasks.id（见下方「链路完整性」）
ALTER TABLE schedule_blocks ADD COLUMN source_proposal_id INTEGER;                   -- 来源草案，便于「重排受影响的时段」定位
ALTER TABLE schedule_blocks ADD COLUMN ai_locked INTEGER NOT NULL DEFAULT 0;         -- 1 = 用户手动微调过，重排时默认保留
```

**链路完整性（必须做，否则功能残缺）**

`schedule_blocks` 原本只有 `source_today_item_id`，它指向 `today_plan_items`，再由 `today_plan_items.source_task_id` 间接指向 `checklist_tasks`。如果用户排的是「清单里但未加入今日计划」的任务，apply 之后这条块**没有任何字段能指回原任务**，后果是：

- 勾选清单任务不会联动日程块状态；
- 「重排受影响的时段」无法从任务反查它落在哪些块上。

因此采取双保险：

1. `schedule_blocks` 新增 `source_task_id`，apply 时无论来源如何都写入原始任务 id；
2. 若该任务当天不在 `today_plan_items` 中，apply 时**自动补建**一条今日计划（复用 `commands/checklist.rs` 的 `add_task_to_today_plan` 逻辑），保证「今日计划」与「日历」视图一致，同时填上 `source_today_item_id`。

新增草案表：

```sql
CREATE TABLE IF NOT EXISTS ai_plan_proposals (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  proposal_date TEXT NOT NULL,               -- 目标起始日 YYYY-MM-DD
  horizon_days INTEGER NOT NULL DEFAULT 1,   -- 1..7
  status TEXT NOT NULL DEFAULT 'draft',      -- draft | applied | discarded | expired
  scope TEXT NOT NULL DEFAULT 'day',         -- day | range | window（window = 局部重排）
  scope_window_start INTEGER,                -- 局部重排窗口起始分钟
  scope_window_end INTEGER,                  -- 局部重排窗口结束分钟
  engine TEXT NOT NULL DEFAULT 'llm',        -- llm | local_heuristic（降级来源）
  model TEXT NOT NULL DEFAULT '',
  degraded INTEGER NOT NULL DEFAULT 0,       -- 1 = 本次为降级结果
  source_snapshot TEXT NOT NULL,             -- JSON：生成时的任务/日程快照，用于 apply 时漂移检测
  items_json TEXT NOT NULL,                  -- JSON：AiPlanItem[]
  warnings_json TEXT NOT NULL DEFAULT '[]',  -- JSON：AiPlanWarning[]
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_plan_proposals_date
  ON ai_plan_proposals (proposal_date, status, id);
```

新增 settings 键（密钥单独走 credential，其余集中一个 JSON）：

| key | 形态 | 说明 |
| --- | --- | --- |
| `ai_scheduler_enabled` | `"0"` / `"1"` | 总开关 |
| `ai_scheduler_base_url` | 字符串 | 默认 `https://api.openai.com/v1`，允许自建网关 / 代理 |
| `ai_scheduler_model` | 字符串 | 默认 `gpt-4o-mini`，可换 |
| `ai_scheduler_timeout_seconds` | 数字字符串 | 默认 `60` |
| `ai_scheduler_max_retries` | 数字字符串 | 默认 `2` |
| `ai_scheduler_preferences` | JSON | 可用时段、高效时段、默认时长、最小间隔、单日容量等（见 3.3） |
| `ai_scheduler_api_key` | **DPAPI 密文** | 走 `credential::set_secret`，读取时永不回明文 |

### 3.1.1 跨设备同步的连带改动（原方案完全遗漏）

**这是本次审查中最严重的一处遗漏。** `checklist_tasks` / `today_plan_items` 的字段不止存在于本地表，它们还作为**跨设备同步载荷**在设备间流转：

```rust
// src-tauri/src/sync_package/models.rs:176
pub struct SharedChecklistTask {
    pub sync_id: String,
    pub category_key: Option<String>,
    ...
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}
```

因此加 `priority` / `estimated_minutes` **不是「加两列」那么小**，它会穿透整个同步链路。经清点，需要同步改动的写入点共 **17 处**：

| 文件 | 处数 | 性质 |
| --- | --- | --- |
| `commands/checklist.rs` | 6 | 主业务写入 |
| `sync_package/identity.rs` | 4 | 同步落库（`1934` / `2018` 等） |
| `commands/feishu/links.rs` | 4 | 飞书任务映射写入 |
| `commands/schedule.rs` | 4 | 从日程生成今日任务 |
| `sync_package/models.rs` / `export.rs` | 3 | 载荷模型 + 导出 + `DeletedPayload` 转换 |
| `dashboard_server.rs` | 1 | 仪表盘补写（已显式列名，安全） |

**兼容性结论（已核实，可以放心做）**：

- `SYNC_SCHEMA_VERSION = 2`（`models.rs:8`），但 `merge.rs:102-104` 用的是 `.max(local, remote)` ——**版本号是取最大值而非相等校验**，所以升到 3 不会让旧设备的数据被拒绝。
- 载荷字段全部是 `Option<T>`，合并逻辑逐字段 `if let Some(...)` ——**`None` 表示「不覆盖」**。因此新字段声明为 `Option<String>` / `Option<i64>` 后，旧版本设备导出的载荷反序列化得到 `None`，不会把新设备上已填的优先级清空。

**但实现时必须遵守两条**：

1. 新字段在载荷里必须是 `Option<...>`，且不能加 `#[serde(default = "...")]` 之类的默认值填充——那会让 `None` 变成默认值，从而**覆盖**对端已设的真实值。
2. `export.rs:244` 与 `:631`（`DeletedPayload::from`）两处都要改：前者填真实值，后者必须填 `None`（删除事件不携带业务字段）。

> 若不做这一步，后果是：用户在 A 设备设了优先级，同步到 B 设备后优先级丢失；且 A 设备后续同步可能把 B 设备的空值反写回来，形成**优先级反复被清空**的诡异现象。

**应对**：把 S1 从「数据层」扩展为「数据层 + 同步链路」，S1 的验收必须包含一次真实的双设备同步往返（或 `sync_package/tests.rs` 里的载荷往返单测）。



### 3.2 前端类型 `src/types/aiScheduler.ts`（新增）

```ts
export type AiPriority = 'high' | 'medium' | 'low';

export type AiTimeWindow = {
  weekday: number;       // 1 = 周一 … 7 = 周日
  start_minute: number;  // 距 00:00 的分钟数，0..1439，5 分钟对齐
  end_minute: number;
};

export type AiProviderPreset = 'deepseek' | 'openai' | 'custom';

// 结构化输出档位。DeepSeek 只支持 json_object；OpenAI 支持 json_schema。
// 'auto' 由 test_ai_scheduler_connection 探测后回写为确定值并缓存。
export type AiStructuredOutputMode = 'auto' | 'json_schema' | 'json_object';

export type AiSchedulerSettings = {
  enabled: boolean;
  provider_preset: AiProviderPreset;  // 选择预设时自动填充 base_url / model / structured_output_mode
  base_url: string;                   // 默认 https://api.deepseek.com
  model: string;                      // 默认 deepseek-v4-flash
  structured_output_mode: AiStructuredOutputMode; // 默认 'auto'
  available_models: string[];         // 由 GET /models 拉取，供下拉选择
  api_key: string;                    // 保存时写入；读取时恒为空串
  api_key_configured: boolean;        // 唯一用于回显的状态
  timeout_seconds: number;            // 默认 60
  max_retries: number;                // 默认 2
  max_tokens: number;                 // 默认 2048，DeepSeek 上限 8192
  temperature: number;                // 默认 0.2
  disable_thinking: boolean;          // 默认 true（见 §5.4 说明）
  available_windows: AiTimeWindow[];  // 每天可安排时段
  peak_windows: AiTimeWindow[];       // 每日高效时段
  default_block_minutes: number;      // 未估时任务默认时长，默认 45
  min_break_minutes: number;          // 相邻块最小间隔，默认 10
  max_daily_minutes: number;          // 单日上限，默认 480
  send_notes: boolean;                // 是否把备注一并发给模型，默认 true（用户已确认）
};
```

**服务商预设对照表**（设置页做一个下拉，选中即自动填下面三列，用户仍可手改）：

| 预设 | `base_url` | 默认 `model` | `structured_output_mode` |
| --- | --- | --- | --- |
| `deepseek`（默认） | `https://api.deepseek.com` | `deepseek-v4-flash` | `json_object` |
| `openai` | `https://api.openai.com/v1` | `gpt-4o-mini` | `json_schema` |
| `custom` | 用户填写 | 用户填写 | `auto` |

> 注意 **`deepseek-chat` 与 `deepseek-reasoner` 已于 2026/07/24 弃用**，不要作为默认值或写进文档示例。当前可用模型为 `deepseek-v4-flash`（非思考模式）与 `deepseek-v4-pro`。模型名一律由 `GET /models` 拉取后让用户选，避免文档与线上脱节。

> ⚠️ **已按 §12.3 修正**：排期单位是**队列条目**（`today_plan_items`），
> 因此 `task_ids` → `queue_item_ids`、`AiUnscheduledEntry.task_id` → `item_id`、
> `AiPlanWarning.task_id` → `queue_item_id`。下方代码块保留原始形态便于对照历史，**以实际源码为准**。

export type AiPlanRequest = {
  target_date: string;
  horizon_days: number;                 // 1..7
  queue_item_ids: number[] | null;      // null = 当天队列全部条目；值是 today_plan_items.id
  category_keys: string[] | null;       // null = 全部分类
  respect_priority: boolean;
  keep_locked_blocks: boolean;          // 保留 ai_locked 与手动块
  extra_instruction: string | null;     // 自然语言补充（≤200 字）
};

export type AiPlanItem = {
  id: string;                        // 前端稳定 key，后端生成
  source_task_id: number | null;      // 队列条目的清单来源；手动条目为 null
  source_today_item_id: number | null; // 队列条目 id（today_plan_items.id）
  schedule_date: string;
  start_minute: number;
  end_minute: number;
  title: string;                     // 后端回填
  category_key: string;              // 后端回填
  subject_id: number | null;         // 后端回填
  priority: AiPriority;              // 后端回填
  rationale: string | null;          // 模型给的排期理由，≤40 字
  manually_adjusted: boolean;        // 用户拖动过
  conflict_with: number[];           // 与哪些已有 schedule_block 冲突
};

export type AiPlanWarningCode =
  | 'no_window' | 'over_capacity' | 'due_risk' | 'conflict'
  | 'unknown_task' | 'schema_repaired' | 'truncated' | 'snapshot_drift' | 'duplicate';

export type AiPlanWarning = {
  code: AiPlanWarningCode;
  message: string;
  queue_item_id: number | null;       // today_plan_items.id，**不是**清单任务 id
  item_id: string | null;             // 指向 AiPlanItem.id
};

export type AiPlanProposal = {
  id: number;
  target_date: string;
  horizon_days: number;
  status: 'draft' | 'applied' | 'discarded' | 'expired';
  scope: 'day' | 'range' | 'window';
  scope_window_start: number | null;
  scope_window_end: number | null;
  engine: 'llm' | 'local_heuristic';
  degraded: boolean;
  model: string;
  created_at: string;
  items: AiPlanItem[];
  warnings: AiPlanWarning[];
  stats: {
    scheduled_count: number;
    unscheduled_count: number;
    total_minutes: number;
    overflow_minutes: number;
  };
};

export type AiSchedulerError = {
  code: AiSchedulerErrorCode;
  message: string;                  // 用户可读中文
  retryable: boolean;
  retry_after_seconds: number | null;
  degraded: { kind: 'local_heuristic' | 'none'; description: string } | null;
};

export type AiSchedulerErrorCode =
  | 'missing_api_key' | 'unauthorized' | 'forbidden' | 'rate_limited'
  | 'quota_exceeded' | 'timeout' | 'network' | 'server_error'
  | 'bad_request' | 'invalid_response' | 'conflict' | 'not_found' | 'db_error';

export type ScheduleChangeEvent = {
  kind:
    | 'task_created' | 'task_updated' | 'task_deleted' | 'task_completed'
    | 'due_changed' | 'block_moved' | 'block_deleted' | 'block_created';
  occurrence_date: string;
  task_id: number | null;
  block_id: number | null;
  delta_minutes: number | null;
};

export type AiApplyOptions = {
  overwrite_conflicts: boolean;  // 是否覆盖冲突块，默认 false
  skip_locked: boolean;          // 是否跳过 ai_locked 块，默认 true
};

export type AiApplyResult = {
  status: 'applied' | 'partial' | 'failed';
  created_count: number;
  skipped_count: number;
  conflicted_count: number;
  message: string;
  created_block_ids: number[];
};
```

### 3.3 后端结构体 `src-tauri/src/commands/ai_scheduler/models.rs`（新增）

与前端类型一一对应（`serde` 重命名策略沿用现有 `rename_all = "snake_case"` 风格），另有内部结构：

```rust
// 上下文快照：既喂给模型，也用于 apply 阶段的漂移检测
#[derive(Serialize, Deserialize)]
pub struct PlanContext {
    pub generated_at: String,
    pub horizon_days: i64,
    pub horizon_start: String,             // horizon 的起止日，prompt 与漂移检测都要
    pub horizon_end: String,
    pub queue_items: Vec<ContextQueueItem>, // 别名兼容旧快照里的 `tasks`
    pub existing_blocks: Vec<ContextBlock>,
    pub available_windows: Vec<AiTimeWindow>,
    pub peak_windows: Vec<AiTimeWindow>,
    pub min_break_minutes: i64,
    pub max_daily_minutes: i64,
    pub default_block_minutes: i64,
}

/// 排期单位是**队列条目**，不是清单任务（见 §12.3）。
#[derive(Serialize, Deserialize, Clone)]
pub struct ContextQueueItem {
    pub item_id: i64,                 // today_plan_items.id —— 模型唯一可引用的 id
    pub source_task_id: Option<i64>,  // 手动加进队列的条目为 None
    pub title: String,
    pub category_key: String,      // 取值域：politics|english|math|major|general
    pub category_label: String,    // 显示名，从 settings 键 checklist_category_names 解析后填入，供 prompt 使用
    pub subject_id: Option<i64>,
    pub priority: String,
    pub estimated_minutes: i64,
    pub due_date: Option<String>,
    pub note: Option<String>,      // 仅 send_notes = true 时填充
}

impl ContextQueueItem {
    /// 实际占用时长：未估时回落为 default_block_minutes.max(5)。
    pub fn effective_minutes(&self, default_block_minutes: i64) -> i64 { /* … */ }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContextBlock {
    pub block_id: i64,
    pub date: String,
    pub start_minute: i64,
    pub end_minute: i64,
    pub title: String,
    pub locked: bool,
}
```

> `category_key` 通过 `map_board_scope_to_category_key` 从 `board_scope` 得到，**不是**从 `checklist_columns` 取。`category_label` 通过 `load_category_names`（`checklist.rs:840` 附近）解析 `checklist_category_names` 设置得到；发给模型的用 label（可读），落库与前端匹配用 key。

#[derive(Serialize, Deserialize, Clone)]
pub struct ContextBlock {
    pub block_id: i64,
    pub date: String,
    pub start_minute: i64,
    pub end_minute: i64,
    pub title: String,
    pub locked: bool,
}
```

---

## 4. 接口设计（Tauri 命令）

全部注册到 `src-tauri/src/lib.rs` 的 `generate_handler!`。

### 4.0 两条必须遵守的实现约束

**约束 A：凡涉及网络的命令一律写成 `async fn` + `spawn_blocking`。**

这是项目既有模式，见 `commands/caldav.rs:148-162`、`commands/feishu/sync.rs:49`、`commands/sync/object_storage.rs:55`，共 7 处范例：

```rust
#[tauri::command]
pub async fn preview_ai_schedule(
    app: AppHandle,
    request: AiPlanRequest,
) -> Result<AiPlanProposal, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // 同步 blocking reqwest 调用放这里
        run_preview(app, request)
    })
    .await
    .map_err(|error| format!("AI 排期后台任务失败：{error}"))?
}
```

> 若误写成同步 `fn`，Tauri 会在主线程执行它。`timeout_seconds = 60` + `max_retries = 2` 最坏会让界面冻结 180 秒。

**约束 B：错误用 `Result<T, String>` 返回，字符串内容是 JSON 信封。**

原因：`src/services/tauriInvoke.ts` 的 `normalizeTauriError` 对非 `Error` 对象会退化成 `String(reason)`，即 `"[object Object]"`。若直接返回结构化错误对象，任何走通用错误提示的路径都会显示乱码。因此保持项目统一签名，把结构化信息序列化进字符串：

```rust
fn encode_error(err: AiSchedulerError) -> String {
    serde_json::to_string(&err).unwrap_or_else(|_| r#"{"code":"db_error","message":"未知错误","retryable":false}"#.to_string())
}
```

前端在 `aiSchedulerApi.ts` 里统一解码，**不直接复用 `invokeCommand`**：

```ts
async function invokeAi<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invokeCommand<T>(command, args);
  } catch (raw) {
    throw parseAiSchedulerError(raw);   // 解析 JSON 信封，兜底成 { code: 'network', ... }
  }
}
```

### 4.1 命令清单

| # | 命令 | 入参 | 出参 | 说明 |
| --- | --- | --- | --- | --- |
| 1 | `get_ai_scheduler_settings` | — | `AiSchedulerSettings` | 读取配置；`api_key` 恒为空串 |
| 2 | `save_ai_scheduler_settings` | `settings: AiSchedulerSettings` | `AiSchedulerSettings` | 密钥走 DPAPI，传空串表示不改 |
| 3 | `test_ai_scheduler_connection` | — | `{ ok, model, latency_ms, message }` | 发一条最小请求验证 key / base_url / model |
| 4 | `preview_ai_schedule` | `request: AiPlanRequest` | `AiPlanProposal` | 生成草案，**不写日程** |
| 5 | `update_ai_plan_proposal_items` | `proposalId, items: AiPlanItem[]` | `AiPlanProposal` | 保存用户手动微调结果 |
| 6 | `regenerate_ai_plan_proposal` | `proposalId, feedback: string \| null` | `AiPlanProposal` | 带反馈重新生成（复用原请求参数） |
| 7 | `apply_ai_plan_proposal` | `proposalId, options: AiApplyOptions` | `AiApplyResult` | 二次校验 + 事务写入 |
| 8 | `discard_ai_plan_proposal` | `proposalId` | `void` | 丢弃草案 |
| 9 | `get_latest_ai_plan_proposal` | `targetDate: string` | `AiPlanProposal \| null` | 打开抽屉时恢复未完成草案 |
| 10 | `replan_ai_schedule_after_change` | `change: ScheduleChangeEvent` | `AiPlanProposal` | 一键重排受影响时段，同样只产出草案 |

### 4.2 关键接口细节

> ⚠️ **本节已按 §12.3 修正**：排期来源不再是「清单里所有未完成且未钉住的任务」，
> 而是 **`target_date` 当天的计划队列**（`today_plan_items`，`completed = 0`）。
> 下面伪代码里的「任务」请一律读作「队列条目」；`task_ids` 已更名 `queue_item_ids`，
> 其值是 `today_plan_items.id`。`ai_pinned` 不再作为过滤条件。

**`preview_ai_schedule` 的服务端流程**

```
1. 读设置 → enabled 校验 → api_key_configured 校验（否则 missing_api_key）
2. context.rs::build_context(request)  → PlanContext
   · 条目筛选：当日队列 completed = 0（若 queue_item_ids 非空则取交集）
   · 已有日程：horizon 范围内全部 blocks；ai_locked / 手动块标记 locked = true
   · 时段：按 weekday 展开到每个具体日期
3. 若 keep_locked_blocks：把 locked 块从可占用空档中扣除，作为硬约束写入 prompt
4. prompt.rs::build(request, &context) → (system, user)
5. client.rs::chat_json(system, user, RESPONSE_SCHEMA)
      └ 失败且可重试 → 退避重试；不可重试或重试用尽 → planner::plan_locally() 降级
6. validator.rs::validate(raw, &context) → (Vec<AiPlanItem>, Vec<AiPlanWarning>)
7. planner.rs::persist_proposal(...)  → 写入 ai_plan_proposals（status = draft）
8. 返回 AiPlanProposal（含 stats）
```

**`apply_ai_plan_proposal` 的服务端流程**

```
1. 载入草案，status 必须是 draft，否则 not_found
2. 读取当前库状态与草案做 diff（读当前库，不用快照）：
   · 队列条目已移出队列 / 已勾选完成 → 丢弃该条目，warning: snapshot_drift
   · 条目 priority / estimated_minutes / due_date 变了 → 保留但记 warning
3. 逐条硬校验：
   · start_minute < end_minute，且落在当日可用时段内（可用时段按**库里当前设置**重读）
   · 与现有 blocks 重叠 → 若 overwrite_conflicts = false 则丢弃该条并记 conflict
   · 与草案内其它条目重叠 → 保留更早的一条，其余记 conflict
4. 事务写入 schedule_blocks：
   · schedule_date / start_minute / end_minute / title / category_key / subject_id
   · source_today_item_id = 草案条目的 source_today_item_id（指向那条已存在的队列条目）
   · source_proposal_id = 草案 id，ai_locked = 0，status = 'planned'
5. 草案 status = applied；返回 AiApplyResult（含逐条跳过原因）
6. trigger_shared_sync(app, "ai_schedule_apply")
```

**`replan_ai_schedule_after_change` 的窗口算法**

```
window = [delta 事件所在块/任务的 start - LOOKBACK, end + LOOKAHEAD]
         LOOKBACK = LOOKAHEAD = 120 分钟（常量，可配置）

scope = 'window'
可重排集合 = 与 window 相交的：今日未完成的 today_plan_items / 清单任务
硬约束集合 = window 之外的当天全部 blocks + window 内的 ai_locked 块
→ 走与 preview 相同的第 3~8 步，产出 status = draft 的新草案
→ 用户确认后才 apply（apply 时只替换 window 内、且 source_proposal_id 非空的旧块）
```

---

## 5. Prompt 与 Response Schema 设计

### 5.1 System Prompt（要点）

```text
你是考研学习日程规划器。唯一任务：把给定的待办任务分配到给定的可用时间窗内。

硬约束（违反即失败）：
1. 只能使用输入 queue_items 中出现的 item_id，不得编造、不得改写标题。
2. date 必须落在 horizon 内，且属于该日期 available_windows 的覆盖范围。
3. 同一 date 内任意两条 item 不得重叠，相邻两条间隔 ≥ min_break_minutes。
4. 不得占用 locked = true 的时间块。
5. 任务不得排到其 due_date 之后。

软目标（按优先级递减）：
1. priority = high 的任务优先放入 peak_windows。
2. due_date 更近的任务优先提前。
3. 同 category_key 的任务尽量连续，减少上下文切换。
4. 单日总时长不超过 max_daily_minutes。
5. 无法安排的任务放入 unscheduled 并给出 reason，绝不硬塞。

时间一律使用「距 00:00 的分钟数」整数。只输出 JSON，不要任何解释文字。
```

### 5.2 User Prompt（结构）

紧凑分段纯文本 + 一个 JSON 块，避免模型解析歧义：

```text
[horizon] 2026-09-25 .. 2026-09-27
[min_break_minutes] 10
[max_daily_minutes] 480
[default_block_minutes] 45

[available_windows]
2026-09-25: 08:00-12:00, 14:00-18:00, 19:00-22:30
2026-09-26: 08:00-12:00, 14:00-18:00
...

[peak_windows]
2026-09-25: 08:30-11:00

[existing_blocks]
2026-09-25: 10:00-12:00 英语真题精读 (locked)
2026-09-25: 19:00-20:30 政治网课

[queue_items]        （这一天的计划队列；只有这里的条目可以被排期）
{"item_id": 41, "title": "数学 660 题 第三章", "priority": "high",
 "estimated_minutes": 90, "due_date": "2026-09-26", "category_key": "数学"}
...
```

### 5.3 Response Schema

> 这份 schema 有两种投递方式：`json_schema` 档作为请求体的 `response_format`；`json_object` 档（DeepSeek 走这档）作为文本内嵌进提示词。见 §5.4。
>
> ⚠️ **已按 §12.3 修正**：`item_id` 指 `today_plan_items.id`（排期单位是队列条目）。

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["items", "unscheduled"],
  "properties": {
    "items": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["item_id", "date", "start_minute", "end_minute", "rationale"],
        "properties": {
          "item_id":      { "type": "integer" },
          "date":         { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
          "start_minute": { "type": "integer", "minimum": 0, "maximum": 1435 },
          "end_minute":   { "type": "integer", "minimum": 5, "maximum": 1440 },
          "rationale":    { "type": "string", "maxLength": 40 }
        }
      }
    },
    "unscheduled": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["item_id", "reason"],
        "properties": {
          "item_id": { "type": "integer" },
          "reason":  { "type": "string", "maxLength": 40 }
        }
      }
    }
  }
}
```

> 模型**不返回** title / category_key / subject_id。后端在 `validator.rs` 里用 `item_id` 查上下文回填，杜绝编造。

### 5.4 请求参数与结构化输出

#### 请求参数

排期是确定性任务，必须压低随机性并限制成本与延迟：

| 参数 | 值 | 理由 |
| --- | --- | --- |
| `temperature` | `0.2` | 同一输入应产出稳定结果，便于用户对比「重新生成」前后的差异 |
| `max_tokens` | `2048` | 约 40 条排期足够。**必须用 `max_tokens` 而不是 `max_completion_tokens`**——后者是 OpenAI 的私有新名，兼容网关普遍不认；DeepSeek 的合法范围是 1–8192 |
| `top_p` | `1` | 与低温搭配，不做额外截断 |
| `stream` | `false` | 排期是一次性结果，不需要流式；省掉 SSE 解析 |
| 思考模式 | **显式关闭** | 见下 |

**必须显式关闭思考模式。** DeepSeek V4 默认开启思考，而思考模式下：

- `temperature` / `top_p` 被忽略，等于低温约束失效，「重新生成」结果不可预期；
- 思考内容落在 `reasoning_content` 字段，最终答案在 `content`——**只解析 `choices[0].message.content`**，误读 `reasoning_content` 会拿到一段思考过程；
- 延迟与成本显著上升，对本场景收益为零。

关闭方式为请求体加 `"thinking": {"type": "disabled"}`。若某网关不认识该字段而被拒，则退化为「选非思考模型」（如 `deepseek-v4-flash` 的非思考模式）。

**解析时必须检查 `finish_reason`**：若为 `length`，说明输出被 `max_tokens` 截断，JSON 必然不完整 → 记 `truncated` warning，并触发提高 `max_tokens` 的一次重试，而不是直接判定模型输出错误。

#### 结构化输出的档位协商

各家兼容网关能力并不一致，**不能假设 `json_schema` 可用**：

| 服务商 | `json_object` | `json_schema`（strict） |
| --- | --- | --- |
| OpenAI | 支持 | 支持（约束解码，schema 有保证） |
| DeepSeek | 支持 | **不支持**（仅其 Responses API 的 `text.format` 有） |
| 通义 / 硅基流动等 | 支持 | 不支持 |

因此采用**探测 + 缓存 + 两档降级**：

1. `test_ai_scheduler_connection` 时探测一次：先试 `json_schema`，若返回 400 且错误指向 `response_format`，则判定为 `json_object` 档，**把结果写回 `structured_output_mode` 并缓存**。这样正式排期时不会再浪费一次失败的往返。
2. 排期时按缓存的档位发请求，不再试错。
3. 若探测结果为 `json_object`，System Prompt 尾部自动追加 §5.3 的 schema 文本 + 「必须输出符合上述 Schema 的 JSON」。

**`json_object` 档有两条硬性要求**（不满足会直接报错或拿到垃圾输出）：

- 提示词中**必须出现 "json" 字样**。§5.1 的 System Prompt 结尾已是「只输出 JSON，不要任何解释文字。」，满足此条件——但**若后续改写了这段提示词，必须保留 "json" 字样**。
- 提示词中**必须给出目标结构的示例**。§5.2 的 User Prompt 中已内嵌 schema 说明；`json_object` 档下把 §5.3 的完整 schema 一并附上。

> `json_object` 只保证「是合法 JSON」，不保证「符合你的字段结构」。所以 §5.5 校验器不是可选项——它是 `json_object` 档下唯一的结构保障。这也是本方案坚持「AI 只回 `item_id` + 时间、字段由后端回填」的另一个原因：即便模型把字段名写错，也只影响 `item_id` 和时间这两个极难写错的值。

### 5.5 校验器规则（`validator.rs`）

| 检查 | 不通过处理 |
| --- | --- |
| `item_id` 不在上下文中 | 丢弃 + warning `unknown_task` |
| 不在可用时段内 / 越界 | 丢弃 + warning `no_window` |
| `start >= end` 或非 5 分钟对齐 | 尝试按 `default_block_minutes` 修正，失败则丢弃 |
| 与其它草案条目重叠 | 保留双方，标注 `conflict`，前端高亮 |
| 与已有日程重叠 | 填 `conflict_with`，默认不写入 |
| 排到 `due_date` 之后 | 保留 + warning `due_risk` |
| 单日超 `max_daily_minutes` | 保留 + warning `over_capacity` |
| 有效条目数 = 0 且存在任务 | 判定 `invalid_response`，触发一次修复重试 |

---

## 6. 错误码、降级与重试

### 6.1 错误分类与处置

| code | 触发 | 可重试 | 前端提示 | 降级 |
| --- | --- | --- | --- | --- |
| `missing_api_key` | 未配置密钥 | 否 | 「请先在 设置 → 集成 → AI 排期 中填写 API Key」+ 跳转按钮 | 提示可先用本地启发式 |
| `unauthorized` (401) | key 无效 / 过期 | 否 | 「API Key 无效，请重新填写」 | 本地启发式 |
| `forbidden` (403) | 模型无权限 | 否 | 「当前 Key 无权访问该模型，请更换 model」 | 本地启发式 |
| `bad_request` (400) | base_url / model 写错 | 否 | 「请求被拒绝，请检查接口地址与模型名」 | 本地启发式 |
| `bad_request` 且指向 `response_format` | 该服务商不支持当前结构化输出档位 | 否（静默处理） | **不提示用户**：由 §5.4 的档位协商自动降级并回写缓存 | 无（改写档位后重试） |
| 响应 `finish_reason = "length"` | 输出被 `max_tokens` 截断 | 是（**提高 token 上限重试 1 次**） | 「模型输出过长被截断，正在重试」 | 本地启发式 |
| `rate_limited` (429) | 限流 | 是（读 `Retry-After`） | 「请求过于频繁，{n}s 后自动重试」 | 重试耗尽后本地启发式 |
| `quota_exceeded` | 余额 / 配额 | 否 | 「账户额度不足，请检查 OpenAI 账单」 | 本地启发式 |
| `timeout` | 超过 `timeout_seconds` | 是 | 「请求超时，正在重试（{i}/{n}）」 | 同上 |
| `network` | DNS / TLS / 连接失败 | 是 | 「网络不可用，已重试 {n} 次」+ 检查代理 | 本地启发式 |
| `server_error` (5xx) | 服务端异常 | 是 | 「OpenAI 服务异常，稍后重试」 | 同上 |
| `invalid_response` | schema 校验后有效条目为 0 | 是（**修复重试 1 次**） | 「模型返回内容不合法，已重试修复」 | 本地启发式 |
| `conflict` | 写入时全部条目冲突 | 否 | 「所有条目与现有日程冲突，请在预览中调整」 | 无（留在草案） |
| `db_error` | SQLite 异常 | 否 | 「本地数据写入失败：{detail}」 | 无 |

### 6.2 重试策略

- 退避：第 1 次重试等 1s，第 2 次等 3s（线性），总次数 = `max_retries`（默认 2）
- 429 优先使用响应头 `Retry-After`，无则用退避值
- `invalid_response` 的**修复重试**：把校验失败原因回灌为一条 user 消息，要求模型重新输出，仅执行 1 次
- 超时设置：`Client::builder().connect_timeout(10s).timeout(timeout_seconds)`

### 6.3 降级路径（核心）

```rust
// client.rs 失败后
match planner::plan_locally(&context, &request) {
    Ok(items) => {
        proposal.engine = "local_heuristic";
        proposal.degraded = true;
        proposal.warnings.push(AiPlanWarning {
            code: "schema_repaired",  // 复用为降级标记
            message: "AI 暂不可用，已改用本地启发式排期（未考虑语义偏好）".into(),
            ..
        });
        Ok(proposal)   // 仍然返回可用草案
    }
    Err(e) => Err(e),  // 本地也排不出来才真正失败
}
```

**本地启发式算法**（`planner::plan_locally`，纯确定性，可离线单测）：

1. 排序键：`priority` 降序 → `due_date` 升序 → `estimated_minutes` 降序
2. 按日期顺序、每日内 peak 窗口优先，扫描可用空档列表
3. 首个能容纳的连续空档即放置；放不下则跳到下一日
4. 全部日期放不下 → 进入 `unscheduled` + warning `no_window`
5. 维护「相邻最小间隔」与「单日容量」两个累计约束

这条路径保证了：**API 挂了、断网、Key 过期，功能依然可用**，只是排期质量下降，并如实告知用户。

### 6.4 数据一致性与边界情况（原方案遗漏，需补）

| 项 | 问题 | 处置 |
| --- | --- | --- |
| `expired` 状态无实现 | 原方案在 schema 里定义了 `status = 'expired'`，但没有任何代码会设置它，等于死状态 | 应用启动时（`background_tasks.rs` 挂一处）把 `created_at` 超过 3 天且仍为 `draft` 的草案置为 `expired`，并顺手删除更早的记录；或者直接删掉 `expired` 这个取值 |
| 条目归属未校验 | `update_ai_plan_proposal_items` 接收前端回传的 `items`，若不校验 `id` 归属，可以借一个草案的 id 改写另一个草案 | 在 `validator.rs` 里校验每个 `item.id` 都存在于该 proposal 的 `items_json` 中，否则返回 `bad_request`；同时禁止改 `source_task_id`（只能改时间与增删） |
| apply 与 replan 并发 | 两个流程可同时基于旧快照生成草案，后写入者覆盖先写入者 | 加进程内互斥（`Mutex<HashSet<String>>` 按 `target_date` 加锁），或简单的「同一日期同时只允许一个 draft」规则：新建草案时把该日期旧的 draft 置 `discarded` |
| 未填预计耗时的任务 | 需求 1 要求读取「预计耗时」，但清单原本没有这个字段，用户不填就没有值 | 默认 `0` 视为未估，排期时取 `default_block_minutes`（默认 45）。**UI 必须让"未填"可见**（显示为灰色的「未估」而非「0 分钟」），否则用户会以为 AI 把任务排成了 0 分钟 |
| 草案里的任务被重复排入 | 模型可能对同一任务输出两条不重叠的时间 | 校验器按 `item_id` 分组，同一天内只保留第一条，其余记 `due_risk` 之外的专用 warning `duplicate` |

---

## 7. 安全与隐私

| 关注点 | 措施 |
| --- | --- |
| API Key 落盘 | `credential::set_secret` → Windows DPAPI（`CRYPTPROTECT_UI_FORBIDDEN`），密文前缀 `dpapi:v1:` |
| Key 回传前端 | **永不回传**。`get_ai_scheduler_settings` 只返回 `api_key_configured: bool`（对齐 `email.rs` 的 `password_configured`） |
| Key 出现在日志 | 同步运行记录 / 日志构建处显式排除；错误信息中不得拼接 Authorization 头 |
| 传输 | 强制 HTTPS（rustls）；`base_url` 若为 http，仅允许 `localhost` / `127.0.0.1` |
| 首页数据出境 | 首次启用时弹确认框，明确告知将发送：任务标题（+ 可选备注）、优先级、预计耗时、截止日、分类名、已有日程标题与时间段。**不上传**：专注记录、统计、复盘内容、白名单、账号凭据 |
| 备注控制 | `send_notes` 默认 **`true`**（用户已确认），备注会随任务一起发送给所选服务商。设置页必须保留可关闭的开关，并在开关旁写明「关闭后 AI 无法依据备注里的细节排期」；首次启用时在确认弹窗中明确列出「任务备注」属于出境字段 |
| 合规声明 | 同步更新 `SECURITY.md` 与 `README.md` 的隐私边界章节 |
| 同步污染 | 草案表 `ai_plan_proposals` **不进入** `sync_package` 导出集合（第 976 行附近的表清单不加），避免草稿跨设备漫游 |

### 7.1 服务商与网络可达性

`api.openai.com` 在中国大陆网络下**通常不可直连**，而 DeepSeek 的 `api.deepseek.com` 可直连。因此：

- **默认服务商为 DeepSeek**，开箱可用，不需要用户先解决网络问题。这也让 S2 的连通性闸门在默认配置下就能通过。
- OpenAI 作为并列预设保留。选它的用户需要自备网络条件（代理或中转网关），设置页在选择该预设时给出这句提示。
- **`custom` 预设是逃生通道**：任何兼容 `POST /chat/completions` 的中转 / 自建网关 / 国产模型服务都能填。设置页把「接口地址」放在与「API Key」同等显眼的位置。
- 协议层只依赖三件事：`POST {base_url}/chat/completions`、`Authorization: Bearer <key>`、标准 `choices[0].message.content`。任何满足这三点的服务都能接入。

**`test_ai_scheduler_connection` 是 S2 阶段的硬闸门**，一次性完成三件事：

1. 发一条最小请求验证 key / base_url / model 可用，返回模型名与往返延迟；
2. 探测 `structured_output_mode` 并回写缓存（见 §5.4）；
3. 调 `GET /models` 拉取该账号可见的模型列表，填进 `available_models` 供下拉选择——**避免用户手打模型名写错，也避免文档里的默认模型名过时**。

连通性不过就不进入 S3 之后的阶段，避免后续所有调试都被网络问题掩盖。

**错误表现必须可区分**：DNS 解析失败、TLS 握手失败、连接超时三种情况在 `network` 错误码下给出不同中文文案，帮用户判断是改 `base_url` 还是改代理设置。DeepSeek 的 401 与 OpenAI 的 401 文案可以统一为「API Key 无效或被拒绝」。

### 7.2 外部日历的写入冲突

`commands/caldav.rs:1026/1082` 与 `commands/feishu/links.rs:550/587` 都会**反向 INSERT / UPDATE `schedule_blocks`**（把远端日历的变更同步回本地）。这意味着 AI 排期面对的不是一张静止的表：

| 冲突场景 | 处理 |
| --- | --- |
| apply 后块被推送到远端，用户在手机日历上改了时间 | 下次拉取会改写本地块。`source_proposal_id` 仍保留，但时间已变 |
| `replan` 用的是草案生成时的快照，期间远端拉取改过块 | **`replan` 必须重新读一次当前 `schedule_blocks`**，不得复用 `source_snapshot` 里的块数据；快照只用于任务的漂移检测 |
| 远端拉回的块与 AI 新排的块重叠 | 走 `apply` 的二次校验，按 `overwrite_conflicts` 决定是否覆盖；默认不覆盖，留成冲突由用户在预览里处理 |
| 用户手动在日历页拖动的块（`ai_locked = 0` 但确实动过） | 无法与 AI 块区分。折中：用户拖动后由前端调一次 `update_schedule_block` 时顺带把 `ai_locked` 置 1 |

> 结论：`replan` 的「重读当前库 + 不用快照里的块」这条必须写进实现，否则会出现「按旧时间重排、覆盖掉刚同步来的变更」的数据丢失。

---

## 8. 需要改动的代码模块清单

### 8.1 新增文件

**后端 `src-tauri/src/commands/ai_scheduler/`**

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 模块装配；对外路径保持 `commands::ai_scheduler::xxx` |
| `models.rs` | 设置、请求、草案条目、上下文、错误类型 |
| `settings.rs` | 命令 1–3：读取 / 保存 / 连通性测试；密钥走 `credential` |
| `context.rs` | 收集任务、日程、时段，构造 `PlanContext` 快照 |
| `prompt.rs` | System / User Prompt 与 `RESPONSE_SCHEMA` 常量 |
| `client.rs` | `reqwest::blocking` 调用 Chat Completions；超时、退避重试、错误分类 |
| `validator.rs` | schema + 业务硬校验，回填 title / category / subject |
| `planner.rs` | 草案落库、本地启发式排期、stats 计算 |
| `apply.rs` | 命令 7：漂移检测 + 二次校验 + 事务写入 + 触发同步 |
| `replan.rs` | 命令 10：窗口计算与局部重排 |
| `tests.rs` | 校验器、本地启发式、重试分类的离线单测 |

**前端**

| 文件 | 职责 |
| --- | --- |
| `src/types/aiScheduler.ts` | 3.2 全部类型 |
| `src/services/aiSchedulerApi.ts` | 10 个命令的 `invokeCommand` 封装 + 错误对象解析 |
| `src/components/AiPlanDrawer.tsx` | 草案预览抽屉：时间轴、拖拽微调、警告区、确认/放弃/重新生成 |
| `src/components/AiPlanTimeline.tsx` | 时间轴渲染（复用日历页的刻度与配色） |
| `src/pages/settings/AiSchedulerPanel.tsx` | 设置面板：Key、base_url、model、可用时段、高效时段、容量参数。**自带状态与命令调用**，只接收 `expanded / locked / onToggle`（见 §12.1 #20） |

### 8.2 修改文件

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/storage/db.rs` | 加 8 列（`add_column_if_missing`，含 `today_plan_items` 两列与 `schedule_blocks.source_task_id`）+ 建 `ai_plan_proposals` 表与索引 |
| `src-tauri/src/lib.rs` | `generate_handler!` 登记 10 个新命令；`mod` 声明 |
| `src-tauri/src/commands/checklist.rs` | `create/update_checklist_task`、`create/update_today_plan_item` 的 Draft 增加 `priority` / `estimated_minutes`；页面数据返回新字段；`add_task_to_today_plan` 抽成可复用函数供 apply 调用 |
| `src-tauri/src/sync_package/models.rs` | `SharedChecklistTask` / `SharedTodayPlanItem` 增加 `priority: Option<String>`、`estimated_minutes: Option<i64>`；`SYNC_SCHEMA_VERSION` 升到 3 |
| `src-tauri/src/sync_package/export.rs` | `export_checklist_tasks`（244 行）填真实值；`DeletedPayload::from`（631 行）两处填 `None` |
| `src-tauri/src/sync_package/import.rs` / `identity.rs` | 4 处 INSERT/UPDATE 带上新列；合并时遵循「`None` 不覆盖」语义 |
| `src-tauri/src/commands/feishu/links.rs` | 4 处 `checklist_tasks` / `today_plan_items` 写入带上新列 |
| `src-tauri/src/commands/schedule.rs` | `mark_conflicts` 提炼为可复用；块创建支持 `source_task_id` / `source_proposal_id` / `ai_locked`；`update_schedule_block` 在用户拖动时置 `ai_locked = 1`；4 处 `today_plan_items` 写入带新列 |
| `src/types/checklist.ts` | `ChecklistTask` / `ChecklistTaskDraft` / `TodayPlanItem` / `TodayPlanItemDraft` 增加 `priority`、`estimated_minutes`；`ChecklistTask` 另加 `ai_pinned` |
| `src/types/schedule.ts` | `ScheduleBlock` 增加 `source_task_id`、`source_proposal_id`、`ai_locked` |
| `src/services/checklistApi.ts` | Draft 透传新字段 |
| `src/pages/ChecklistPage.tsx` | 工具栏加「AI 排期」按钮；任务行**可编辑**优先级与预计耗时（不能只展示）；行菜单加「钉住」 |
| `src/pages/SchedulePage.tsx` | 顶部加「重新规划受影响的时段」；AI 来源块显示标记；移动/删除后触发 `replan_ai_schedule_after_change` |
| `src/pages/SettingsPage.tsx` | 仅在 `expandedPanels` 初始化中增加 `aiScheduler: false`（状态与回调由面板自持，见 §12.1 #20） |
| `src/pages/settings/IntegrationsPanel.tsx` | 增加 AI 排期分组入口（与飞书/邮件/CalDAV 并列） |
| `src/pages/settings/types.ts` | `SettingsPanelKey` 增加 `aiScheduler` |
| `src/components/ScheduleDrawer.tsx` | 展示 `ai_locked` 开关与 AI 来源提示 |
| `src/components/TodayPlanDrawer.tsx` | 任务编辑器增加「优先级」「预计耗时」录入（与清单页共用同一表单骨架） |
| `src/components.css` | 新增启用前「会发送 / 不会发送」两栏对照样式 `.confirm-dialog-columns` |
| `src/styles.css` | 新增抽屉与时间轴样式（优先复用现有 class） |
| `项目文件结构说明.md` | 同步新增文件与职责（项目规约要求） |
| `SECURITY.md` / `FEATURES.md` / `CHANGELOG.md` | 隐私边界、功能说明、变更记录 |

### 8.3 导航改动

**建议不新增顶级页面**，采用双入口 + 抽屉：

- 主入口：清单页工具栏「AI 排期」（对应「把清单排到日历」的核心诉求）
- 次入口：日历页「重新规划受影响的时段」
- 配置入口：设置 → 集成 → AI 排期

理由：`AppPage` 与 `pages` 需要同步改两处且 `Alt+N` 快捷键已用满（1–7），抽屉比新页面更贴合「一键、可预览、可撤销」的交互。

> 若你更希望有独立页面，只需在 `src/types/navigation.ts` 加 `'aiPlan'`、在 `src/navigation.tsx` 补 `pages.aiPlan`、并分配 `Alt+8`，其余模块可原样复用。

---

## 9. 实施步骤与验收

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| S1 数据层 + 同步链路 | db.rs 迁移（8 列 + 1 表）+ **`sync_package` 载荷 / 导出 / 导入 / 合并** + feishu 与 schedule 的 8 处写入点 + 前后端类型 + 清单页可编辑优先级与耗时 | `cargo test`；旧库升级后新列存在且默认值正确；**载荷往返单测**证明新字段可同步、且旧版本载荷不会清空已有值（`None` 不覆盖）；清单可录入并保存 |
| S2 设置与密钥 | `AiSchedulerPanel` + 命令 1–3 | 保存后重启应用，`api_key_configured` 为 true 且明文不回传；**连通性测试必须通过**（不通过则阻塞后续阶段） |
| S3 本地排期闭环 + 最小抽屉 | `context.rs` + `validator.rs` + `planner::plan_locally` + `apply.rs` + `AiPlanDrawer`**只读版**（可确认 / 可放弃，不可拖拽） | **完全不接 AI** 也能生成草案、人工预览、确认写入日历；`cargo test` 覆盖边角（空时段、超容量、截止日冲突、today_plan_items 无清单来源） |
| S4 接入 AI | `client.rs` + `prompt.rs` + 参数设定 + 两档结构化输出 + 降级与重试 | 断网 / 错 Key / 错 model / 不支持 strict 的网关四种情况均出现明确中文提示；前三种自动降级为本地启发式且 `degraded = true` |
| S5 微调与重生成 | `AiPlanDrawer` 加拖拽 + 命令 5、6、8、9 | 拖拽后 `manually_adjusted = true`；重生成后草案可对比；放弃后 `schedule_blocks` 无变化 |
| S6 重新规划 | `replan.rs` + 命令 10 + 日历页入口 | 改一个任务后，仅窗口内条目变化，窗口外块 id 完全不变；**先手动改一次远端日历并拉取，确认重排基于最新库而非快照** |
| S7 质量闸门 | `npm run typecheck` / `npm run build` / `cargo clippy` / `cargo test` | 全绿；更新结构说明、安全文档、`CHANGELOG.md` |

> **S3 之所以要带「最小只读抽屉」**：否则 S3 之后有「能写入日历的后端」但没有任何 UI 入口，验收标准无法人工执行，只能靠单测，风险会积压到 S5 才暴露。
>
> **S2 的连通性测试是硬闸门**：见 §7.1，`api.openai.com` 在国内通常不可直连。不先确认能通，S4 之后所有调试都会被网络问题掩盖。

**必须补的测试用例**（`tests.rs`）：

- 同一任务在同一日期被模型排了两次 → 校验器去重
- 模型返回 `item_id` 不存在 → `unknown_task`，不写入
- 排期跨越 `max_daily_minutes` → `over_capacity` warning 且仍可预览
- 降级路径：mock client 返回 503 → 走本地排期，产出非空草案
- 重试路径：mock 首次 429（`Retry-After: 2`）→ 第二次成功
- apply 漂移：草案生成后任务被删除 → `snapshot_drift` 且跳过该条

---

## 10. 风险与取舍

| 风险 | 取舍 |
| --- | --- |
| 模型输出不稳定 | 已用「只回索引 + 严格 json_schema + 本地校验器」三重兜底；非法条目丢弃而非报错 |
| Token 成本 | 只发送必要字段；`send_notes` 默认关闭；上下文用压缩文本而非完整 JSON 美化 |
| 隐私边界与项目「不上传用户隐私数据」原则的冲突 | 首次启用强确认 + 字段级最小化 + 文档显式声明；用户可随时关闭总开关 |
| 重排打乱用户手动安排 | `ai_locked` 列 + 窗口限制 + 全部走草案预览，三重保护 |
| 自建网关兼容性 | `base_url` 可配置，但仅支持 OpenAI Chat Completions 协议；非兼容协议需另写 adapter（本期不做） |
| 草案表被同步污染 | 明确不加入 `sync_package` 导出集合 |
| **API 直连不可用**（新增） | `api.openai.com` 国内通常不通。把自定义 `base_url` 做成主路径，并在 S2 设连通性硬闸门，避免网络问题掩盖后续所有调试 |
| **外部日历反向写入**（新增） | `caldav.rs` / `feishu/links.rs` 会改写 `schedule_blocks`，`replan` 必须重读当前库而非用快照，否则会覆盖刚同步来的远端变更 |
| **UI 冻结**（新增） | 网络命令必须走 `async fn` + `spawn_blocking`，否则长超时会卡死界面 |
| **错误信息乱码**（新增） | 结构化错误需序列化成 JSON 字符串返回，否则被前端 `normalizeTauriError` 降级成 `"[object Object]"` |

---

## 11. 待确认假设

1. ~~接口形态~~ **已确认**：采用 OpenAI 兼容的 `POST /chat/completions` 协议，默认服务商 DeepSeek（`https://api.deepseek.com`，模型 `deepseek-v4-flash`），其余服务商通过预设或 `custom` 接入。详见 §3.2 预设表与 §7.1。
2. ~~入口形态~~ **已确认**：采用「清单页 + 日历页双入口 + 抽屉」，不新增顶级导航页。见 §8.3。
3. ~~备注字段~~ **已确认**：`send_notes` 默认 **`true`**，备注会发送给所选服务商。见 §7 与 §3.2。

**三项假设已全部确认，方案可以进入实施。**

---

## 12. 审查修正记录

本文档经一次代码对照审查后修订，以下是原始方案的错误与缺口（保留记录便于回溯）：

| # | 等级 | 原方案问题 | 修正位置 |
| --- | --- | --- | --- |
| 1 | P0 | 命令全部写成同步 `fn`，与项目 `async fn` + `spawn_blocking` 的既有模式不符，最坏会让 UI 冻结 180 秒 | §4.0 约束 A |
| 2 | P0 | 错误类型直接用结构体，会被 `normalizeTauriError` 降级成 `"[object Object]"` | §4.0 约束 B |
| 3 | P0 | 「分类标签 = `checklist_columns`」映射错误，实际是 `board_scope` → 五值枚举 | §1 表、§3.3 |
| 4 | P0 | 只给 `checklist_tasks` 加列，漏了可独立创建的 `today_plan_items` | §3.1 |
| 5 | P0 | apply 后块无字段指回 `checklist_tasks`，勾选联动与反向重排都断链 | §3.1 链路完整性 |
| 6 | P1 | S3 交付「能写入日历」却没有 UI 入口，验收无法人工执行 | §9 S3 |
| 7 | P1 | 未把「`api.openai.com` 国内不可直连」当作主路径前提 | §7.1、§9 S2 |
| 8 | P1 | 未考虑 CalDAV / 飞书反向写入 `schedule_blocks`，`replan` 用快照会覆盖刚同步来的变更 | §7.2 |
| 9 | P1 | 未提 `strict` 结构化输出在兼容网关的兼容性 | §5.4 |
| 10 | P1 | 未设 `temperature` / `max_completion_tokens` | §5.4 |
| 11 | P1 | 清单页只「展示」优先级与耗时，用户无法录入 | §8.2 |
| 12 | P2 | `expired` 状态无实现路径；条目归属未校验；apply 与 replan 无互斥；未估时任务的默认值策略未定义 | §6.4 |
| 13 | P0 | 把 `json_schema`（strict）当首选档。**DeepSeek 的 Chat Completions 不支持它**，会直接 400 | §5.4 |
| 14 | P1 | 用 `max_completion_tokens`。这是 OpenAI 私有新名，兼容网关普遍不认，应改 `max_tokens` | §5.4 |
| 15 | P1 | 未处理思考模式：V4 默认开启思考，此时 `temperature` 失效、答案在 `content` 而非 `reasoning_content`，且延迟成本翻倍 | §5.4 |
| 16 | P1 | 未校验 `finish_reason`，截断产生的残缺 JSON 会被误判成模型输出错误 | §5.4 |
| 17 | P1 | 模型名会过期。`deepseek-chat` / `deepseek-reasoner` 已于 2026/07/24 弃用 | §3.2、§7.1 |
| 18 | **P0** | **把「给清单加字段」当成本地改动。实际 `checklist_tasks` / `today_plan_items` 是跨设备同步载荷（`sync_package/models.rs`），共 17 处写入点需连带修改；不改会导致优先级同步丢失且反复被清空** | §3.1.1、§8.2、§9 S1 |

### 12.1 实施期补充（S1 / S2 落地时发现）

| # | 等级 | 问题 | 处置 |
| --- | --- | --- | --- |
| 19 | P2 | 「首次启用时弹确认框」缺少可持久化的依据，否则每次开关都会弹 | `AiSchedulerSettings` 增 `privacy_acknowledged: bool`（`Default = false`），仅在 false→true 翻转时弹一次；见 §3.2、§7 |
| 20 | P2 | 若把 `AiSchedulerPanel` 的状态与回调按 §8.2 穿过 `SettingsPage` → `IntegrationsPanel`，需要再往后者（已收 36 个 props）追加十几个 props | 面板改为**自带状态与命令调用**的自洽组件，只接收 `expanded / locked / onToggle`；`SettingsPage` 仅需在 `expandedPanels` 初始化处加 `aiScheduler: false`。§8.2 对应行已据此调整 |
| 21 | P2 | §8.2 列出「`feishu/links.rs` 4 处、`schedule.rs` 4 处写入点需带新列」 | 实际核对后**无需修改**：`feishu/links.rs` 的 4 处是部分列 `INSERT`/`UPDATE`（新列走默认值，且远端改动不应覆盖本地 AI 排期属性）；`schedule.rs` 的 4 处全部位于 `#[cfg(test)] mod tests` 内。真正的生产写入点只有 `commands/checklist.rs`(6) 与 `sync_package/identity.rs`(4) |
| 22 | P3 | 「默认档位」与「预设档位」语义易混：`AiSchedulerSettings::default()` 的 `structured_output_mode` 是 `auto`，而 DeepSeek 预设给的是 `json_object` | 保持 `auto` 作为初值（未探测前不假设），由 S2 连通性测试探测后回写为确定值并缓存；预设表只用于用户切换服务商时填充表单 |
| 23 | **P0** | **密钥永不落盘（用户实测反馈：「填入密钥后仍提示未配置」）**。根因是取值顺序错位：`normalize_settings` 会把 `api_key` 清空以杜绝明文持久化，而保存逻辑却从 `normalized.api_key` 取明文，于是 `credential::set_secret` 永远收到空串 | 改为**在 `normalize_settings` 之前**取明文（`settings.api_key.trim()`），并把密钥落地抽成 `persist_api_key(connection, raw_api_key, now)` 以便直接单测。补 3 个回归测试：`save_persists_typed_api_key_and_reports_configured`（含 `dpapi:v1:` 密文前缀断言）、`blank_api_key_keeps_existing_secret`、`api_key_must_be_read_before_normalize_clears_it`。**教训**：`normalize_settings` 兼具「校验」与「脱敏」两职，凡从它的输出反取明文的写法都会静默退化为空值 |
| 24 | P2 | 「测试连接」只从凭据存储读密钥，不落盘当前表单 → 用户刚输入但未点「保存配置」的密钥直接触发 `missing_api_key`，表现为「填了密钥也没用」 | 前端 `handleTest` 改为**先保存再测试**（空密钥在后端等价于「不修改」，重复保存安全）；测试后的回读改用 `loadSettings({ silent: true })`，避免表单闪一次「正在读取…」 |
| 25 | P3 | §6.1 的提示文案「设置 → AI 排期」与实际导航不符：面板挂在 `集成` 页签下，用户按文案找不到入口 | 文案统一改为「设置 → 集成 → AI 排期」；`SettingsPage.tsx` 的「集成」页签描述由「飞书与邮件」改为「飞书、邮件与 AI 排期」 |

**审查中确认为正确、无需改动的部分**：`credential.rs` 的 DPAPI 复用、`add_column_if_missing(connection, table, column, definition)` 签名、`reqwest 0.13` 的 `blocking` + `rustls` feature、`mark_conflicts` 与 `trigger_shared_sync` 的复用、`ai_plan_proposals` 排除出 `sync_package` 导出集合、分钟制整数作为时间表示。

### 12.2 实施期补充（S3 落地时发现）

| # | 等级 | 问题 | 处置 |
| --- | --- | --- | --- |
| 26 | P2 | §3.2 的 `AiPlanStats` 只给了 `unscheduled_count` 计数，`source_snapshot` 里也没存明细 → 抽屉只能说「有 3 条没排上」，说不出**是哪 3 个任务、为什么**，用户无法判断该放宽时段还是该改截止日 | 新增 `UnscheduledEntry { task_id, title, reason }` 与 `ai_plan_proposals.unscheduled_json` 列（`add_column_if_missing` 幂等迁移，S1 旧库自动补齐）；`planner::resolve_unscheduled` 按 `task_id` 回填标题，前端不必再查一次清单 |
| 27 | P2 | §4.2 要求 apply 前做「可用时段二次硬校验」，但没说用**哪一份**设置。若用调用方传入的 `settings`，用户在「预览 → 确认」之间改过的时段就不会生效 | `apply_proposal` 内部重新 `settings::current_settings(connection)` 读**库里的当前配置**。**副作用（已固化为测试）**：单测必须把窗口真正 `persist_settings` 落盘，否则会退回默认的全天时段而一条都跳不过去 |
| 28 | P2 | §3.2 的 `AiApplyResult` 只有计数与 `message`，`prepare_items` 逐条产出的告警（哪条任务已删除、哪条不在时段内、跟哪条日程冲突）在写库后被整体丢弃 | `AiApplyResult` 追加 `warnings: Vec<AiPlanWarning>`（纯追加字段，旧前端忽略即兼容），抽屉在写入后直接把原因列给用户 |
| 29 | P2 | 漂移检测原本只比对 `priority`，而 `task_drift_state` 已经把 `due_date` / `estimated_minutes` 查出来了却不使用（触发了 dead_code） | 用上这两个字段：预计耗时按「有效时长」口径比对（未估时任务回落为 `default_block_minutes`，否则每个未估时任务都会误报），截止日已过则发 `due_risk`。两条都有独立测试 |
| 30 | P3 | `validator::horizon_bounds` 是 `context::build_context` 里两行逻辑的重复实现，且没有任何生产调用点（只有它自己的单测用），`-D warnings` 下会变成错误 | 移入 `context.rs` 并让 `build_context` 复用它，重复消失、无需 `#[allow(dead_code)]`；补一条「起始日非法时原样回退」的测试 |
| 31 | P3 | S3 自测数据本身写错了两处：**超容量**用例的两段可用时段合计恰好 480 分钟，等于当日容量上限，永远造不出溢出；**高效时段优先**用例没计入 `min_break_minutes`，空档被休整间隔压到 110 分钟，容不下 120 分钟的任务 | 修正测试数据而非放宽实现：前者把 `max_daily_minutes` 压到 300，后者改为两个都放得下、只靠「命中高效时段」区分先后的空档。**教训**：排期类断言必须先按 `min_break_minutes` 把占用区间外扩再算空档长度 |
| 32 | P3 | apply 阶段的 `warnings` 原本被 `message` 文案描述为「详见草案警告」，但草案警告是**生成期**的，与**写入期**的跳过原因不是同一批 | 文案改为「原因见下方提示」，并在抽屉里把 `result.warnings` 单独渲染成一个告警卡片，避免用户去翻已经过期的草案告警 |

### 12.3 排期来源改为「计划队列」（S3 验收后修正，影响面最大）

**背景**：S3 交付后实测发现，无论当天队列里有什么，AI 都会把五个分类里**所有**未完成任务拿去排。原因在 §4.2 第 1 步——它规定排期输入是 `checklist_tasks` 里 `completed = 0 AND ai_pinned = 0` 的全集，与「今日 / 计划队列」无关。这与用户预期不符：队列才是「今天要做这些」的表达。

因此把排期单位从**清单任务**改为**队列条目**（`today_plan_items`）。

| # | 等级 | 问题 | 处置 |
| --- | --- | --- | --- |
| 33 | P1 | 排期输入是全体未完成清单任务，用户无法通过队列控制「这次排什么」 | `checklist::list_schedulable_tasks` → **`list_queue_items(connection, today_date)`**：`WHERE today_date = ?1 AND completed = 0`。来源日期＝`request.target_date`（页面当前选中的日期）。手动加进队列的条目（`source_task_id IS NULL`）同样是队列成员，**一起排** |
| 34 | P1 | 排期单位一换，全链路的主键语义就变了：`AiPlanRequest.task_ids`、`RawPlanItem.task_id`、`UnscheduledEntry.task_id` 原本都是 `checklist_tasks.id`，继续沿用会拿队列 id 去查清单表 | 统一改名为 `queue_item_ids` / `item_id`（`today_plan_items.id`）。**连带**：`ContextTask` → **`ContextQueueItem`**（含 `item_id` + `Option` 的 `source_task_id`）、`PlanContext::task_by_id` → `item_by_id`、`AiPlanWarning.task_id` → **`queue_item_id`**（刻意不叫 `task_id`，避免有人误去清单表里查）。`PlanContext.tasks` → `queue_items` 加 `#[serde(default, alias = "tasks")]`，让库里已有的旧快照仍能解析 |
| 35 | P1 | §4.2 第 4 步的「apply 时若任务不在今日计划则补建一条」在队列即输入的模型下是多余的：草案本身就是队列条目的派生物 | `validator` 直接回填 `source_today_item_id = Some(candidate.item_id)`，`apply` 把它写进 `schedule_blocks`。**链路完整性变成天然成立**，删掉 `ensure_today_plan_item_for_task` 的调用与配套的 `ensure_sync_meta_for_local_id`（该 helper 本身仍被 `add_task_to_today_plan` 命令使用，未删）。回归测试 `block_links_to_existing_queue_item_without_creating_a_duplicate` 锁住「不得凭空补建第二条今日计划」 |
| 36 | P1 | 漂移检测读的是 `checklist_tasks`，但草案的依据已经是队列条目；用户把条目**移出队列**或勾选完成都不会被发现 | `TaskDriftState`/`task_drift_state` → **`QueueItemDriftState`/`queue_item_drift_state`**，改读 `today_plan_items`。检测面反而变宽：手动条目（无清单来源）过去完全不受漂移检测，现在也覆盖了 |
| 37 | P2 | `ai_pinned`（§4.2 第 1 步的排除条件）在新模型下会变成一层**用户看不见也改不了**的隐藏过滤——加进今天却没被排期将无法解释，而该字段目前没有 UI 入口 | **不再按 `ai_pinned` 过滤**，并写明这是对 §4.2 的有意修正：「在队列里」本身就是用户的显式选择。列与同步链路保持不动，`ai_pinned` 仍可作为普通清单属性使用 |
| 38 | P2 | 队列条目没有 `board_scope`，而它是分类的权威来源（`checklist_tasks.board_scope` → `map_board_scope_to_category_key`）；直接拿 `subject_id` 反查会在「通用板 + 指定科目」这类数据上给出不同答案 | 查询用 `LEFT JOIN checklist_tasks` 取 `board_scope`，回退顺序为 `board_scope` → `category_key_for_subject_id(subject_id)` → `general`。来源任务已被删除时自动走回退 |

### 12.4 S4 真模型排期落地时的修正（2026-09-26）

| # | 等级 | 问题 | 处置 |
| --- | --- | --- | --- |
| 39 | P1 | 原方案默认「AI 失败自动降级到本地启发式」，实测用户**不想要本地排期**：草案来源徽标长期显示「本地排期（未使用 AI）」，且降级是静默的 | `AiPlanRequest` 增加 `allow_local_fallback: bool`（`#[serde(default)]` = **false**）。默认路径 AI 失败就报错；只有用户在错误卡片里点「改用本地排期」才对**下一次生成**置 true，成功后前端立即复位。降级草案 `degraded = true`，徽标显示「本地兜底排期（AI 不可用时的降级结果）」 |
| 40 | P1 | 抽屉挂在页面组件内部，切换页面时随页面卸载被关闭，未确认草案在 UI 上「凭空消失」 | 抽屉提升为 **App 级单例**（`App.tsx` 持有，`src/services/aiPlanBus.ts` CustomEvent 总线沿用 `APP_NAVIGATE_EVENT` 约定）。清单页 / 日历页只渲染入口按钮（`openAiPlanDrawer(date, labels)`）并订阅 `AI_PLAN_APPLIED_EVENT` 刷新自身；分类显示名通过打开事件从清单页透传，日历页不传时用 Timeline 的兜底名 |
| 41 | P2 | `json_object` 档模型的返回可能带代码围栏、被 `finish_reason=length` 截断、或带尾逗号 | `client::repair_json_text` 三步修复（剥围栏 → 字符串感知补括号 → 去尾逗号），修复成功附 `WARN_SCHEMA_REPAIRED`；仍失败则 `ERR_INVALID_RESPONSE`（可重试）。重试用 600ms·2^n 退避（上限 8s），服务端 `Retry-After` 优先 |
| 42 | P2 | 模型可能编造清单里不存在的条目或改写标题 | 候选只允许携带 `item_id` + 时间；`title` / `category_key` / `subject_id` / `priority` 一律由 `validator` 按 id 从队列回填，`unknown_task` 告警并跳过 |
