//! 本地启发式排期器与草案落库。
//!
//! `plan_locally` 是纯确定性算法（无网络、无随机），因此可以离线单测。它同时是
//! **降级路径**：S4 起若 LLM 不可用则回落到这里，保证「API 挂了功能依然可用」（§6.3）。
//!
//! 产出的是 `RawPlanResponse`（只有队列条目 id 与时间），再交给 `validator` 统一校验 ——
//! 这样本地路径与 AI 路径的合法性判定完全一致，不存在两套规则。

use super::models::*;
use super::{client, context, prompt, settings, validator};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

// ── 本地启发式排期（方案 §6.3） ──

fn priority_rank(priority: &str) -> i64 {
    match priority {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        // 脏数据不应打乱顺序，按 medium 处理。
        _ => 1,
    }
}

/// 排序键：priority 降序 → due_date 升序 → estimated_minutes 降序 → item_id 兜底。
///
/// `respect_priority = false` 时忽略优先级，只按截止日与时长排（用户可能只关心 DDL）。
fn ordered_queue_items<'a>(
    plan_context: &'a PlanContext,
    respect_priority: bool,
) -> Vec<&'a ContextQueueItem> {
    let far_future = "9999-12-31".to_string();
    let mut keyed: Vec<(i64, String, i64, i64, &'a ContextQueueItem)> = plan_context
        .queue_items
        .iter()
        .map(|item| {
            let rank = if respect_priority {
                priority_rank(&item.priority)
            } else {
                0
            };
            let due = item.due_date.clone().unwrap_or_else(|| far_future.clone());
            (rank, due, -item.estimated_minutes, item.item_id, item)
        })
        .collect();
    keyed.sort_by(|a, b| (a.0, a.1.as_str(), a.2, a.3).cmp(&(b.0, b.1.as_str(), b.2, b.3)));
    keyed.into_iter().map(|(_, _, _, _, item)| item).collect()
}

fn adaptive_duration(item: &ContextQueueItem, plan_context: &PlanContext) -> i64 {
    let raw = item.effective_minutes(plan_context.default_block_minutes);
    // 用户填写的预计时长是明确意图，管家只替未估时条目补全时长。
    if !plan_context.planner_preferences.adaptive_durations || item.estimated_minutes > 0 {
        return raw;
    }
    let (min, max) = match item.category_key.as_str() {
        "math" | "major" => (45, 90),
        "english" | "politics" => (25, 60),
        _ => (25, 45),
    };
    (raw.clamp(min, max) / 5 * 5).max(5)
}

fn append_meal_items(items: &mut Vec<AiPlanItem>, plan_context: &PlanContext) {
    if !plan_context.planner_preferences.auto_meals {
        return;
    }
    let start = context::parse_date(&plan_context.horizon_start).ok();
    let Some(start) = start else {
        return;
    };
    for date in context::horizon_dates(start, plan_context.horizon_days) {
        let date_key = context::date_string(date);
        for meal in &plan_context.planner_preferences.meal_windows {
            if meal.end_minute <= meal.start_minute {
                continue;
            }
            let id = format!("{date_key}-meal-{}", meal.kind);
            if items.iter().any(|item| item.id == id) {
                continue;
            }
            let conflict_with = plan_context
                .existing_blocks
                .iter()
                .filter(|block| {
                    block.date == date_key
                        && block.start_minute < meal.end_minute
                        && meal.start_minute < block.end_minute
                })
                .map(|block| block.block_id)
                .collect();
            items.push(AiPlanItem {
                id,
                source_task_id: None,
                source_today_item_id: None,
                schedule_date: date_key.clone(),
                start_minute: meal.start_minute,
                end_minute: meal.end_minute,
                title: meal.kind.clone(),
                category_key: "general".to_string(),
                subject_id: None,
                priority: "low".to_string(),
                rationale: Some("固定生活安排，保证学习节奏".to_string()),
                manually_adjusted: false,
                conflict_with,
                kind: "meal".to_string(),
            });
        }
    }
    items.sort_by(|a, b| {
        (a.schedule_date.as_str(), a.start_minute, a.end_minute).cmp(&(
            b.schedule_date.as_str(),
            b.start_minute,
            b.end_minute,
        ))
    });
}

