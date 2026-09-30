//! 排期编排与草案落库。
//!
//! 两条路径产出同一种 `RawPlanResponse`（只有队列条目 id 与时间）：
//! - 模型路径：提示词 → Chat Completions → `refine`（可行性修复：不可行的挪、漏排的补）；
//! - 本地路径：`refine` 在「模型什么都没给」时的特例，只在用户显式点「改用本地排期」时使用。
//!
//! 两者随后都交给 `validator` 统一校验，合法性判定只有一套规则。

use super::models::*;
use super::{client, context, prompt, refine, settings, slots, validator};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeSet, HashMap};

/// 三餐只加在有学习安排的日子，并跳过已经过去的、或与保留日程重叠的餐次。
///
/// 旧实现给 horizon 的每一天都加三餐且不看已有日程：重排同一天时，上次写进日历的
/// 「午餐」（ai_locked）会让新午餐在写入时报「与已锁定时段冲突」，每次都多一条噪音。
fn append_meal_items(items: &mut Vec<AiPlanItem>, plan_context: &PlanContext) {
    if !plan_context.planner_preferences.auto_meals {
        return;
    }
    let study_dates: BTreeSet<String> = items
        .iter()
        .filter(|item| item.kind != "meal")
        .map(|item| item.schedule_date.clone())
        .collect();
    for date_key in study_dates {
        let earliest = slots::earliest_start(plan_context.clock.as_ref(), &date_key);
        for meal in &plan_context.planner_preferences.meal_windows {
            if meal.end_minute <= meal.start_minute || meal.start_minute < earliest {
                continue;
            }
            let occupied = plan_context.existing_blocks.iter().any(|block| {
                block.date == date_key
                    && !block.replaceable
                    && slots::overlaps(
                        meal.start_minute,
                        meal.end_minute,
                        block.start_minute,
                        block.end_minute,
                    )
            });
            let id = format!("{date_key}-meal-{}", meal.kind);
            if occupied || items.iter().any(|item| item.id == id) {
                continue;
            }
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
                conflict_with: Vec::new(),
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

/// 本地规则排期：`refine` 在「模型什么都没给」时的特例。
///
/// 顺序：优先级 → 错过次数 → 截止日 → 重科目；高优先级与数学 / 专业课优先进高效时段；
/// 整段放不下就拆段；截止日前放不下才排到之后（校验器会给 `due_risk`）；
/// 实在放不下才进 `unscheduled`。每条都带规则化的理由。
pub fn plan_locally(
    plan_context: &PlanContext,
    request: &AiPlanRequest,
) -> Result<RawPlanResponse, AiSchedulerError> {
    context::parse_date(&plan_context.horizon_start)?;
    let outcome = refine::make_feasible(
        &RawPlanResponse::default(),
        plan_context,
        request.respect_priority,
        refine::Mode::Local,
    );
    let mut response = outcome.response;
    response.summary = Some(local_summary(&response));
    Ok(response)
}

/// 本地排期的一句话总结：只陈述结果，不假装有「思路」。
fn local_summary(response: &RawPlanResponse) -> String {
    let placed: BTreeSet<i64> = response.items.iter().map(|item| item.item_id).collect();
    let minutes: i64 = response
        .items
        .iter()
        .map(|item| item.end_minute - item.start_minute)
        .sum();
    let mut text = format!(
        "按优先级与截止日依次排入空档，共 {} 条、{}",
        placed.len(),
        duration_label(minutes)
    );
    if !response.unscheduled.is_empty() {
        text.push_str(&format!("；{} 条没排上", response.unscheduled.len()));
    }
    text
}

fn duration_label(minutes: i64) -> String {
    if minutes < 60 {
        return format!("{minutes} 分钟");
    }
    if minutes % 60 == 0 {
        format!("{} 小时", minutes / 60)
    } else {
        format!("{:.1} 小时", minutes as f64 / 60.0)
    }
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
    /// 模型或本地规则给出的一句话总结。
    pub summary: Option<&'a str>,
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

/// 草案统计。学习条目与三餐分开算：三餐不计入「排了几条」，也不计入每日目标。
fn compute_stats(
    items: &[AiPlanItem],
    warnings: &[AiPlanWarning],
    unscheduled_count: i64,
    plan_context: &PlanContext,
) -> AiPlanStats {
    let study: Vec<&AiPlanItem> = items.iter().filter(|item| item.kind != "meal").collect();
    // 拆段的条目只算一条。
    let distinct: BTreeSet<i64> = study
        .iter()
        .filter_map(|item| item.source_today_item_id)
        .collect();
    let unlinked = study
        .iter()
        .filter(|item| item.source_today_item_id.is_none())
        .count();
    let open_days = slots::build_day_frames(plan_context)
        .iter()
        .filter(|frame| !frame.past)
        .count() as i64;
    let daily_target = plan_context
        .planner_preferences
        .daily_target_minutes
        .min(plan_context.max_daily_minutes)
        .max(0);
    let adjusted: BTreeSet<&str> = warnings
        .iter()
        .filter(|warning| warning.code == WARN_ADJUSTED)
        .filter_map(|warning| warning.item_id.as_deref())
        .collect();

    AiPlanStats {
        scheduled_count: (distinct.len() + unlinked) as i64,
        unscheduled_count,
        total_minutes: items.iter().map(AiPlanItem::duration_minutes).sum(),
        overflow_minutes: validator::overflow_minutes(items, plan_context),
        study_minutes: study.iter().map(|item| item.duration_minutes()).sum(),
        target_minutes: daily_target * open_days,
        adjusted_count: adjusted.len() as i64,
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
    let stats = compute_stats(items, warnings, unscheduled.len() as i64, plan_context);

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
              unscheduled_json, created_at, updated_at, summary
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15)
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
                new_proposal.summary,
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
        summary: new_proposal.summary.map(str::to_string),
        already_scheduled: plan_context.already_scheduled.clone(),
        replaceable_block_count: plan_context.replaceable_block_ids.len() as i64,
    })
}

const PROPOSAL_COLUMNS: &str = "
    id, proposal_date, horizon_days, status, scope, scope_window_start, scope_window_end,
    engine, model, degraded, source_snapshot, items_json, warnings_json, unscheduled_json,
    created_at, summary
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
    summary: Option<String>,
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
        summary: row.get(15)?,
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

    let stats = compute_stats(&items, &warnings, unscheduled.len() as i64, &plan_context);

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
        summary: row.summary,
        already_scheduled: plan_context.already_scheduled.clone(),
        replaceable_block_count: plan_context.replaceable_block_ids.len() as i64,
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
        clock: None,
        already_scheduled: Vec::new(),
        replaceable_block_ids: Vec::new(),
        source_request: None,
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

/// 读取快照。
///
/// 用途只有两类「生成时的依据」：apply 的漂移检测（生成时的预计时长、要替换的旧块 id），
/// 以及「按反馈调整」复现同一组请求参数。**冲突判定不能用它**：CalDAV / 飞书会反向写回
/// `schedule_blocks`，基于快照判断冲突必然漏掉远端变更（§7.2 的教训），apply 读的是当前库。
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

fn bad_request_message(message: impl Into<String>) -> AiSchedulerError {
    AiSchedulerError::new(ERR_BAD_REQUEST, message, false)
}

fn not_found() -> AiSchedulerError {
    AiSchedulerError::new(ERR_NOT_FOUND, "草案不存在或已被清理，请重新生成", false)
}

/// 调模型前的开关 / 凭据检查。
fn ensure_ai_ready(settings: &AiSchedulerSettings) -> Result<(), AiSchedulerError> {
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
    Ok(())
}

/// 排期前的预检：没有可排条目、日期已过、或范围内已经没有空档时，不浪费一次网络往返，
/// 并且直接告诉用户卡在哪里。
fn ensure_schedulable(
    request: &AiPlanRequest,
    plan_context: &PlanContext,
) -> Result<(), AiSchedulerError> {
    if plan_context.queue_items.is_empty() {
        if !plan_context.already_scheduled.is_empty() {
            return Err(bad_request_message(format!(
                "「{}」队列里的 {} 条都已经在日历上了，不会重复排期；想重排 AI 之前的安排，请勾选「重新安排已写入日历的 AI 日程」",
                request.target_date,
                plan_context.already_scheduled.len()
            )));
        }
        return Err(bad_request_message(format!(
            "「{}」的计划队列里没有未完成条目，先在「今日 / 计划」里加几条再生成草案",
            request.target_date
        )));
    }
    let frames = slots::build_day_frames(plan_context);
    if !frames.is_empty() && frames.iter().all(|frame| frame.past) {
        return Err(bad_request_message(format!(
            "{} 已经过去：请把没完成的条目移到今天的队列，或选择今天及之后的日期",
            plan_context.horizon_end
        )));
    }
    let break_minutes = slots::break_minutes(plan_context);
    let room: i64 = frames
        .iter()
        .filter(|frame| !frame.past)
        .map(|frame| {
            let free: i64 = frame
                .free_gaps(&[], break_minutes)
                .iter()
                .map(|(start, end)| end - start)
                .sum();
            free.min(frame.capacity_left(0, plan_context.max_daily_minutes))
        })
        .sum();
    if room < MIN_SEGMENT_MINUTES {
        return Err(bad_request_message(
            "规划范围内已经没有可排的空档（今天剩下的时间不多，或日程与学习上限已排满）；可以改成「连续 3 天」，或在设置里放宽可用时段",
        ));
    }
    Ok(())
}

/// 预览编排（命令 4）：开关校验 → 队列与空档预检 → **模型排期** → （仅显式要求时）本地兜底。
///
/// 降级语义（2026-09-26 按用户要求修正，偏离 §6.3）：默认**不**悄悄回落本地启发式。
/// `request.allow_local_fallback = true`（前端错误卡片上的「改用本地排期」）时直接走本地，
/// 草案带 `degraded = true`，前端明确标注这是兜底结果。
pub fn preview_proposal(
    connection: &Connection,
    request: AiPlanRequest,
    settings: &AiSchedulerSettings,
) -> Result<AiPlanProposal, AiSchedulerError> {
    ensure_ai_ready(settings)?;
    expire_stale_proposals(connection)?;
    let request = context::normalize_plan_request(request)?;
    let plan_context = context::build_context(connection, &request, settings)?;
    ensure_schedulable(&request, &plan_context)?;
    generate(connection, &request, settings, &plan_context, None)
}

/// 选路径。用户点了「改用本地排期」就直接走本地——再等一次多半还会失败的模型请求没有意义。
/// 「按反馈调整」只能走模型：本地规则读不懂自然语言反馈。
fn generate(
    connection: &Connection,
    request: &AiPlanRequest,
    settings: &AiSchedulerSettings,
    plan_context: &PlanContext,
    revision: Option<&prompt::PlanRevision<'_>>,
) -> Result<AiPlanProposal, AiSchedulerError> {
    if request.allow_local_fallback && revision.is_none() {
        return local_plan_proposal(connection, request, plan_context);
    }
    llm_plan(connection, request, settings, plan_context, revision)
}

fn local_plan_proposal(
    connection: &Connection,
    request: &AiPlanRequest,
    plan_context: &PlanContext,
) -> Result<AiPlanProposal, AiSchedulerError> {
    let raw = plan_locally(plan_context, request)?;
    let outcome = validator::validate(&raw, plan_context);
    let mut items = outcome.items;
    append_meal_items(&mut items, plan_context);
    persist_proposal(
        connection,
        NewProposal {
            request,
            plan_context,
            items: &items,
            warnings: &outcome.warnings,
            unscheduled: &outcome.unscheduled,
            engine: ENGINE_LOCAL_HEURISTIC,
            degraded: true,
            model: "",
            scope: SCOPE_DAY,
            window: None,
            summary: raw.summary.as_deref(),
        },
    )
}

/// 模型路径：提示词 → Chat Completions → `refine`（可行性修复）→ 同一个校验器 → 落库（engine = llm）。
fn llm_plan(
    connection: &Connection,
    request: &AiPlanRequest,
    settings: &AiSchedulerSettings,
    plan_context: &PlanContext,
    revision: Option<&prompt::PlanRevision<'_>>,
) -> Result<AiPlanProposal, AiSchedulerError> {
    let base_url = client::normalize_base_url(&settings.base_url)?;
    let api_key = settings::load_api_key(connection)?;
    let http = client::build_client(settings.timeout_seconds)?;

    let system_prompt = prompt::build_system_prompt();
    let user_prompt = prompt::build_user_prompt(request, plan_context, revision);
    let outcome = client::chat_json(
        &http,
        &base_url,
        &api_key,
        settings,
        &system_prompt,
        &user_prompt,
    )?;

    // 模型给的时间不可行就挪到最近空档，漏排的补上——而不是整条丢掉。
    let refined = refine::make_feasible(
        &outcome.response,
        plan_context,
        request.respect_priority,
        refine::Mode::Llm,
    );
    let validated = validator::validate(&refined.response, plan_context);
    let mut warnings = validated.warnings;
    warnings.extend(refined.warnings);
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
            summary: refined.response.summary.as_deref(),
        },
    )
}

