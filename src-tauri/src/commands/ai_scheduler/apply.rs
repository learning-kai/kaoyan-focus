//! 命令 7：把草案写入日历。
//!
//! 这是全流程唯一会写 `schedule_blocks` 的地方，因此承担四道保险（方案 §4.2）：
//! 1. **漂移检测**——草案生成后任务可能已被删除或勾选完成；
//! 2. **二次硬校验**——生成到确认之间，用户可能改过可用时段、也可能有远端日程同步进来；
//! 3. **替换旧 AI 块**——仅当草案是用「重新安排」生成的，才删掉被新安排取代的旧 AI 块；
//! 4. **单事务写入**——删旧、写新要么全成，要么一条都不动。
//!
//! 注意 `replan` 的教训（方案 §7.2）：冲突判定必须读**当前库**，不能依赖 `source_snapshot`
//! 里的日程块——CalDAV / 飞书会把远端变更反向写回 `schedule_blocks`。快照只提供
//! 「生成时的依据」：当时的预计时长、当时允许替换哪些块（且每个块都按当前库复核）。

use super::models::*;
use super::{context, database_path, planner, settings};
use crate::commands::checklist;
use crate::commands::schedule::{trigger_shared_sync, ENTITY_SCHEDULE_BLOCK};
use crate::storage::db::open_database;
use crate::sync_package::{ensure_sync_meta_for_local_id, mark_entity_deleted};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeSet, HashMap};
use tauri::AppHandle;

fn db_error(error: rusqlite::Error) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, format!("写入本地日程失败：{error}"), false)
}

fn source_error(message: String) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, message, false)
}

#[tauri::command]
pub fn apply_ai_plan_proposal(
    app: AppHandle,
    proposal_id: i64,
    options: AiApplyOptions,
) -> Result<AiApplyResult, String> {
    let connection = open_database(&database_path(&app)?).map_err(|error| error.to_string())?;
    let result =
        apply_proposal(&connection, proposal_id, &options).map_err(|error| error.to_envelope())?;

    // 只有真的写进去（或删掉了旧块）才触发同步，避免空转一次三路推送。
    if result.created_count > 0 || result.replaced_count > 0 {
        trigger_shared_sync(&app, "ai_schedule_apply");
    }
    Ok(result)
}