/// 把占用区间按最小间隔外扩：这样从「可用时段减去占用」得到的空档天然满足相邻间隔约束。
fn expand_by_break(ranges: &[(i64, i64)], min_break_minutes: i64) -> Vec<(i64, i64)> {
    ranges
        .iter()
        .map(|(start, end)| (start - min_break_minutes, end + min_break_minutes))
        .collect()
}

/// 从可用时段中挖掉占用区间，返回剩余空档。
fn subtract_ranges(windows: &[(i64, i64)], occupied: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut free: Vec<(i64, i64)> = windows.to_vec();
    for (occupied_start, occupied_end) in occupied {
        let mut next: Vec<(i64, i64)> = Vec::new();
        for (start, end) in free {
            // 完全不相交 → 原样保留。
            if *occupied_end <= start || *occupied_start >= end {
                next.push((start, end));
                continue;
            }
            if *occupied_start > start {
                next.push((start, *occupied_start));
            }
            if *occupied_end < end {
                next.push((*occupied_end, end));
            }
        }
        free = next;
    }
    free
}

/// 与高效时段相交的空档优先——这是本地启发式里唯一体现「偏好」的地方。
fn order_gaps_by_peak(gaps: Vec<(i64, i64)>, peaks: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut keyed: Vec<(bool, i64, (i64, i64))> = gaps
        .into_iter()
        .map(|gap| {
            let touches_peak = peaks
                .iter()
                .any(|(peak_start, peak_end)| gap.0 < *peak_end && *peak_start < gap.1);
            // `!touches_peak`：false（即命中高效时段）排在前面。
            (!touches_peak, gap.0, gap)
        })
        .collect();
    keyed.sort_by_key(|(not_peak, start, range)| (*not_peak, *start, *range));
    keyed.into_iter().map(|(_, _, range)| range).collect()
}

/// 生成排期候选。放不下的条目进 `unscheduled`，**绝不硬塞**。
///
/// 与 LLM 路径的差别：这里把「不得排到 due_date 之后」当作硬约束（方案 §5.1 第 5 条），
/// 因此本地路径不会产出 `due_risk`；该警告只在模型越界时由校验器给出。
pub fn plan_locally(
    plan_context: &PlanContext,
    request: &AiPlanRequest,
) -> Result<RawPlanResponse, AiSchedulerError> {
    let start = context::parse_date(&plan_context.horizon_start)?;
    let dates = context::horizon_dates(start, plan_context.horizon_days);

    let mut placed: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    let mut items: Vec<RawPlanItem> = Vec::new();
    let mut unscheduled: Vec<RawUnscheduledItem> = Vec::new();
    let break_minutes = match plan_context.planner_preferences.rest_style.as_str() {
        "gentle" => plan_context.min_break_minutes.max(15),
        "focused" => plan_context.min_break_minutes.min(5),
        _ => plan_context.min_break_minutes,
    };

    for queue_item in ordered_queue_items(plan_context, request.respect_priority) {
        let duration = adaptive_duration(queue_item, plan_context);
        let mut scheduled = false;

        for date in &dates {
            let date_key = context::date_string(*date);

            // 硬约束：不得排到截止日之后。日期是递增的，越界即可停止尝试。
            if let Some(due_date) = queue_item.due_date.as_deref() {
                if date_key.as_str() > due_date {
                    break;
                }
            }

            let windows = context::windows_for_date(&plan_context.available_windows, *date);
            if windows.is_empty() {
                continue;
            }

            // `keep_locked_blocks = false` 时，AI 产出的未锁定块视为可被顶替，不再占用空档。
            let mut existing: Vec<(i64, i64)> = plan_context
                .existing_blocks
                .iter()
                .filter(|block| block.date == date_key)
                .filter(|block| request.keep_locked_blocks || block.locked)
                .map(|block| (block.start_minute, block.end_minute))
                .collect();

            if plan_context.planner_preferences.auto_meals {
                existing.extend(
                    plan_context
                        .planner_preferences
                        .meal_windows
                        .iter()
                        .filter_map(|meal| {
                            (meal.end_minute > meal.start_minute)
                                .then_some((meal.start_minute, meal.end_minute))
                        }),
                );
            }

            let existing_total: i64 = existing.iter().map(|(s, e)| e - s).sum();
            let placed_ranges = placed.get(&date_key).cloned().unwrap_or_default();
            let placed_total: i64 = placed_ranges.iter().map(|(s, e)| e - s).sum();

            // 累计约束：单日容量。
            if existing_total + placed_total + duration > plan_context.max_daily_minutes {
                continue;
            }

            let mut occupied = existing;
            occupied.extend(placed_ranges);
            let occupied = expand_by_break(&context::merge_ranges(occupied), break_minutes);

            let peaks = context::peaks_for_date(&plan_context.peak_windows, *date);
            let gaps = subtract_ranges(&windows, &occupied);

            // 首个能容纳的连续空档即放置。
            if let Some((gap_start, gap_end)) = order_gaps_by_peak(gaps, &peaks)
                .into_iter()
                .find(|(gap_start, gap_end)| gap_end - gap_start >= duration)
            {
                debug_assert!(gap_start + duration <= gap_end);
                items.push(RawPlanItem {
                    item_id: queue_item.item_id,
                    date: date_key.clone(),
                    start_minute: gap_start,
                    end_minute: gap_start + duration,
                    // 本地路径给不出语义理由，诚实留空比编一个更好。
                    rationale: None,
                });
                placed
                    .entry(date_key)
                    .or_default()
                    .push((gap_start, gap_start + duration));
                scheduled = true;
                break;
            }
        }

        if !scheduled {
            unscheduled.push(RawUnscheduledItem {
                item_id: queue_item.item_id,
                reason: Some(if queue_item.due_date.is_some() {
                    "截止日前没有足够长的可用时段".to_string()
                } else {
                    "可用时段内放不下（可能受单日容量或已有日程限制）".to_string()
                }),
            });
        }
    }

    Ok(RawPlanResponse { items, unscheduled })
}

