//! 上下文快照：把「要排的任务 + 已有日程 + 可用时段 + 容量参数」收成一个可序列化结构。
//!
//! 该快照承担两个职责：
//! 1. 喂给排期器（S3 是本地启发式，S4 起追加喂给模型，由 `prompt.rs` 消费）；
//! 2. 落进 `ai_plan_proposals.source_snapshot`，供 `apply` 阶段做漂移检测（方案 §4.2）。
//!
//! **注意**：`replan` 不得复用本快照里的 `existing_blocks`——远端 CalDAV / 飞书会反向
//! 改写 `schedule_blocks`，必须重读当前库（方案 §7.2）。

use super::models::*;
use crate::commands::checklist;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};

/// 分类键的合法取值域。与 `checklist.rs` 的五值枚举一致。
pub const CATEGORY_KEYS: [&str; 5] = ["politics", "english", "math", "major", "general"];

pub fn source_error(message: String) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, format!("本地数据读取失败：{message}"), false)
}

pub fn bad_request(message: impl Into<String>) -> AiSchedulerError {
    AiSchedulerError::new(ERR_BAD_REQUEST, message, false)
}

pub fn parse_date(value: &str) -> Result<NaiveDate, AiSchedulerError> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| bad_request(format!("日期格式不正确：{value}（应为 YYYY-MM-DD）")))
}

pub fn date_string(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// 周几，1 = 周一 … 7 = 周日。与 `AiTimeWindow.weekday` 同一约定。
pub fn weekday_of(date: NaiveDate) -> i64 {
    date.weekday().num_days_from_monday() as i64 + 1
}

/// horizon 覆盖的全部日期。
pub fn horizon_dates(start: NaiveDate, horizon_days: i64) -> Vec<NaiveDate> {
    (0..horizon_days.max(1))
        .filter_map(|offset| start.checked_add_signed(Duration::days(offset)))
        .collect()
}

/// 把日期序列边界（字符串形式）对齐到 horizon，返回 `(起始日, 结束日)`。
///
/// `build_context` 与 S6 的漂移检测都用它，避免两处各写一遍「起止日」推导。
/// 起始日无法解析时原样回退，让上层各自报错，而不是在这里静默改成今天。
pub fn horizon_bounds(start: &str, horizon_days: i64) -> (String, String) {
    let Ok(start_date) = parse_date(start) else {
        return (start.to_string(), start.to_string());
    };
    let dates = horizon_dates(start_date, horizon_days);
    let end = dates.last().copied().unwrap_or(start_date);
    (date_string(start_date), date_string(end))
}

/// 把请求参数规范化：钳制 horizon、校验并归一化日期、截断补充指令。
///
/// `queue_item_ids` / `category_keys` 传空数组等价于 `None`（表示「不限制」），
/// 否则前端「一个分类都没勾」会被理解成「一个条目都不排」。
pub fn normalize_plan_request(
    mut request: AiPlanRequest,
) -> Result<AiPlanRequest, AiSchedulerError> {
    let target = parse_date(&request.target_date)?;
    request.target_date = date_string(target);
    request.horizon_days = request
        .horizon_days
        .clamp(MIN_HORIZON_DAYS, MAX_HORIZON_DAYS);

    request.extra_instruction = request
        .extra_instruction
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .chars()
                .take(MAX_EXTRA_INSTRUCTION_CHARS)
                .collect::<String>()
        });

    if let Some(ids) = request.queue_item_ids.as_mut() {
        ids.retain(|id| *id > 0);
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            request.queue_item_ids = None;
        }
    }

    if let Some(keys) = request.category_keys.as_mut() {
        keys.retain(|key| CATEGORY_KEYS.contains(&key.trim()));
        keys.sort();
        keys.dedup();
        if keys.is_empty() {
            request.category_keys = None;
        }
    }

    Ok(request)
}