/// 读取当前库中与本次写入相关的日程块（**不是**快照里的）。
fn load_present_blocks(
    connection: &Connection,
    dates: &[String],
) -> Result<Vec<ContextBlock>, AiSchedulerError> {
    if dates.is_empty() {
        return Ok(Vec::new());
    }
    let mut sorted = dates.to_vec();
    sorted.sort();
    sorted.dedup();
    let first = sorted.first().cloned().unwrap_or_default();
    let last = sorted.last().cloned().unwrap_or_default();

    let mut statement = connection
        .prepare(
            "
            SELECT id, schedule_date, start_minute, end_minute, title, ai_locked, source_proposal_id
            FROM schedule_blocks
            WHERE schedule_date >= ?1 AND schedule_date <= ?2
            ORDER BY schedule_date ASC, start_minute ASC, id ASC
            ",
        )
        .map_err(db_error)?;

    let rows = statement
        .query_map(params![first, last], |row| {
            let ai_locked: i64 = row.get(5)?;
            let source_proposal_id: Option<i64> = row.get(6)?;
            Ok(ContextBlock {
                block_id: row.get(0)?,
                date: row.get(1)?,
                start_minute: row.get(2)?,
                end_minute: row.get(3)?,
                title: row.get(4)?,
                locked: ai_locked != 0 || source_proposal_id.is_none(),
                replaceable: false,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn overlaps(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> bool {
    a_start < b_end && b_start < a_end
}

/// 一个可以被本次写入替换掉的旧 AI 块（已按当前库复核）。
struct ReplaceCandidate {
    block_id: i64,
    source_today_item_id: Option<i64>,
    date: String,
    start_minute: i64,
    end_minute: i64,
}

/// apply 用到的「生成草案时的依据」，来自快照。
#[derive(Default)]
struct ApplyBasis {
    /// 生成时每个队列条目的有效预计时长。漂移检测与它比，而**不是**与草案块长比：
    /// 自适应时长、拆段、压缩都会让块长合理地偏离预计时长，拿块长比会每条都误报。
    snapshot_minutes: HashMap<i64, i64>,
    /// 生成时标为可替换、且此刻仍是「AI 产出、未锁定、未开始」的旧块。
    replaceable: Vec<ReplaceCandidate>,
}

fn load_apply_basis(
    connection: &Connection,
    proposal_id: i64,
) -> Result<ApplyBasis, AiSchedulerError> {
    let Some(snapshot) = planner::load_proposal_snapshot(connection, proposal_id)? else {
        return Ok(ApplyBasis::default());
    };
    let snapshot_minutes = snapshot
        .queue_items
        .iter()
        .map(|item| {
            (
                item.item_id,
                item.effective_minutes(snapshot.default_block_minutes),
            )
        })
        .collect();
    let mut replaceable = Vec::new();
    for block_id in &snapshot.replaceable_block_ids {
        // 预览之后用户可能锁定了它、开始了它，或远端同步改了它——这些都不能再删。
        let candidate = connection
            .query_row(
                "
                SELECT source_today_item_id, schedule_date, start_minute, end_minute
                FROM schedule_blocks
                WHERE id = ?1 AND ai_locked = 0 AND source_proposal_id IS NOT NULL
                  AND status = 'planned'
                ",
                params![block_id],
                |row| {
                    Ok(ReplaceCandidate {
                        block_id: *block_id,
                        source_today_item_id: row.get(0)?,
                        date: row.get(1)?,
                        start_minute: row.get(2)?,
                        end_minute: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(db_error)?;
        replaceable.extend(candidate);
    }
    Ok(ApplyBasis {
        snapshot_minutes,
        replaceable,
    })
}

/// 单条草案条目的处置结果。
struct PrepareOutcome {
    accepted: Vec<AiPlanItem>,
    warnings: Vec<AiPlanWarning>,
    skipped_count: i64,
    conflicted_count: i64,
    /// 因「与已有日程冲突」而跳过的条数，用于全冲突时给出准确错误文案。
    conflict_skips: i64,
}

/// 漂移检测 + 二次硬校验。
fn prepare_items(
    connection: &Connection,
    draft_items: &[AiPlanItem],
    settings: &AiSchedulerSettings,
    options: &AiApplyOptions,
    basis: &ApplyBasis,
) -> Result<PrepareOutcome, AiSchedulerError> {
    let dates: Vec<String> = draft_items
        .iter()
        .map(|item| item.schedule_date.clone())
        .collect();
    let present_blocks = load_present_blocks(connection, &dates)?;
    // 本次会被替换的旧 AI 块不算冲突：写入前它会在同一事务里被删掉。
    let replace_ids: BTreeSet<i64> = basis
        .replaceable
        .iter()
        .map(|candidate| candidate.block_id)
        .collect();

    let mut warnings: Vec<AiPlanWarning> = Vec::new();
    let mut accepted: Vec<AiPlanItem> = Vec::new();
    let mut skipped_count = 0i64;
    let mut conflicted_count = 0i64;
    let mut conflict_skips = 0i64;
    // 草案内部重叠时按开始时间保留第一条（方案 §4.2 第 3 步）。
    let mut placed: Vec<(String, i64, i64)> = Vec::new();

    let mut ordered: Vec<&AiPlanItem> = draft_items.iter().collect();
    ordered.sort_by(|a, b| {
        (a.schedule_date.as_str(), a.start_minute, a.end_minute).cmp(&(
            b.schedule_date.as_str(),
            b.start_minute,
            b.end_minute,
        ))
    });

    for item in ordered {
        // ── 1. 漂移检测：队列条目可能已被移出队列、勾选完成或改过属性 ──
        //
        // 检测对象是**队列条目**而不是它的来源任务：草案照队列生成，条目一旦被移出
        // 队列、被完成、或属性变了，草案就失去了依据。旧草案（无 `source_today_item_id`）
        // 跳过本步，交给后面的时段与冲突校验兜底。
        if let Some(queue_item_id) = item.source_today_item_id {
            match checklist::queue_item_drift_state(connection, queue_item_id)
                .map_err(source_error)?
            {
                None => {
                    warnings.push(
                        AiPlanWarning::new(
                            WARN_SNAPSHOT_DRIFT,
                            format!("「{}」已被移出当日队列，跳过该条", item.title),
                        )
                        .for_queue_item(Some(queue_item_id))
                        .for_item(Some(item.id.clone())),
                    );
                    skipped_count += 1;
                    continue;
                }
                Some(state) if state.completed => {
                    warnings.push(
                        AiPlanWarning::new(
                            WARN_SNAPSHOT_DRIFT,
                            format!("「{}」已完成，跳过该条", item.title),
                        )
                        .for_queue_item(Some(queue_item_id))
                        .for_item(Some(item.id.clone())),
                    );
                    skipped_count += 1;
                    continue;
                }
                Some(state) => {
                    // 属性变化不影响写入，但要让用户知道草案依据已经过时。
                    if state.priority != item.priority {
                        warnings.push(
                            AiPlanWarning::new(
                                WARN_SNAPSHOT_DRIFT,
                                format!(
                                    "「{}」的优先级在预览期间发生变化，仍按草案写入",
                                    item.title
                                ),
                            )
                            .for_queue_item(Some(queue_item_id))
                            .for_item(Some(item.id.clone())),
                        );
                    }
                    // 预计耗时在预览之后被改过 → 草案仍可写入，但要告诉用户依据已经过时。
                    //
                    // 比的是「生成时的预计时长」与「现在的预计时长」，两边用同一套有效时长口径
                    // （未估时回落为默认块长）。不和草案块长比：自适应 / 拆段 / 压缩本来就会改块长。
                    let current_minutes = if state.estimated_minutes > 0 {
                        state.estimated_minutes
                    } else {
                        settings.default_block_minutes.max(5)
                    };
                    if let Some(previous) = basis.snapshot_minutes.get(&queue_item_id) {
                        if *previous != current_minutes {
                            warnings.push(
                                AiPlanWarning::new(
                                    WARN_SNAPSHOT_DRIFT,
                                    format!(
                                        "「{}」的预计时长在预览后由 {previous} 分钟改为 {current_minutes} 分钟，仍按草案写入；如需同步请重新生成",
                                        item.title
                                    ),
                                )
                                .for_queue_item(Some(queue_item_id))
                                .for_item(Some(item.id.clone())),
                            );
                        }
                    }
                    // 草案把条目排到了截止日之后 → 属于截止日风险，必须显式提示（§5.5）。
                    if let Some(due_date) = state.due_date.as_deref() {
                        if item.schedule_date.as_str() > due_date {
                            warnings.push(
                                AiPlanWarning::new(
                                    WARN_DUE_RISK,
                                    format!(
                                        "「{}」的截止日是 {due_date}，草案排在 {}，已经逾期",
                                        item.title, item.schedule_date
                                    ),
                                )
                                .for_queue_item(Some(queue_item_id))
                                .for_item(Some(item.id.clone())),
                            );
                        }
                    }
                }
            }
        }

        // ── 2. 时间仍须合法且落在当前可用时段内（用户可能改过设置）──
        if item.end_minute <= item.start_minute || item.start_minute < 0 || item.end_minute > 1440 {
            warnings.push(
                AiPlanWarning::new(
                    WARN_NO_WINDOW,
                    format!("「{}」的时间区间不合法，已跳过", item.title),
                )
                .for_queue_item(item.source_today_item_id)
                .for_item(Some(item.id.clone())),
            );
            skipped_count += 1;
            continue;
        }
        let windows = context::windows_for_date(
            &settings.available_windows,
            context::parse_date(&item.schedule_date)?,
        );
        let inside_window = windows
            .iter()
            .any(|(start, end)| item.start_minute >= *start && item.end_minute <= *end);
        if item.kind != "meal" && !inside_window {
            warnings.push(
                AiPlanWarning::new(
                    WARN_NO_WINDOW,
                    format!(
                        "「{}」不在 {} 的可用时段内（设置可能已变更），已跳过",
                        item.title, item.schedule_date
                    ),
                )
                .for_queue_item(item.source_today_item_id)
                .for_item(Some(item.id.clone())),
            );
            skipped_count += 1;
            continue;
        }

        // ── 3. 草案内部重叠 ──
        let clashes = placed.iter().any(|(date, start, end)| {
            date == &item.schedule_date
                && overlaps(item.start_minute, item.end_minute, *start, *end)
        });
        if clashes {
            warnings.push(
                AiPlanWarning::new(
                    WARN_CONFLICT,
                    format!("「{}」与草案内其它条目重叠，已跳过该条", item.title),
                )
                .for_queue_item(item.source_today_item_id)
                .for_item(Some(item.id.clone())),
            );
            skipped_count += 1;
            conflicted_count += 1;
            continue;
        }

        // ── 4. 与当前库中已有日程的冲突 ──
        let overlapping: Vec<&ContextBlock> = present_blocks
            .iter()
            .filter(|block| {
                !replace_ids.contains(&block.block_id)
                    && block.date == item.schedule_date
                    && overlaps(
                        item.start_minute,
                        item.end_minute,
                        block.start_minute,
                        block.end_minute,
                    )
            })
            .collect();

        if !overlapping.is_empty() {
            let locked_hit = overlapping.iter().any(|block| block.locked);
            if locked_hit && options.skip_locked {
                warnings.push(
                    AiPlanWarning::new(
                        WARN_CONFLICT,
                        format!("「{}」与已锁定/手动安排的时段冲突，已跳过", item.title),
                    )
                    .for_queue_item(item.source_today_item_id)
                    .for_item(Some(item.id.clone())),
                );
                skipped_count += 1;
                conflicted_count += 1;
                conflict_skips += 1;
                continue;
            }
            if !options.overwrite_conflicts {
                let names: Vec<&str> = overlapping
                    .iter()
                    .map(|block| block.title.as_str())
                    .collect();
                warnings.push(
                    AiPlanWarning::new(
                        WARN_CONFLICT,
                        format!(
                            "「{}」与已有日程（{}）冲突，已跳过",
                            item.title,
                            names.join("、")
                        ),
                    )
                    .for_queue_item(item.source_today_item_id)
                    .for_item(Some(item.id.clone())),
                );
                skipped_count += 1;
                conflicted_count += 1;
                conflict_skips += 1;
                continue;
            }
        }

        placed.push((
            item.schedule_date.clone(),
            item.start_minute,
            item.end_minute,
        ));
        accepted.push(item.clone());
    }

    // 拆段条目的每一段都会触发同一条漂移提示，按 (code, message) 去重。
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    warnings.retain(|warning| seen.insert((warning.code.clone(), warning.message.clone())));

    Ok(PrepareOutcome {
        accepted,
        warnings,
        skipped_count,
        conflicted_count,
        conflict_skips,
    })
}

pub fn apply_proposal(
    connection: &Connection,
    proposal_id: i64,
    options: &AiApplyOptions,
) -> Result<AiApplyResult, AiSchedulerError> {
    let Some(proposal) = planner::load_proposal(connection, proposal_id)? else {
        return Err(AiSchedulerError::new(
            ERR_NOT_FOUND,
            "草案不存在或已被清理，请重新生成",
            false,
        ));
    };
    if proposal.status != PROPOSAL_STATUS_DRAFT {
        return Err(AiSchedulerError::new(
            ERR_CONFLICT,
            format!("该草案状态为「{}」，不能重复写入", proposal.status),
            false,
        ));
    }

    let settings = settings::current_settings(connection)?;
    let basis = load_apply_basis(connection, proposal_id)?;
    let outcome = prepare_items(connection, &proposal.items, &settings, options, &basis)?;

    if outcome.accepted.is_empty() {
        if outcome.conflict_skips > 0 && !proposal.items.is_empty() {
            return Err(AiSchedulerError::new(
                ERR_CONFLICT,
                "所有条目都与现有日程冲突，请在预览中调整后再写入",
                false,
            ));
        }
        // 没有任何可写入条目（例如任务全被删除）→ 直接把草案收尾，不再反复提示。
        planner::set_proposal_status(connection, proposal_id, PROPOSAL_STATUS_DISCARDED)?;
        return Ok(AiApplyResult {
            status: "failed".to_string(),
            created_count: 0,
            skipped_count: outcome.skipped_count,
            conflicted_count: outcome.conflicted_count,
            message: "没有条目被写入日历，草案已作废，请重新生成".to_string(),
            created_block_ids: Vec::new(),
            warnings: outcome.warnings,
            replaced_count: 0,
        });
    }

    // 只删「被新安排取代」的旧 AI 块：条目这次排上了，或旧块的位置被新安排占了。
    // 这次没排上的条目保留原来的块，不会从日历上凭空消失。
    let accepted_queue_ids: BTreeSet<i64> = outcome
        .accepted
        .iter()
        .filter_map(|item| item.source_today_item_id)
        .collect();
    let doomed: Vec<i64> = basis
        .replaceable
        .iter()
        .filter(|candidate| {
            candidate
                .source_today_item_id
                .is_some_and(|queue_id| accepted_queue_ids.contains(&queue_id))
                || outcome.accepted.iter().any(|item| {
                    item.schedule_date == candidate.date
                        && overlaps(
                            item.start_minute,
                            item.end_minute,
                            candidate.start_minute,
                            candidate.end_minute,
                        )
                })
        })
        .map(|candidate| candidate.block_id)
        .collect();

    let transaction = connection.unchecked_transaction().map_err(db_error)?;
    let now = Utc::now().to_rfc3339();
    let mut created_block_ids: Vec<i64> = Vec::new();

    // 与 `delete_schedule_block` 同一删除方式：先记墓碑再删行，远端日历才会同步删除。
    let deleted_at = Utc::now().timestamp_millis();
    for block_id in &doomed {
        mark_entity_deleted(&transaction, ENTITY_SCHEDULE_BLOCK, *block_id, deleted_at)
            .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false))?;
        transaction
            .execute(
                "DELETE FROM schedule_blocks WHERE id = ?1",
                params![block_id],
            )
            .map_err(db_error)?;
    }
    let replaced_count = doomed.len() as i64;

    for item in &outcome.accepted {
        // 链路完整性（方案 §3.1）在这里是**天然成立**的：草案本身就是从队列条目派生的，
        // `source_today_item_id` 直接指向那条已存在的 `today_plan_items` 记录，
        // 不再需要像旧实现那样「先补建今日计划再写日程」。
        // 队列条目已被删除时该列为空，此时只写日程块，不凭空造今日计划。
        transaction
            .execute(
                "
                INSERT INTO schedule_blocks (
                  schedule_date, title, note, category_key, subject_id, source_today_item_id,
                  start_minute, end_minute, status, source_task_id, source_proposal_id, ai_locked,
                  created_at, updated_at
                ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, 'planned', ?8, ?9, ?10, ?11, ?11)
                ",
                params![
                    item.schedule_date,
                    item.title,
                    item.category_key,
                    item.subject_id,
                    item.source_today_item_id,
                    item.start_minute,
                    item.end_minute,
                    item.source_task_id,
                    proposal_id,
                    i64::from(item.kind == "meal"),
                    now,
                ],
            )
            .map_err(db_error)?;

        let block_id = transaction.last_insert_rowid();
        ensure_sync_meta_for_local_id(
            &transaction,
            ENTITY_SCHEDULE_BLOCK,
            block_id,
            Some(format!(
                "schedule_block:{}:ai-proposal:{}:{}",
                item.schedule_date, proposal_id, item.start_minute
            )),
            Utc::now().timestamp_millis(),
        )
        .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false))?;

        created_block_ids.push(block_id);
    }

    planner::set_proposal_status(&transaction, proposal_id, PROPOSAL_STATUS_APPLIED)?;
    transaction.commit().map_err(db_error)?;

    let created_count = created_block_ids.len() as i64;
    let status = if outcome.skipped_count == 0 {
        "applied"
    } else {
        "partial"
    };
    let replaced_note = if replaced_count > 0 {
        format!("，替换了 {replaced_count} 条旧安排")
    } else {
        String::new()
    };
    let message = if outcome.skipped_count == 0 {
        format!("已写入 {created_count} 条日程{replaced_note}")
    } else {
        format!(
            "已写入 {created_count} 条{replaced_note}，跳过 {} 条（原因见下方提示）",
            outcome.skipped_count
        )
    };

    Ok(AiApplyResult {
        status: status.to_string(),
        created_count,
        skipped_count: outcome.skipped_count,
        conflicted_count: outcome.conflicted_count,
        message,
        created_block_ids,
        warnings: outcome.warnings,
        replaced_count,
    })
}

/// 命令 8：丢弃草案。`schedule_blocks` 完全不受影响。
#[tauri::command]
pub fn discard_ai_plan_proposal(app: AppHandle, proposal_id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?).map_err(|error| error.to_string())?;
    let updated = planner::set_proposal_status(&connection, proposal_id, PROPOSAL_STATUS_DISCARDED)
        .map_err(|error| error.to_envelope())?;
    if !updated {
        return Err(
            AiSchedulerError::new(ERR_NOT_FOUND, "草案不存在或已被清理", false).to_envelope(),
        );
    }
    Ok(())
}

/// 命令 9：读取最近一个待确认草案。抽屉打开时用它恢复现场。
#[tauri::command]
pub fn get_latest_ai_plan_proposal(
    app: AppHandle,
    target_date: String,
) -> Result<Option<AiPlanProposal>, String> {
    let connection = open_database(&database_path(&app)?).map_err(|error| error.to_string())?;
    context::parse_date(&target_date).map_err(|error| error.to_envelope())?;
    planner::latest_draft_proposal(&connection, &target_date).map_err(|error| error.to_envelope())
}

/// 命令 5：从草案里删掉条目（预览阶段只支持删除，不改时间）。
#[tauri::command]
pub fn update_ai_plan_proposal_items(
    app: AppHandle,
    proposal_id: i64,
    items: Vec<AiPlanItem>,
) -> Result<AiPlanProposal, String> {
    let connection = open_database(&database_path(&app)?).map_err(|error| error.to_string())?;
    planner::update_proposal_items(&connection, proposal_id, &items)
        .map_err(|error| error.to_envelope())
}

/// 命令 6：按用户反馈重新生成。网络命令，同样走 `spawn_blocking`。
#[tauri::command]
pub async fn regenerate_ai_plan_proposal(
    app: AppHandle,
    proposal_id: i64,
    feedback: String,
) -> Result<AiPlanProposal, String> {
    tauri::async_runtime::spawn_blocking(move || run_revise(app, proposal_id, feedback))
        .await
        .map_err(|error| {
            AiSchedulerError::new(ERR_NETWORK, format!("排期后台任务失败：{error}"), true)
                .to_envelope()
        })?
        .map_err(|error| error.to_envelope())
}

fn run_revise(
    app: AppHandle,
    proposal_id: i64,
    feedback: String,
) -> Result<AiPlanProposal, AiSchedulerError> {
    let connection =
        open_database(&database_path(&app).map_err(db_error_string)?).map_err(db_error_string)?;
    let settings = settings::current_settings(&connection)?;
    planner::revise_proposal(&connection, proposal_id, &feedback, &settings)
}

/// 命令 4：AI 排期预览。
///
/// **网络命令必须 `async fn` + `spawn_blocking`**：模型调用走阻塞 HTTP，若在主线程
/// 执行会冻结整个界面（与 CalDAV / 飞书同步同一约定）。
#[tauri::command]
pub async fn preview_ai_schedule(
    app: AppHandle,
    request: AiPlanRequest,
) -> Result<AiPlanProposal, String> {
    tauri::async_runtime::spawn_blocking(move || run_preview(app, request))
        .await
        .map_err(|error| {
            AiSchedulerError::new(ERR_NETWORK, format!("排期后台任务失败：{error}"), true)
                .to_envelope()
        })?
        .map_err(|error| error.to_envelope())
}

fn run_preview(app: AppHandle, request: AiPlanRequest) -> Result<AiPlanProposal, AiSchedulerError> {
    let connection =
        open_database(&database_path(&app).map_err(db_error_string)?).map_err(db_error_string)?;
    let settings = settings::current_settings(&connection)?;
    planner::preview_proposal(&connection, request, &settings)
}

fn db_error_string(error: String) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, error, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_database;
    use std::path::PathBuf;

    fn temp_database(name: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(name);
        (directory, path)
    }

    /// 往当日队列塞一条**手动条目**（无清单来源）。
    ///
    /// 排期与漂移检测都只认 `today_plan_items` 这一行，因此夹具不再需要造清单任务。
    fn seed_queue_item(connection: &Connection, id: i64, title: &str, completed: bool) -> i64 {
        seed_queue_item_with(connection, id, title, completed, None, 60)
    }

    fn seed_queue_item_with(
        connection: &Connection,
        id: i64,
        title: &str,
        completed: bool,
        due_date: Option<&str>,
        estimated_minutes: i64,
    ) -> i64 {
        connection
            .execute(
                "
                INSERT INTO today_plan_items (
                  id, today_date, source_task_id, subject_id, title, note, due_date, sort_order,
                  completed, synced_source_completion, priority, estimated_minutes,
                  created_at, updated_at
                ) VALUES (?1, '2026-09-25', NULL, 3, ?2, NULL, ?3, 0, ?4, 0, 'high', ?5, 'now', 'now')
                ",
                params![
                    id,
                    title,
                    due_date,
                    i64::from(completed),
                    estimated_minutes
                ],
            )
            .expect("seed queue item");
        id
    }

    /// 草案条目。`queue_item_id` 是队列条目 id，也是 apply 阶段做漂移检测的键。
    fn item(id: &str, queue_item_id: i64, start: i64, end: i64) -> AiPlanItem {
        AiPlanItem {
            id: id.to_string(),
            // 手动条目：没有清单来源，但队列条目 id 一定在。
            source_task_id: None,
            source_today_item_id: Some(queue_item_id),
            schedule_date: "2026-09-25".to_string(),
            start_minute: start,
            end_minute: end,
            title: format!("条目{queue_item_id}"),
            category_key: "math".to_string(),
            subject_id: Some(3),
            priority: "high".to_string(),
            rationale: None,
            manually_adjusted: false,
            conflict_with: Vec::new(),
            kind: "study".to_string(),
        }
    }

    fn settings_with_friday_window() -> AiSchedulerSettings {
        AiSchedulerSettings {
            available_windows: vec![AiTimeWindow {
                weekday: 5,
                start_minute: 480,
                end_minute: 720,
            }],
            ..AiSchedulerSettings::default()
        }
    }

    /// 条目在预览之后被移出当日队列 → 草案失去依据，跳过。
    #[test]
    fn item_removed_from_queue_is_skipped() {
        let (_directory, path) = temp_database("apply-drift.sqlite3");
        let connection = open_database(&path).expect("open db");

        let outcome = prepare_items(
            &connection,
            &[item("a", 999, 480, 540)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert!(outcome.accepted.is_empty());
        assert_eq!(outcome.skipped_count, 1);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_SNAPSHOT_DRIFT));
    }

    #[test]
    fn completed_queue_item_is_skipped() {
        let (_directory, path) = temp_database("apply-completed.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "已完成的条目", true);

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 480, 540)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert!(outcome.accepted.is_empty());
        assert_eq!(outcome.skipped_count, 1);
    }

    #[test]
    fn item_outside_window_is_skipped() {
        let (_directory, path) = temp_database("apply-window.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目", false);

        // 12:00–13:00 不在周五 08:00–12:00 的窗口内。
        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 720, 780)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert!(outcome.accepted.is_empty());
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_NO_WINDOW));
    }

    #[test]
    fn meal_block_can_be_before_study_window() {
        let (_directory, path) = temp_database("apply-meal.sqlite3");
        let connection = open_database(&path).expect("open db");
        let mut meal = item("meal", 0, 420, 460);
        meal.source_today_item_id = None;
        meal.source_task_id = None;
        meal.title = "早餐".to_string();
        meal.kind = "meal".to_string();

        let outcome = prepare_items(
            &connection,
            &[meal],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert_eq!(outcome.accepted.len(), 1, "餐食不应被学习时段过滤");
    }

    #[test]
    fn manual_block_conflict_skips_by_default() {
        let (_directory, path) = temp_database("apply-conflict.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目", false);
        // 手动块：source_proposal_id 为 NULL → 视为 locked。
        connection
            .execute(
                "
                INSERT INTO schedule_blocks (
                  schedule_date, title, category_key, start_minute, end_minute, status,
                  created_at, updated_at
                ) VALUES ('2026-09-25', '手动安排', 'english', 480, 600, 'planned', 'now', 'now')
                ",
                [],
            )
            .expect("seed block");

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 540, 600)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert!(outcome.accepted.is_empty(), "默认不覆盖冲突");
        assert_eq!(outcome.conflict_skips, 1);
    }

    #[test]
    fn overwrite_conflicts_accepts_unlocked_overlap() {
        let (_directory, path) = temp_database("apply-overwrite.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目", false);
        // AI 产出的块（source_proposal_id 非空）→ 未锁定，可被覆盖。
        connection
            .execute(
                "
                INSERT INTO schedule_blocks (
                  schedule_date, title, category_key, start_minute, end_minute, status,
                  source_proposal_id, ai_locked, created_at, updated_at
                ) VALUES ('2026-09-25', 'AI 上次排的', 'english', 480, 600, 'planned', 9, 0, 'now', 'now')
                ",
                [],
            )
            .expect("seed block");

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 540, 600)],
            &settings_with_friday_window(),
            &AiApplyOptions {
                overwrite_conflicts: true,
                skip_locked: true,
            },
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert_eq!(outcome.accepted.len(), 1);
        assert_eq!(outcome.skipped_count, 0);
    }

    #[test]
    fn intra_draft_overlap_keeps_earliest_and_skips_rest() {
        let (_directory, path) = temp_database("apply-intra.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目一", false);
        seed_queue_item(&connection, 2, "条目二", false);

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 480, 600), item("b", 2, 540, 660)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert_eq!(outcome.accepted.len(), 1, "保留更早的一条");
        assert_eq!(outcome.accepted[0].source_today_item_id, Some(1));
        assert_eq!(outcome.skipped_count, 1);
    }

    /// 排期单位就是队列条目，因此写入的日程块必须直接挂回那条**已存在**的队列条目，
    /// 而不是像旧实现那样「先补建一条今日计划再写日程」。
    #[test]
    fn block_links_to_existing_queue_item_without_creating_a_duplicate() {
        let (_directory, path) = temp_database("apply-no-source.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "手动加进今天的条目", false);

        let request = AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            ..AiPlanRequest::default()
        };
        let settings = settings_with_friday_window();
        let plan_context =
            context::build_context(&connection, &request, &settings).expect("context");

        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                items: &[item("a", 1, 480, 540)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");

        let result =
            apply_proposal(&connection, proposal.id, &AiApplyOptions::default()).expect("apply");
        assert_eq!(result.created_count, 1);

        let (source_task_id, source_today_item_id): (Option<i64>, Option<i64>) = connection
            .query_row(
                "SELECT source_task_id, source_today_item_id FROM schedule_blocks",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read block");

        assert_eq!(source_task_id, None, "手动条目没有清单来源");
        assert_eq!(
            source_today_item_id,
            Some(1),
            "日程块必须挂回它派生自的那条队列条目"
        );

        let today_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM today_plan_items", [], |row| {
                row.get(0)
            })
            .expect("count today items");
        assert_eq!(today_count, 1, "队列里已有一条，不该再凭空补建第二条");
    }

    #[test]
    fn apply_writes_blocks_and_links_queue_item() {
        let (_directory, path) = temp_database("apply-write.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "数学 660 题", false);

        let settings = settings_with_friday_window();
        let plan_context = context::build_context(
            &connection,
            &context::normalize_plan_request(AiPlanRequest {
                target_date: "2026-09-25".to_string(),
                horizon_days: 1,
                ..AiPlanRequest::default()
            })
            .expect("normalize"),
            &settings,
        )
        .expect("context");

        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &AiPlanRequest {
                    target_date: "2026-09-25".to_string(),
                    horizon_days: 1,
                    ..AiPlanRequest::default()
                },
                plan_context: &plan_context,
                items: &[item("a", 1, 480, 540)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");

        let result =
            apply_proposal(&connection, proposal.id, &AiApplyOptions::default()).expect("apply");
        assert_eq!(result.status, "applied");
        assert_eq!(result.created_count, 1);

        let (title, source_task_id, source_today_item_id, proposal_id, status): (
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            String,
        ) = connection
            .query_row(
                "SELECT title, source_task_id, source_today_item_id, source_proposal_id, status
                 FROM schedule_blocks",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("read block");

        assert_eq!(title, "条目1");
        assert_eq!(
            source_task_id, None,
            "夹具那条队列条目是手动加的，没有清单来源"
        );
        assert_eq!(proposal_id, Some(proposal.id));
        assert_eq!(status, "planned");
        assert_eq!(
            source_today_item_id,
            Some(1),
            "必须挂回队列条目，保证日历与今日计划对得上（§3.1）"
        );

        let (refreshed_status,): (String,) = connection
            .query_row(
                "SELECT status FROM ai_plan_proposals WHERE id = ?1",
                params![proposal.id],
                |row| Ok((row.get(0)?,)),
            )
            .expect("read proposal");
        assert_eq!(refreshed_status, PROPOSAL_STATUS_APPLIED);
    }

    #[test]
    fn apply_result_carries_skip_reasons() {
        let (_directory, path) = temp_database("apply-partial.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "能排下的任务", false);
        seed_queue_item(&connection, 2, "排在窗口外的任务", false);

        let request = AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            ..AiPlanRequest::default()
        };
        let settings = settings_with_friday_window();
        // `apply_proposal` 重读的是库里的当前配置（用户可能在预览之后改过时段），
        // 因此必须把周五窗口真正落盘，否则会退回默认的全天时段，一条都不会被跳过。
        settings::persist_settings(&connection, &settings, "2026-09-25T00:00:00Z")
            .expect("persist settings");
        let plan_context =
            context::build_context(&connection, &request, &settings).expect("context");
        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                // 第二条 12:00–13:00 落在周五窗口（08:00–12:00）之外 → 会被跳过。
                items: &[item("a", 1, 480, 540), item("b", 2, 720, 780)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");

        let result =
            apply_proposal(&connection, proposal.id, &AiApplyOptions::default()).expect("apply");

        assert_eq!(result.status, "partial");
        assert_eq!(result.created_count, 1);
        assert_eq!(result.skipped_count, 1);
        // 只有计数时用户不知道该改什么，因此原因必须随结果回到前端。
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, WARN_NO_WINDOW);
        // 文案里的标题取自**草案条目**（`item()` 夹具固定为「任务{id}」），
        // 而不是清单表里的标题：草案自己就是被跳过的那份数据。
        assert!(
            result.warnings[0].message.contains("条目2"),
            "提示里要指名是哪一条：{}",
            result.warnings[0].message
        );
    }

    #[test]
    fn due_date_drift_is_reported_as_due_risk() {
        let (_directory, path) = temp_database("apply-due-risk.sqlite3");
        let connection = open_database(&path).expect("open db");
        // 草案在生成之后截止日被提前到了 09-20，此时 09-25 的安排已经逾期。
        seed_queue_item_with(
            &connection,
            1,
            "原本不紧急的任务",
            false,
            Some("2026-09-20"),
            60,
        );

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 480, 540)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert_eq!(outcome.accepted.len(), 1, "逾期不该阻止写入，只提示");
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_DUE_RISK));
    }

    fn basis_with_minutes(queue_item_id: i64, minutes: i64) -> ApplyBasis {
        ApplyBasis {
            snapshot_minutes: HashMap::from([(queue_item_id, minutes)]),
            replaceable: Vec::new(),
        }
    }

    #[test]
    fn duration_drift_uses_effective_minutes_rule() {
        let (_directory, path) = temp_database("apply-duration-drift.sqlite3");
        let connection = open_database(&path).expect("open db");
        // 生成草案时预计 60 分钟，预览之后被改成了 90 分钟。
        seed_queue_item_with(&connection, 1, "耗时变长的任务", false, None, 90);

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 480, 540)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &basis_with_minutes(1, 60),
        )
        .expect("prepare");

        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_SNAPSHOT_DRIFT));
        assert_eq!(outcome.accepted.len(), 1);
    }

    /// 自适应时长 / 拆段让块长偏离预计时长是正常的，只要预计时长本身没变就不是漂移。
    #[test]
    fn adaptive_block_length_is_not_reported_as_drift() {
        let (_directory, path) = temp_database("apply-adaptive-no-drift.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item_with(&connection, 1, "预计 90 分钟、被拆成 60 分钟一段", false, None, 90);

        let outcome = prepare_items(
            &connection,
            &[item("a", 1, 480, 540)],
            &settings_with_friday_window(),
            &AiApplyOptions::default(),
            &basis_with_minutes(1, 90),
        )
        .expect("prepare");

        assert!(
            outcome
                .warnings
                .iter()
                .all(|warning| warning.code != WARN_SNAPSHOT_DRIFT),
            "{:?}",
            outcome.warnings
        );
    }

    #[test]
    fn unestimated_task_does_not_trigger_duration_drift() {
        let (_directory, path) = temp_database("apply-no-duration-drift.sqlite3");
        let connection = open_database(&path).expect("open db");
        // estimated_minutes = 0 表示未估时，块长应等于 default_block_minutes。
        seed_queue_item_with(&connection, 1, "未估时任务", false, None, 0);

        let mut settings = settings_with_friday_window();
        settings.default_block_minutes = 60;
        let outcome = prepare_items(
            &connection,
            // 60 分钟块，正好等于 default_block_minutes → 不应误报漂移。
            &[item("a", 1, 480, 540)],
            &settings,
            &AiApplyOptions::default(),
            &ApplyBasis::default(),
        )
        .expect("prepare");

        assert!(
            !outcome
                .warnings
                .iter()
                .any(|warning| warning.code == WARN_SNAPSHOT_DRIFT),
            "未估时任务的块长就是默认块长，不该报漂移：{:?}",
            outcome.warnings
        );
    }

    #[test]
    fn applying_discarded_proposal_is_rejected() {
        let (_directory, path) = temp_database("apply-status.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目", false);

        let request = AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            ..AiPlanRequest::default()
        };
        let settings = settings_with_friday_window();
        let plan_context =
            context::build_context(&connection, &request, &settings).expect("context");
        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                items: &[item("a", 1, 480, 540)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");

        planner::set_proposal_status(&connection, proposal.id, PROPOSAL_STATUS_DISCARDED)
            .expect("discard");

        let error = apply_proposal(&connection, proposal.id, &AiApplyOptions::default())
            .expect_err("不能重复写入");
        assert_eq!(error.code, ERR_CONFLICT);
    }

    #[test]
    fn all_conflicting_items_yield_conflict_error() {
        let (_directory, path) = temp_database("apply-all-conflict.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目", false);
        connection
            .execute(
                "
                INSERT INTO schedule_blocks (
                  schedule_date, title, category_key, start_minute, end_minute, status,
                  created_at, updated_at
                ) VALUES ('2026-09-25', '手动安排', 'english', 480, 600, 'planned', 'now', 'now')
                ",
                [],
            )
            .expect("seed block");

        let request = AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            ..AiPlanRequest::default()
        };
        let settings = settings_with_friday_window();
        let plan_context =
            context::build_context(&connection, &request, &settings).expect("context");
        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                items: &[item("a", 1, 540, 600)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");

        let error = apply_proposal(&connection, proposal.id, &AiApplyOptions::default())
            .expect_err("全部冲突应报 conflict");
        assert_eq!(error.code, ERR_CONFLICT);

        let (count,): (i64,) = connection
            .query_row("SELECT COUNT(*) FROM schedule_blocks", [], |row| {
                Ok((row.get(0)?,))
            })
            .expect("count");
        assert_eq!(count, 1, "冲突时不得写入任何块");
    }

    /// 队列条目已经挂在日历上的日程块（2026-09-25）。`source_proposal_id = None` 即手动块。
    fn seed_linked_block(
        connection: &Connection,
        queue_item_id: i64,
        start: i64,
        end: i64,
        source_proposal_id: Option<i64>,
    ) -> i64 {
        connection
            .execute(
                "
                INSERT INTO schedule_blocks (
                  schedule_date, title, category_key, source_today_item_id, start_minute,
                  end_minute, status, source_proposal_id, ai_locked, created_at, updated_at
                ) VALUES ('2026-09-25', '旧安排', 'math', ?1, ?2, ?3, 'planned', ?4, 0, 'now', 'now')
                ",
                params![queue_item_id, start, end, source_proposal_id],
            )
            .expect("seed linked block");
        connection.last_insert_rowid()
    }

    fn clock_at(minute: i64) -> Option<PlanClock> {
        Some(PlanClock {
            date: "2026-09-25".to_string(),
            minute,
        })
    }

    fn friday_request(replace_ai_blocks: bool) -> AiPlanRequest {
        AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 1,
            replace_ai_blocks,
            ..AiPlanRequest::default()
        }
    }

    fn queued_ids(plan_context: &PlanContext) -> Vec<i64> {
        let mut ids: Vec<i64> = plan_context.queue_items.iter().map(|item| item.item_id).collect();
        ids.sort_unstable();
        ids
    }

    /// 点两次「生成」不该出现两份：已在日历上的条目默认跳过，并说明原因。
    #[test]
    fn items_already_on_calendar_are_not_planned_twice() {
        let (_directory, path) = temp_database("context-already.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "AI 排过的", false);
        seed_queue_item(&connection, 2, "手动排过的", false);
        seed_queue_item(&connection, 3, "还没排的", false);
        seed_linked_block(&connection, 1, 600, 660, Some(9));
        seed_linked_block(&connection, 2, 700, 760, None);

        let plan_context = context::build_context_at(
            &connection,
            &friday_request(false),
            &settings_with_friday_window(),
            clock_at(8 * 60),
        )
        .expect("context");

        assert_eq!(queued_ids(&plan_context), vec![3]);
        let detail = |item_id: i64| {
            plan_context
                .already_scheduled
                .iter()
                .find(|entry| entry.item_id == item_id)
                .map(|entry| entry.detail.clone())
                .unwrap_or_default()
        };
        assert!(detail(1).contains("AI 已排"), "{}", detail(1));
        assert!(detail(2).contains("手动安排"), "{}", detail(2));
        assert!(plan_context.replaceable_block_ids.is_empty());
    }

    /// 「重新安排」只放开 AI 未锁定的旧块，手动块依旧钉住条目。
    #[test]
    fn replace_mode_requeues_ai_items_but_keeps_manual_ones_pinned() {
        let (_directory, path) = temp_database("context-replace.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "AI 排过的", false);
        seed_queue_item(&connection, 2, "手动排过的", false);
        let ai_block = seed_linked_block(&connection, 1, 600, 660, Some(9));
        seed_linked_block(&connection, 2, 700, 760, None);

        let plan_context = context::build_context_at(
            &connection,
            &friday_request(true),
            &settings_with_friday_window(),
            clock_at(8 * 60),
        )
        .expect("context");

        assert_eq!(queued_ids(&plan_context), vec![1]);
        assert_eq!(plan_context.replaceable_block_ids, vec![ai_block]);
        assert!(plan_context
            .existing_blocks
            .iter()
            .any(|block| block.block_id == ai_block && block.replaceable));
        assert!(plan_context
            .already_scheduled
            .iter()
            .any(|entry| entry.item_id == 2));
    }

    /// 时间过了却没完成：不算「已排」，而是记一次错过，条目照常重排。
    #[test]
    fn missed_block_counts_as_missed_and_item_is_replanned() {
        let (_directory, path) = temp_database("context-missed.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "上午没做的", false);
        seed_linked_block(&connection, 1, 480, 540, Some(9));

        let plan_context = context::build_context_at(
            &connection,
            &friday_request(false),
            &settings_with_friday_window(),
            clock_at(10 * 60),
        )
        .expect("context");

        assert_eq!(queued_ids(&plan_context), vec![1]);
        assert_eq!(plan_context.queue_items[0].missed_count, 1);
        assert!(plan_context.already_scheduled.is_empty());
        assert!(
            plan_context.replaceable_block_ids.is_empty(),
            "过去的块是历史，不删"
        );
    }

    #[test]
    fn replan_replaces_old_ai_block_in_the_same_transaction() {
        let (_directory, path) = temp_database("apply-replace.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "数学", false);
        let old_block = seed_linked_block(&connection, 1, 600, 660, Some(9));

        let request = friday_request(true);
        let plan_context = context::build_context_at(
            &connection,
            &request,
            &settings_with_friday_window(),
            clock_at(7 * 60),
        )
        .expect("context");
        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                items: &[item("a", 1, 480, 540)],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LOCAL_HEURISTIC,
                degraded: false,
                model: "",
                scope: SCOPE_DAY,
                window: None,
                summary: None,
            },
        )
        .expect("persist");
        assert_eq!(proposal.replaceable_block_count, 1);

        let result =
            apply_proposal(&connection, proposal.id, &AiApplyOptions::default()).expect("apply");
        assert_eq!(result.created_count, 1);
        assert_eq!(result.replaced_count, 1);
        assert!(result.message.contains("替换了 1 条旧安排"), "{}", result.message);

        let mut statement = connection
            .prepare("SELECT id FROM schedule_blocks ORDER BY id")
            .expect("prepare");
        let remaining: Vec<i64> = statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert_eq!(remaining.len(), 1, "旧块被删，只剩新块");
        assert!(!remaining.contains(&old_block));
    }

    #[test]
    fn removing_items_moves_them_back_to_unscheduled_and_drops_orphan_meals() {
        let (_directory, path) = temp_database("proposal-remove.sqlite3");
        let connection = open_database(&path).expect("open db");
        seed_queue_item(&connection, 1, "条目一", false);
        seed_queue_item(&connection, 2, "条目二", false);
        let request = friday_request(false);
        let plan_context =
            context::build_context_at(&connection, &request, &settings_with_friday_window(), None)
                .expect("context");
        let mut meal = item("meal", 0, 720, 780);
        meal.source_today_item_id = None;
        meal.title = "午餐".to_string();
        meal.kind = "meal".to_string();
        let study_a = item("a", 1, 480, 540);
        let study_b = item("b", 2, 600, 660);
        let proposal = planner::persist_proposal(
            &connection,
            planner::NewProposal {
                request: &request,
                plan_context: &plan_context,
                items: &[study_a.clone(), study_b.clone(), meal.clone()],
                warnings: &[],
                unscheduled: &[],
                engine: ENGINE_LLM,
                degraded: false,
                model: "m",
                scope: SCOPE_DAY,
                window: None,
                summary: Some("上午数学"),
            },
        )
        .expect("persist");
        assert_eq!(proposal.summary.as_deref(), Some("上午数学"));

        // 预览阶段不能改时间，只能删。
        let mut moved = study_a.clone();
        moved.start_minute = 500;
        let error = planner::update_proposal_items(&connection, proposal.id, &[moved, meal.clone()])
            .err()
            .expect("改时间应被拒绝");
        assert_eq!(error.code, ERR_BAD_REQUEST);

        let updated = planner::update_proposal_items(&connection, proposal.id, &[study_a, meal.clone()])
            .expect("remove b");
        assert_eq!(updated.items.len(), 2);
        assert!(updated
            .unscheduled
            .iter()
            .any(|entry| entry.item_id == 2 && entry.reason == "已从草案中移除"));

        let emptied =
            planner::update_proposal_items(&connection, proposal.id, &[meal]).expect("remove a");
        assert!(emptied.items.is_empty(), "只剩三餐时三餐也去掉");
        assert_eq!(emptied.unscheduled.len(), 2);
    }
}