// ── 草案落库与读取 ──

fn db_error(error: rusqlite::Error) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, format!("草案读写失败：{error}"), false)
}

fn serialize_error(error: serde_json::Error) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, format!("草案序列化失败：{error}"), false)
}

/// 落库所需的全部信息。用一个结构体装起来，避免 `persist_proposal` 出现十个参数。
pub struct NewProposal<'a> {
    pub request: &'a AiPlanRequest,
    pub plan_context: &'a PlanContext,
    pub items: &'a [AiPlanItem],
    pub warnings: &'a [AiPlanWarning],
    /// 来自排期器与校验器的「排不下」声明。
    pub unscheduled: &'a [RawUnscheduledItem],
    pub engine: &'a str,
    pub degraded: bool,
    pub model: &'a str,
    pub scope: &'a str,
    pub window: Option<(i64, i64)>,
}

/// 把「排不下」的 item_id 补上标题，让前端不必再查一次队列。
fn resolve_unscheduled(
    entries: &[RawUnscheduledItem],
    plan_context: &PlanContext,
) -> Vec<UnscheduledEntry> {
    entries
        .iter()
        .map(|entry| UnscheduledEntry {
            item_id: entry.item_id,
            title: plan_context
                .item_by_id(entry.item_id)
                .map(|item| item.title.clone())
                .unwrap_or_else(|| format!("队列条目 {}", entry.item_id)),
            reason: entry
                .reason
                .clone()
                .unwrap_or_else(|| "未能排入可用时段".to_string()),
        })
        .collect()
}

fn compute_stats(
    items: &[AiPlanItem],
    unscheduled_count: i64,
    plan_context: &PlanContext,
) -> AiPlanStats {
    AiPlanStats {
        scheduled_count: items.len() as i64,
        unscheduled_count,
        total_minutes: items.iter().map(AiPlanItem::duration_minutes).sum(),
        overflow_minutes: validator::overflow_minutes(items, plan_context),
    }
}

fn parse_json_or_default<T: serde::de::DeserializeOwned + Default>(raw: &str) -> T {
    serde_json::from_str(raw).unwrap_or_default()
}