/// 按反馈调整（命令 6）：用生成原草案时的同一组请求参数重读当前库，把上一版草案与用户反馈
/// 一起交给模型。失败时原草案保持不变（只有新草案落库成功才会顶掉旧的）。
pub fn revise_proposal(
    connection: &Connection,
    proposal_id: i64,
    feedback: &str,
    settings: &AiSchedulerSettings,
) -> Result<AiPlanProposal, AiSchedulerError> {
    ensure_ai_ready(settings)?;
    let feedback: String = feedback.trim().chars().take(MAX_FEEDBACK_CHARS).collect();
    if feedback.is_empty() {
        return Err(bad_request_message(
            "先写一句想怎么调整，例如「数学挪到下午」",
        ));
    }
    let proposal = load_proposal(connection, proposal_id)?.ok_or_else(not_found)?;
    if proposal.status != PROPOSAL_STATUS_DRAFT {
        return Err(AiSchedulerError::new(
            ERR_CONFLICT,
            "这份草案已经写入或放弃，不能再调整",
            false,
        ));
    }
    let snapshot = load_proposal_snapshot(connection, proposal_id)?;
    let mut request = snapshot
        .and_then(|plan_context| plan_context.source_request)
        .unwrap_or_else(|| AiPlanRequest {
            target_date: proposal.target_date.clone(),
            horizon_days: proposal.horizon_days,
            ..AiPlanRequest::default()
        });
    request.allow_local_fallback = false;
    let request = context::normalize_plan_request(request)?;
    let plan_context = context::build_context(connection, &request, settings)?;
    ensure_schedulable(&request, &plan_context)?;

    let revision = prompt::PlanRevision {
        previous_items: &proposal.items,
        feedback: &feedback,
    };
    generate(connection, &request, settings, &plan_context, Some(&revision))
}