/// 合并重叠或首尾相接的区间。源数据可能来自用户手填的相邻时段。
pub fn merge_ranges(mut ranges: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    ranges.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => {
                last.1 = last.1.max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// 某一天实际可用的时段（已按 weekday 过滤并合并）。
pub fn windows_for_date(windows: &[AiTimeWindow], date: NaiveDate) -> Vec<(i64, i64)> {
    let weekday = weekday_of(date);
    merge_ranges(
        windows
            .iter()
            .filter(|window| window.weekday == weekday)
            .map(|window| (window.start_minute, window.end_minute))
            .collect(),
    )
}

/// 某一天的高效时段（同样按 weekday 过滤并合并）。
pub fn peaks_for_date(windows: &[AiTimeWindow], date: NaiveDate) -> Vec<(i64, i64)> {
    windows_for_date(windows, date)
}

fn matches_queue_filter(item: &checklist::SchedulableQueueItem, request: &AiPlanRequest) -> bool {
    if let Some(ids) = request.queue_item_ids.as_ref() {
        if !ids.contains(&item.item_id) {
            return false;
        }
    }
    if let Some(keys) = request.category_keys.as_ref() {
        if !keys.iter().any(|key| key == &item.category_key) {
            return false;
        }
    }
    true
}

/// 读取 horizon 范围内的已有日程块。
///
/// `locked` 的判定：用户手动微调过（`ai_locked != 0`），或该块根本不是 AI 产出的
/// （`source_proposal_id IS NULL`）。后者覆盖用户在日历页手动新建的块——那类块必须
/// 当作硬约束，否则一键排期会把人自己排的事顶掉。
fn list_context_blocks(
    connection: &Connection,
    horizon_start: &str,
    horizon_end: &str,
) -> Result<Vec<ContextBlock>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, schedule_date, start_minute, end_minute, title,
                   ai_locked, source_proposal_id
            FROM schedule_blocks
            WHERE schedule_date >= ?1 AND schedule_date <= ?2
            ORDER BY schedule_date ASC, start_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![horizon_start, horizon_end], |row| {
            let ai_locked: i64 = row.get(5)?;
            let source_proposal_id: Option<i64> = row.get(6)?;
            Ok(ContextBlock {
                block_id: row.get(0)?,
                date: row.get(1)?,
                start_minute: row.get(2)?,
                end_minute: row.get(3)?,
                title: row.get(4)?,
                locked: ai_locked != 0 || source_proposal_id.is_none(),
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// 构造快照。排期来源遵循「队列即输入」：
/// 只取 `target_date` 那天的队列条目（`today_plan_items`，未完成），
/// 再与 `queue_item_ids` / `category_keys` 取交集。
pub fn build_context(
    connection: &Connection,
    request: &AiPlanRequest,
    settings: &AiSchedulerSettings,
) -> Result<PlanContext, AiSchedulerError> {
    // 目标日期必须合法；horizon 的起止日推导统一交给 `horizon_bounds`。
    parse_date(&request.target_date)?;
    let (horizon_start, horizon_end) = horizon_bounds(&request.target_date, request.horizon_days);

    let labels = checklist::category_label_map(connection).map_err(source_error)?;

    let queue_items: Vec<ContextQueueItem> =
        checklist::list_queue_items(connection, &request.target_date)
            .map_err(source_error)?
            .into_iter()
            .filter(|item| matches_queue_filter(item, request))
            .map(|item| {
                let category_label = labels
                    .get(&item.category_key)
                    .cloned()
                    .unwrap_or_else(|| item.category_key.clone());
                ContextQueueItem {
                    item_id: item.item_id,
                    source_task_id: item.source_task_id,
                    title: item.title,
                    category_key: item.category_key,
                    category_label,
                    subject_id: item.subject_id,
                    priority: item.priority,
                    estimated_minutes: item.estimated_minutes,
                    due_date: item.due_date,
                    // 备注只在 send_notes 打开时进入快照。快照会落盘到 source_snapshot，
                    // 因此这里同时是「备注不外发」的执行点（方案 §7）。
                    note: if settings.send_notes { item.note } else { None },
                }
            })
            .collect();

    let existing_blocks =
        list_context_blocks(connection, &horizon_start, &horizon_end).map_err(source_error)?;

    Ok(PlanContext {
        generated_at: Utc::now().to_rfc3339(),
        horizon_days: request.horizon_days,
        horizon_start,
        horizon_end,
        queue_items,
        existing_blocks,
        available_windows: settings.available_windows.clone(),
        peak_windows: settings.peak_windows.clone(),
        min_break_minutes: settings.min_break_minutes,
        max_daily_minutes: settings.max_daily_minutes,
        default_block_minutes: settings.default_block_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("test date")
    }

    #[test]
    fn weekday_matches_iso_numbering() {
        // 2026-09-25 是周五。
        assert_eq!(weekday_of(date("2026-09-25")), 5);
        assert_eq!(weekday_of(date("2026-09-28")), 1);
        assert_eq!(weekday_of(date("2026-09-27")), 7);
    }

    #[test]
    fn horizon_dates_spans_requested_days() {
        let dates = horizon_dates(date("2026-09-25"), 3);
        assert_eq!(dates.len(), 3);
        assert_eq!(date_string(dates[0]), "2026-09-25");
        assert_eq!(date_string(dates[2]), "2026-09-27");
    }

    #[test]
    fn horizon_dates_clamps_to_at_least_one_day() {
        assert_eq!(horizon_dates(date("2026-09-25"), 0).len(), 1);
        assert_eq!(horizon_dates(date("2026-09-25"), -5).len(), 1);
    }

    #[test]
    fn merge_ranges_joins_touching_and_overlapping() {
        let merged = merge_ranges(vec![(480, 600), (600, 720), (700, 800), (900, 960)]);
        assert_eq!(merged, vec![(480, 800), (900, 960)]);
    }

    #[test]
    fn windows_for_date_filters_by_weekday() {
        let windows = vec![
            AiTimeWindow {
                weekday: 5,
                start_minute: 480,
                end_minute: 720,
            },
            AiTimeWindow {
                weekday: 6,
                start_minute: 0,
                end_minute: 60,
            },
        ];
        assert_eq!(
            windows_for_date(&windows, date("2026-09-25")),
            vec![(480, 720)]
        );
        assert!(windows_for_date(&windows, date("2026-09-21")).is_empty());
    }

    #[test]
    fn normalize_rejects_bad_date_and_clamps_horizon() {
        let error = normalize_plan_request(AiPlanRequest {
            target_date: "25/09/2026".to_string(),
            ..AiPlanRequest::default()
        })
        .unwrap_err();
        assert_eq!(error.code, ERR_BAD_REQUEST);

        let request = normalize_plan_request(AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            horizon_days: 99,
            ..AiPlanRequest::default()
        })
        .expect("normalize");
        assert_eq!(request.horizon_days, MAX_HORIZON_DAYS);
    }

    #[test]
    fn normalize_truncates_instruction_and_drops_blank_filters() {
        let request = normalize_plan_request(AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            queue_item_ids: Some(vec![]),
            category_keys: Some(vec!["不存在的分类".to_string()]),
            extra_instruction: Some("  ".to_string()),
            ..AiPlanRequest::default()
        })
        .expect("normalize");

        assert!(request.queue_item_ids.is_none(), "空数组应等价于不限制");
        assert!(
            request.category_keys.is_none(),
            "全部非法分类应回落为不限制，而不是「一个都不排」"
        );
        assert!(request.extra_instruction.is_none(), "纯空白应清空");

        let long = normalize_plan_request(AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            extra_instruction: Some("啊".repeat(500)),
            ..AiPlanRequest::default()
        })
        .expect("normalize");
        assert_eq!(
            long.extra_instruction.map(|value| value.chars().count()),
            Some(MAX_EXTRA_INSTRUCTION_CHARS)
        );
    }

    #[test]
    fn queue_filter_intersects_ids_and_categories() {
        let item = checklist::SchedulableQueueItem {
            item_id: 7,
            source_task_id: Some(41),
            category_key: "math".to_string(),
            subject_id: Some(3),
            title: "数学 660 题".to_string(),
            note: None,
            due_date: None,
            priority: "high".to_string(),
            estimated_minutes: 90,
        };

        let all = AiPlanRequest {
            target_date: "2026-09-25".to_string(),
            ..AiPlanRequest::default()
        };
        assert!(matches_queue_filter(&item, &all));

        let by_id = AiPlanRequest {
            queue_item_ids: Some(vec![7]),
            ..all.clone()
        };
        assert!(matches_queue_filter(&item, &by_id));

        let wrong_id = AiPlanRequest {
            queue_item_ids: Some(vec![99]),
            ..all.clone()
        };
        assert!(
            !matches_queue_filter(&item, &wrong_id),
            "筛选的是队列条目 id（today_plan_items.id），不是清单任务 id"
        );

        let by_category = AiPlanRequest {
            category_keys: Some(vec!["math".to_string()]),
            ..all.clone()
        };
        assert!(matches_queue_filter(&item, &by_category));

        let wrong_category = AiPlanRequest {
            category_keys: Some(vec!["english".to_string()]),
            ..all.clone()
        };
        assert!(!matches_queue_filter(&item, &wrong_category));
    }

    #[test]
    fn horizon_bounds_expands_to_last_day() {
        let (start, end) = horizon_bounds("2026-09-25", 3);
        assert_eq!(start, "2026-09-25");
        assert_eq!(end, "2026-09-27");
    }

    #[test]
    fn horizon_bounds_falls_back_to_input_for_bad_date() {
        // 起始日非法时原样回退，由上层 `parse_date` 负责报错。
        let (start, end) = horizon_bounds("not-a-date", 3);
        assert_eq!(start, "not-a-date");
        assert_eq!(end, "not-a-date");
    }
}