/// 写入草案。
///
/// 写入前把同日期旧的 `draft` 置为 `discarded`：同一日期同时只允许一个待确认草案，
/// 否则 apply 与 replan 会各自基于旧快照生成草案并互相覆盖（方案 §6.4）。
pub fn persist_proposal(
    connection: &Connection,
    new_proposal: NewProposal<'_>,
) -> Result<AiPlanProposal, AiSchedulerError> {
    let request = new_proposal.request;
    let plan_context = new_proposal.plan_context;
    let items = new_proposal.items;
    let warnings = new_proposal.warnings;
    let now = Utc::now().to_rfc3339();

    let unscheduled = resolve_unscheduled(new_proposal.unscheduled, plan_context);
    let stats = compute_stats(items, unscheduled.len() as i64, plan_context);

    let items_json = serde_json::to_string(items).map_err(serialize_error)?;
    let warnings_json = serde_json::to_string(warnings).map_err(serialize_error)?;
    let unscheduled_json = serde_json::to_string(&unscheduled).map_err(serialize_error)?;
    let snapshot_json = serde_json::to_string(plan_context).map_err(serialize_error)?;

    connection
        .execute(
            "
            UPDATE ai_plan_proposals
            SET status = ?1, updated_at = ?2
            WHERE proposal_date = ?3 AND status = ?4
            ",
            params![
                PROPOSAL_STATUS_DISCARDED,
                now,
                request.target_date,
                PROPOSAL_STATUS_DRAFT
            ],
        )
        .map_err(db_error)?;

    connection
        .execute(
            "
            INSERT INTO ai_plan_proposals (
              proposal_date, horizon_days, status, scope, scope_window_start, scope_window_end,
              engine, model, degraded, source_snapshot, items_json, warnings_json,
              unscheduled_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
            ",
            params![
                request.target_date,
                request.horizon_days,
                PROPOSAL_STATUS_DRAFT,
                new_proposal.scope,
                new_proposal.window.map(|window| window.0),
                new_proposal.window.map(|window| window.1),
                new_proposal.engine,
                new_proposal.model,
                i64::from(new_proposal.degraded),
                snapshot_json,
                items_json,
                warnings_json,
                unscheduled_json,
                now,
            ],
        )
        .map_err(db_error)?;

    Ok(AiPlanProposal {
        id: connection.last_insert_rowid(),
        target_date: request.target_date.clone(),
        horizon_days: request.horizon_days,
        status: PROPOSAL_STATUS_DRAFT.to_string(),
        scope: new_proposal.scope.to_string(),
        scope_window_start: new_proposal.window.map(|window| window.0),
        scope_window_end: new_proposal.window.map(|window| window.1),
        engine: new_proposal.engine.to_string(),
        degraded: new_proposal.degraded,
        model: new_proposal.model.to_string(),
        created_at: now,
        items: items.to_vec(),
        warnings: warnings.to_vec(),
        unscheduled,
        stats,
    })
}

const PROPOSAL_COLUMNS: &str = "
    id, proposal_date, horizon_days, status, scope, scope_window_start, scope_window_end,
    engine, model, degraded, source_snapshot, items_json, warnings_json, unscheduled_json,
    created_at
";

struct ProposalRow {
    id: i64,
    target_date: String,
    horizon_days: i64,
    status: String,
    scope: String,
    scope_window_start: Option<i64>,
    scope_window_end: Option<i64>,
    engine: String,
    model: String,
    degraded: bool,
    source_snapshot: String,
    items_json: String,
    warnings_json: String,
    unscheduled_json: String,
    created_at: String,
}

fn row_to_proposal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProposalRow> {
    Ok(ProposalRow {
        id: row.get(0)?,
        target_date: row.get(1)?,
        horizon_days: row.get(2)?,
        status: row.get(3)?,
        scope: row.get(4)?,
        scope_window_start: row.get(5)?,
        scope_window_end: row.get(6)?,
        engine: row.get(7)?,
        model: row.get(8)?,
        degraded: row.get::<_, i64>(9)? != 0,
        source_snapshot: row.get(10)?,
        items_json: row.get(11)?,
        warnings_json: row.get(12)?,
        unscheduled_json: row.get(13)?,
        created_at: row.get(14)?,
    })
}

/// 从库行还原草案。JSON 解析失败时退化为空集合而不是报错——草案内容损坏不该让
/// 抽屉整个打不开，用户至少还能「放弃」它。
fn finish_proposal(row: ProposalRow) -> AiPlanProposal {
    let items: Vec<AiPlanItem> = parse_json_or_default(&row.items_json);
    let warnings: Vec<AiPlanWarning> = parse_json_or_default(&row.warnings_json);
    let unscheduled: Vec<UnscheduledEntry> = parse_json_or_default(&row.unscheduled_json);
    let plan_context: PlanContext = serde_json::from_str(&row.source_snapshot)
        .unwrap_or_else(|_| empty_context(row.horizon_days, &row.target_date));

    let stats = compute_stats(&items, unscheduled.len() as i64, &plan_context);

    AiPlanProposal {
        id: row.id,
        target_date: row.target_date,
        horizon_days: row.horizon_days,
        status: row.status,
        scope: row.scope,
        scope_window_start: row.scope_window_start,
        scope_window_end: row.scope_window_end,
        engine: row.engine,
        degraded: row.degraded,
        model: row.model,
        created_at: row.created_at,
        items,
        warnings,
        unscheduled,
        stats,
    }
}

fn empty_context(horizon_days: i64, target_date: &str) -> PlanContext {
    PlanContext {
        generated_at: String::new(),
        horizon_days,
        horizon_start: target_date.to_string(),
        horizon_end: target_date.to_string(),
        queue_items: Vec::new(),
        existing_blocks: Vec::new(),
        available_windows: Vec::new(),
        peak_windows: Vec::new(),
        min_break_minutes: 0,
        max_daily_minutes: DEFAULT_MAX_DAILY_MINUTES,
        default_block_minutes: DEFAULT_BLOCK_MINUTES,
        planner_preferences: AiPlannerPreferences::default(),
    }
}

pub fn load_proposal(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Option<AiPlanProposal>, AiSchedulerError> {
    let sql = format!("SELECT {PROPOSAL_COLUMNS} FROM ai_plan_proposals WHERE id = ?1");
    let row = connection
        .query_row(&sql, params![proposal_id], row_to_proposal_row)
        .optional()
        .map_err(db_error)?;
    Ok(row.map(finish_proposal))
}

/// 读取快照。`apply` 阶段做漂移检测时用，因此保留原始结构类型。
///
/// S3 的 apply **刻意不用它**：CalDAV / 飞书会反向写回 `schedule_blocks`，
/// 基于快照判断冲突必然漏掉远端变更（§7.2 的教训），所以改为读当前库。
/// 保留此函数供 S6 的 `replan` 判断「原始 horizon 与范围」使用。
#[allow(dead_code)]
pub fn load_proposal_snapshot(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Option<PlanContext>, AiSchedulerError> {
    let raw = connection
        .query_row(
            "SELECT source_snapshot FROM ai_plan_proposals WHERE id = ?1",
            params![proposal_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)?;
    Ok(raw.and_then(|value| serde_json::from_str(&value).ok()))
}

/// 最新一个待确认草案。抽屉打开时用它恢复现场。
pub fn latest_draft_proposal(
    connection: &Connection,
    target_date: &str,
) -> Result<Option<AiPlanProposal>, AiSchedulerError> {
    let sql = format!(
        "SELECT {PROPOSAL_COLUMNS} FROM ai_plan_proposals
         WHERE proposal_date = ?1 AND status = ?2
         ORDER BY id DESC LIMIT 1"
    );
    let row = connection
        .query_row(
            &sql,
            params![target_date, PROPOSAL_STATUS_DRAFT],
            row_to_proposal_row,
        )
        .optional()
        .map_err(db_error)?;
    Ok(row.map(finish_proposal))
}

pub fn set_proposal_status(
    connection: &Connection,
    proposal_id: i64,
    status: &str,
) -> Result<bool, AiSchedulerError> {
    let now = Utc::now().to_rfc3339();
    let affected = connection
        .execute(
            "UPDATE ai_plan_proposals SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, proposal_id],
        )
        .map_err(db_error)?;
    Ok(affected > 0)
}

/// 清掉过期草案（方案 §6.4：原本 `expired` 是个没人设置过的死状态）。
///
/// 在生成新草案前顺手执行，避免额外引入启动钩子；超过 `PROPOSAL_EXPIRE_DAYS` 天的
/// 草稿已无参考价值。
pub fn expire_stale_proposals(connection: &Connection) -> Result<(), AiSchedulerError> {
    let cutoff = (Utc::now() - chrono::Duration::days(PROPOSAL_EXPIRE_DAYS)).to_rfc3339();
    connection
        .execute(
            "UPDATE ai_plan_proposals SET status = ?1 WHERE status = ?2 AND created_at < ?3",
            params![PROPOSAL_STATUS_EXPIRED, PROPOSAL_STATUS_DRAFT, cutoff],
        )
        .map_err(db_error)?;
    // 更早的记录连审计价值也没有了，直接删除避免表无限增长。
    connection
        .execute(
            "DELETE FROM ai_plan_proposals
             WHERE status IN (?1, ?2) AND created_at < ?3",
            params![
                PROPOSAL_STATUS_DISCARDED,
                PROPOSAL_STATUS_EXPIRED,
                (Utc::now() - chrono::Duration::days(PROPOSAL_EXPIRE_DAYS * 4)).to_rfc3339()
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

/// 预览编排（命令 4）：开关校验 → 队列预检 → **模型排期** → （仅显式要求时）本地兜底。
///
/// 降级语义（2026-09-26 按用户要求修正，偏离 §6.3）：默认**不**悄悄回落本地启发式——
/// 「排不出来」就该报错让用户重试，而不是拿一份没经过 AI 的结果冒充排期。
/// `request.allow_local_fallback = true`（前端错误卡片上的「改用本地排期」按钮）时才走本地，
/// 且草案会带上 `degraded = true`，前端明确标注这是兜底结果。
pub fn preview_proposal(
    connection: &Connection,
    request: AiPlanRequest,
    settings: &AiSchedulerSettings,
) -> Result<AiPlanProposal, AiSchedulerError> {
    if !settings.enabled {
        return Err(bad_request_message(
            "AI 排期尚未开启，请先在 设置 → 集成 → AI 排期 中打开",
        ));
    }
    if !settings.privacy_acknowledged {
        return Err(bad_request_message(
            "请先在 设置 → 集成 → AI 排期 中确认数据使用说明",
        ));
    }
    if !settings.api_key_configured {
        return Err(AiSchedulerError::new(
            ERR_MISSING_API_KEY,
            "请先在 设置 → 集成 → AI 排期 中填写 API Key",
            false,
        ));
    }
    if settings.base_url.is_empty() {
        return Err(bad_request_message(
            "请先在 设置 → 集成 → AI 排期 中填写接口地址（Base URL）",
        ));
    }
    if settings.model.is_empty() {
        return Err(bad_request_message(
            "请先在 设置 → 集成 → AI 排期 中选择或填写模型名",
        ));
    }

    expire_stale_proposals(connection)?;
    let request = context::normalize_plan_request(request)?;
    let plan_context = context::build_context(connection, &request, settings)?;

    // 队列里没有可排条目就不浪费一次网络往返——这不是排期失败，是没东西可排。
    if plan_context.queue_items.is_empty() {
        return Err(bad_request_message(format!(
            "「{}」的计划队列里没有未完成条目，先在「今日 / 计划」里加几条再生成草案",
            request.target_date
        )));
    }

    match llm_plan(connection, &request, settings, &plan_context) {
        Ok(proposal) => Ok(proposal),
        Err(llm_error) => {
            if !request.allow_local_fallback {
                return Err(llm_error);
            }
            let raw = plan_locally(&plan_context, &request)?;
            let outcome = validator::validate(&raw, &plan_context);
            let mut items = outcome.items;
            append_meal_items(&mut items, &plan_context);
            persist_proposal(
                connection,
                NewProposal {
                    request: &request,
                    plan_context: &plan_context,
                    items: &items,
                    warnings: &outcome.warnings,
                    unscheduled: &outcome.unscheduled,
                    engine: ENGINE_LOCAL_HEURISTIC,
                    degraded: true,
                    model: "",
                    scope: SCOPE_DAY,
                    window: None,
                },
            )
        }
    }
}

fn bad_request_message(message: impl Into<String>) -> AiSchedulerError {
    AiSchedulerError::new(ERR_BAD_REQUEST, message, false)
}

/// 模型路径：提示词 → Chat Completions → 同一个校验器 → 落库（engine = llm）。
fn llm_plan(
    connection: &Connection,
    request: &AiPlanRequest,
    settings: &AiSchedulerSettings,
    plan_context: &PlanContext,
) -> Result<AiPlanProposal, AiSchedulerError> {
    let base_url = client::normalize_base_url(&settings.base_url)?;
    let api_key = settings::load_api_key(connection)?;
    let http = client::build_client(settings.timeout_seconds)?;

    let system_prompt = prompt::build_system_prompt();
    let user_prompt = prompt::build_user_prompt(request, plan_context);
    let outcome = client::chat_json(
        &http,
        &base_url,
        &api_key,
        settings,
        &system_prompt,
        &user_prompt,
    )?;

    let validated = validator::validate(&outcome.response, plan_context);
    let mut warnings = validated.warnings;
    warnings.extend(outcome.warnings);
    let mut items = validated.items;
    append_meal_items(&mut items, plan_context);

    persist_proposal(
        connection,
        NewProposal {
            request,
            plan_context,
            items: &items,
            warnings: &warnings,
            unscheduled: &validated.unscheduled,
            engine: ENGINE_LLM,
            degraded: false,
            model: &settings.model,
            scope: SCOPE_DAY,
            window: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(weekday: i64, start: i64, end: i64) -> AiTimeWindow {
        AiTimeWindow {
            weekday,
            start_minute: start,
            end_minute: end,
        }
    }

    fn queue_item(
        item_id: i64,
        priority: &str,
        minutes: i64,
        due_date: Option<&str>,
    ) -> ContextQueueItem {
        ContextQueueItem {
            item_id,
            source_task_id: Some(item_id * 10),
            title: format!("条目{item_id}"),
            category_key: "math".to_string(),
            category_label: "数学".to_string(),
            subject_id: Some(3),
            priority: priority.to_string(),
            estimated_minutes: minutes,
            due_date: due_date.map(str::to_string),
            note: None,
        }
    }

    /// 周五（2026-09-25）08:00–12:00 可用，单日上限 480 分钟。
    fn base_context(items: Vec<ContextQueueItem>) -> PlanContext {
        PlanContext {
            generated_at: "2026-09-25T00:00:00Z".to_string(),
            horizon_days: 1,
            horizon_start: "2026-09-25".to_string(),
            horizon_end: "2026-09-25".to_string(),
            queue_items: items,
            existing_blocks: Vec::new(),
            available_windows: vec![window(5, 480, 720)],
            peak_windows: Vec::new(),
            min_break_minutes: 10,
            max_daily_minutes: 480,
            default_block_minutes: 45,
            planner_preferences: AiPlannerPreferences {
                auto_meals: false,
                ..AiPlannerPreferences::default()
            },
        }
    }

    fn request() -> AiPlanRequest {
        AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            ..AiPlanRequest::default()
        }
    }

    #[test]
    fn packs_tasks_back_to_back_respecting_min_break() {
        let plan_context = base_context(vec![
            queue_item(1, "high", 60, None),
            queue_item(2, "medium", 60, None),
        ]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items.len(), 2);
        assert_eq!(raw.items[0].start_minute, 480);
        assert_eq!(raw.items[0].end_minute, 540);
        // 最小间隔 10 分钟 → 第二条从 550 开始。
        assert_eq!(raw.items[1].start_minute, 550);
        assert_eq!(raw.items[1].end_minute, 610);
    }

    #[test]
    fn high_priority_is_placed_first() {
        let plan_context = base_context(vec![
            queue_item(1, "low", 60, None),
            queue_item(2, "high", 60, None),
        ]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items[0].item_id, 2, "high 应先占早时段");
    }

    #[test]
    fn nearer_due_date_wins_at_same_priority() {
        let plan_context = base_context(vec![
            queue_item(1, "high", 60, Some("2026-12-01")),
            queue_item(2, "high", 60, Some("2026-09-25")),
        ]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items[0].item_id, 2);
    }

    #[test]
    fn respect_priority_off_falls_back_to_due_date() {
        let plan_context = base_context(vec![
            queue_item(1, "low", 60, Some("2026-09-25")),
            queue_item(2, "high", 60, Some("2026-12-01")),
        ]);
        let raw = plan_locally(
            &plan_context,
            &AiPlanRequest {
                respect_priority: false,
                ..request()
            },
        )
        .expect("plan");

        assert_eq!(raw.items[0].item_id, 1, "关闭优先级后应按截止日排");
    }

    #[test]
    fn later_peak_window_is_used_when_earlier_gap_is_roomy() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 100, None)]);
        // 08:00–15:00 可用。
        plan_context.available_windows = vec![window(5, 480, 900)];
        // 已有安排 10:00–11:00（锁定）。
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 1,
            date: "2026-09-25".to_string(),
            start_minute: 600,
            end_minute: 660,
            title: "已有安排".to_string(),
            locked: true,
        }];
        // 高效时段在 13:00–15:00。
        plan_context.peak_windows = vec![window(5, 780, 900)];

        let raw = plan_locally(&plan_context, &request()).expect("plan");

        // 两个空档都放得下 100 分钟：(480,590) 与 (670,900)。
        // 按「先来先占」会落在 480，命中高效时段的那一段应当被优先。
        assert_eq!(raw.items.len(), 1);
        assert_eq!(
            raw.items[0].start_minute, 670,
            "应优先使用命中高效时段的空档"
        );
        // 670 = 已有安排结束的 660 + 最小间隔 10，说明休息间隔没有被跳过。
        assert!(raw.items[0].start_minute >= 660 + plan_context.min_break_minutes);
    }

    #[test]
    fn task_that_fits_nowhere_is_unscheduled_not_forced() {
        let plan_context = base_context(vec![queue_item(1, "high", 400, None)]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert!(raw.items.is_empty(), "放不下就不该硬塞");
        assert_eq!(raw.unscheduled.len(), 1);
        assert_eq!(raw.unscheduled[0].item_id, 1);
    }

    #[test]
    fn over_capacity_is_deferred_to_next_day() {
        let mut plan_context = base_context(vec![
            queue_item(1, "high", 300, None),
            queue_item(2, "high", 300, None),
        ]);
        plan_context.horizon_days = 2;
        plan_context.horizon_end = "2026-09-26".to_string();
        // 周五与周六都可排，但单日上限仅 480 → 两条应分两天。
        plan_context.available_windows = vec![window(5, 480, 1080), window(6, 480, 1080)];
        plan_context.max_daily_minutes = 480;

        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items.len(), 2);
        assert_eq!(raw.items[0].date, "2026-09-25");
        assert_eq!(raw.items[1].date, "2026-09-26");
    }

    #[test]
    fn due_date_is_a_hard_constraint() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 60, Some("2026-09-24"))]);
        plan_context.horizon_start = "2026-09-25".to_string();
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert!(
            raw.items.is_empty(),
            "截止日已过就不该排到之后的日子（硬约束，§5.1 第 5 条）"
        );
        assert_eq!(raw.unscheduled.len(), 1);
    }

    #[test]
    fn unestimated_task_uses_default_block_minutes() {
        let plan_context = base_context(vec![queue_item(1, "high", 0, None)]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items[0].end_minute - raw.items[0].start_minute, 45);
    }

    #[test]
    fn butler_mode_blocks_meals_and_adapts_unestimated_tasks() {
        let mut plan_context = base_context(vec![ContextQueueItem {
            item_id: 1,
            source_task_id: None,
            title: "英语阅读".to_string(),
            category_key: "english".to_string(),
            category_label: "英语".to_string(),
            subject_id: None,
            priority: "high".to_string(),
            estimated_minutes: 0,
            due_date: None,
            note: None,
        }]);
        plan_context.available_windows = vec![window(5, 480, 900)];
        plan_context.planner_preferences.auto_meals = true;
        let raw = plan_locally(&plan_context, &request()).expect("plan");
        assert_eq!(raw.items[0].start_minute, 480);
        assert_eq!(raw.items[0].end_minute - raw.items[0].start_minute, 45);
        assert!(raw
            .items
            .iter()
            .all(|item| item.end_minute <= 720 || item.start_minute >= 780));
    }

    #[test]
    fn locked_blocks_always_block_space() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 480, None)]);
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 9,
            date: "2026-09-25".to_string(),
            start_minute: 480,
            end_minute: 600,
            title: "锁定的块".to_string(),
            locked: true,
        }];

        let raw = plan_locally(&plan_context, &request()).expect("plan");
        // 剩 600–720 共 120 分钟，加最小间隔只有 110 可用，装不下 480。
        assert!(raw.items.is_empty());
    }

    #[test]
    fn unlocked_blocks_can_be_replaced_when_not_keeping_locked() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 60, None)]);
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 9,
            date: "2026-09-25".to_string(),
            start_minute: 480,
            end_minute: 600,
            title: "AI 上次排的".to_string(),
            locked: false,
        }];

        let raw = plan_locally(
            &plan_context,
            &AiPlanRequest {
                keep_locked_blocks: false,
                ..request()
            },
        )
        .expect("plan");

        assert_eq!(raw.items.len(), 1);
        assert_eq!(raw.items[0].start_minute, 480, "未锁定块可被顶替");
    }

    #[test]
    fn no_available_window_yields_unscheduled() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 60, None)]);
        // 周日不在可用时段里。
        plan_context.horizon_start = "2026-09-27".to_string();
        plan_context.horizon_end = "2026-09-27".to_string();

        let raw = plan_locally(&plan_context, &request()).expect("plan");
        assert!(raw.items.is_empty());
        assert_eq!(raw.unscheduled.len(), 1);
    }
}