/// 删除草案里的条目（命令 5）。
///
/// 本期只支持**删除**：传入的条目必须是草案里已有的（按 id 匹配），时间 / 标题 / 来源一律以库里
/// 存的为准，前端改不了。时间微调属于 S5 的拖拽，届时再放开并在这里补可行性校验。
pub fn update_proposal_items(
    connection: &Connection,
    proposal_id: i64,
    incoming: &[AiPlanItem],
) -> Result<AiPlanProposal, AiSchedulerError> {
    let mut proposal = load_proposal(connection, proposal_id)?.ok_or_else(not_found)?;
    if proposal.status != PROPOSAL_STATUS_DRAFT {
        return Err(AiSchedulerError::new(
            ERR_CONFLICT,
            "这份草案已经写入或放弃，不能再修改",
            false,
        ));
    }
    let kept_ids: BTreeSet<String> = {
        let stored: HashMap<&str, &AiPlanItem> = proposal
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect();
        let mut kept = BTreeSet::new();
        for item in incoming {
            let Some(original) = stored.get(item.id.as_str()) else {
                return Err(AiSchedulerError::new(
                    ERR_CONFLICT,
                    "草案已经变化，请刷新后重试",
                    false,
                ));
            };
            if original.schedule_date != item.schedule_date
                || original.start_minute != item.start_minute
                || original.end_minute != item.end_minute
            {
                return Err(bad_request_message(
                    "预览阶段暂不支持直接改时间；可以在下方写一句反馈，让 AI 按反馈调整",
                ));
            }
            kept.insert(item.id.clone());
        }
        kept
    };

    let (mut kept, removed): (Vec<AiPlanItem>, Vec<AiPlanItem>) =
        std::mem::take(&mut proposal.items)
            .into_iter()
            .partition(|item| kept_ids.contains(&item.id));
    // 某天的学习条目全删了，那天的三餐也一起去掉：日历上只剩三餐没有意义。
    let study_dates: BTreeSet<String> = kept
        .iter()
        .filter(|item| item.kind != "meal")
        .map(|item| item.schedule_date.clone())
        .collect();
    kept.retain(|item| item.kind != "meal" || study_dates.contains(&item.schedule_date));

    // 被删的学习条目回到「没排上」，拆段条目只要还剩一段就不算没排上。
    let kept_queue_ids: BTreeSet<i64> = kept
        .iter()
        .filter_map(|item| item.source_today_item_id)
        .collect();
    let mut unscheduled = proposal.unscheduled.clone();
    let mut dropped_queue_ids: BTreeSet<i64> = BTreeSet::new();
    for item in removed.iter().filter(|item| item.kind != "meal") {
        let Some(queue_id) = item.source_today_item_id else {
            continue;
        };
        if kept_queue_ids.contains(&queue_id)
            || unscheduled.iter().any(|entry| entry.item_id == queue_id)
        {
            continue;
        }
        dropped_queue_ids.insert(queue_id);
        unscheduled.push(UnscheduledEntry {
            item_id: queue_id,
            title: item.title.clone(),
            reason: "已从草案中移除".to_string(),
        });
    }
    let warnings: Vec<AiPlanWarning> = std::mem::take(&mut proposal.warnings)
        .into_iter()
        .filter(|warning| {
            warning
                .item_id
                .as_deref()
                .is_none_or(|item_id| kept_ids.contains(item_id))
        })
        .filter(|warning| {
            warning
                .queue_item_id
                .is_none_or(|queue_id| !dropped_queue_ids.contains(&queue_id))
        })
        .collect();

    let now = Utc::now().to_rfc3339();
    let items_json = serde_json::to_string(&kept).map_err(serialize_error)?;
    let warnings_json = serde_json::to_string(&warnings).map_err(serialize_error)?;
    let unscheduled_json = serde_json::to_string(&unscheduled).map_err(serialize_error)?;
    connection
        .execute(
            "
            UPDATE ai_plan_proposals
            SET items_json = ?1, warnings_json = ?2, unscheduled_json = ?3, updated_at = ?4
            WHERE id = ?5 AND status = ?6
            ",
            params![
                items_json,
                warnings_json,
                unscheduled_json,
                now,
                proposal_id,
                PROPOSAL_STATUS_DRAFT
            ],
        )
        .map_err(db_error)?;
    load_proposal(connection, proposal_id)?.ok_or_else(not_found)
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
            missed_count: 0,
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
            clock: None,
            already_scheduled: Vec::new(),
            replaceable_block_ids: Vec::new(),
            source_request: None,
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
            replaceable: false,
        }];
        // 高效时段在 13:00–15:00。
        plan_context.peak_windows = vec![window(5, 780, 900)];

        let raw = plan_locally(&plan_context, &request()).expect("plan");

        // 两个空档都放得下 100 分钟：(480,590) 与 (670,900)。
        // 按「先来先占」会落在 480，命中高效时段的那一段应当被优先。
        assert_eq!(raw.items.len(), 1);
        // 高优先级条目从高效时段开头（13:00）开始，整段落在高效时段里，
        // 11:10–13:00 留给后面的条目，而不是从 11:10 起跨进高效时段、把它切碎。
        assert_eq!(
            raw.items[0].start_minute, 780,
            "应优先使用命中高效时段的空档"
        );
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

    /// 截止日已经过了：旧实现让条目从计划里消失，现在尽早补上并由校验器提示逾期。
    #[test]
    fn overdue_item_is_still_scheduled_with_due_risk() {
        let plan_context = base_context(vec![queue_item(1, "high", 60, Some("2026-09-24"))]);
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items.len(), 1);
        assert_eq!(raw.items[0].start_minute, 480);
        assert_eq!(raw.items[0].rationale.as_deref(), Some("已逾期，尽早补上"));
        let outcome = validator::validate(&raw, &plan_context);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_DUE_RISK));
    }

    /// 截止日还来得及时优先排在截止日前；截止日前实在没空，排到之后也比消失强。
    #[test]
    fn due_date_is_preferred_but_late_placement_beats_disappearing() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 60, Some("2026-09-25"))]);
        plan_context.horizon_days = 2;
        plan_context.horizon_end = "2026-09-26".to_string();
        plan_context.available_windows = vec![window(5, 480, 720), window(6, 480, 720)];
        // 周五整段被锁定日程占满。
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 9,
            date: "2026-09-25".to_string(),
            start_minute: 480,
            end_minute: 720,
            title: "模拟考试".to_string(),
            locked: true,
            replaceable: false,
        }];
        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items.len(), 1);
        assert_eq!(raw.items[0].date, "2026-09-26");
        let outcome = validator::validate(&raw, &plan_context);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_DUE_RISK));
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
            missed_count: 0,
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
            replaceable: false,
        }];

        let raw = plan_locally(&plan_context, &request()).expect("plan");
        // 剩 600–720 共 120 分钟，加最小间隔只有 110 可用，装不下 480。
        assert!(raw.items.is_empty());
    }

    /// 是否可替换由 `build_context` 决定（只有本次重新安排的条目的旧 AI 块才会标记），
    /// 排期器只看 `replaceable`。
    #[test]
    fn replaceable_blocks_do_not_block_space() {
        let mut plan_context = base_context(vec![queue_item(1, "high", 60, None)]);
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 9,
            date: "2026-09-25".to_string(),
            start_minute: 480,
            end_minute: 600,
            title: "AI 上次排的".to_string(),
            locked: false,
            replaceable: true,
        }];

        let raw = plan_locally(&plan_context, &request()).expect("plan");

        assert_eq!(raw.items.len(), 1);
        assert_eq!(raw.items[0].start_minute, 480, "可替换的旧块可被顶替");
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

    fn study(date: &str, start: i64, end: i64, queue_id: i64) -> AiPlanItem {
        AiPlanItem {
            id: validator::item_id_for(date, start, queue_id),
            source_task_id: None,
            source_today_item_id: Some(queue_id),
            schedule_date: date.to_string(),
            start_minute: start,
            end_minute: end,
            title: format!("条目{queue_id}"),
            category_key: "math".to_string(),
            subject_id: None,
            priority: "high".to_string(),
            rationale: None,
            manually_adjusted: false,
            conflict_with: Vec::new(),
            kind: "study".to_string(),
        }
    }

    #[test]
    fn meals_are_only_added_on_days_with_study() {
        let mut plan_context = base_context(Vec::new());
        plan_context.horizon_days = 2;
        plan_context.horizon_end = "2026-09-26".to_string();
        plan_context.planner_preferences.auto_meals = true;
        let mut items = vec![study("2026-09-25", 480, 540, 1)];
        append_meal_items(&mut items, &plan_context);

        let meals: Vec<&AiPlanItem> = items.iter().filter(|item| item.kind == "meal").collect();
        assert_eq!(meals.len(), 3);
        assert!(meals.iter().all(|meal| meal.schedule_date == "2026-09-25"));
    }

    #[test]
    fn meals_skip_past_times_and_existing_meal_blocks() {
        let mut plan_context = base_context(Vec::new());
        plan_context.planner_preferences.auto_meals = true;
        // 现在 09:00，早餐已过；午餐上次已经写进日历。
        plan_context.clock = Some(PlanClock {
            date: "2026-09-25".to_string(),
            minute: 9 * 60,
        });
        plan_context.existing_blocks = vec![ContextBlock {
            block_id: 5,
            date: "2026-09-25".to_string(),
            start_minute: 720,
            end_minute: 780,
            title: "午餐".to_string(),
            locked: true,
            replaceable: false,
        }];
        let mut items = vec![study("2026-09-25", 600, 660, 1)];
        append_meal_items(&mut items, &plan_context);

        let meals: Vec<&str> = items
            .iter()
            .filter(|item| item.kind == "meal")
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(meals, vec!["晚餐"]);
    }

    #[test]
    fn stats_count_split_items_once_and_exclude_meals() {
        let mut plan_context = base_context(Vec::new());
        plan_context.planner_preferences.daily_target_minutes = 300;
        let mut meal = study("2026-09-25", 720, 780, 0);
        meal.id = "2026-09-25-meal-午餐".to_string();
        meal.source_today_item_id = None;
        meal.kind = "meal".to_string();
        let items = vec![
            study("2026-09-25", 480, 540, 1),
            study("2026-09-25", 600, 660, 1),
            study("2026-09-25", 800, 845, 2),
            meal,
        ];
        let warnings = vec![AiPlanWarning::new(WARN_ADJUSTED, "挪过".to_string())
            .for_item(Some(items[0].id.clone()))];
        let stats = compute_stats(&items, &warnings, 0, &plan_context);

        assert_eq!(stats.scheduled_count, 2, "拆成两段的条目只算一条，三餐不算");
        assert_eq!(stats.study_minutes, 165);
        assert_eq!(stats.total_minutes, 225);
        assert_eq!(stats.target_minutes, 300);
        assert_eq!(stats.adjusted_count, 1);
        assert_eq!(stats.overflow_minutes, 0);
    }
}
