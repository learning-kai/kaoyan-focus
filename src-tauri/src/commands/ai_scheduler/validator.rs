//! 草案校验器：把「排期候选」变成可信的 `AiPlanItem`。
//!
//! 两条排期路径（本地启发式 / LLM）产出的是同一个 `RawPlanResponse`，都经过这里，
//! 因此合法性判定只有一处实现（方案 §2.1 原则 3、§5.5）。
//!
//! 核心防幻觉手段：候选里**只有** `item_id`（队列条目 id）与时间。`title` /
//! `category_key` / `subject_id` / `priority` 一律由本模块按 `item_id` 从上下文回填，
//! 模型既无法编造条目、也无法篡改标题（方案 §2.1 原则 1）。

use super::context;
use super::models::*;
use std::collections::{BTreeSet, HashMap};

const TIME_ALIGN_MINUTES: i64 = 5;
const MAX_DAY_MINUTE: i64 = 1440;

pub struct ValidationOutcome {
    pub items: Vec<AiPlanItem>,
    pub warnings: Vec<AiPlanWarning>,
    /// 原样透传的「排不下」声明，供 stats 与前端展示。
    pub unscheduled: Vec<RawUnscheduledItem>,
}

fn align_to_five(minutes: i64) -> i64 {
    (minutes / TIME_ALIGN_MINUTES) * TIME_ALIGN_MINUTES
}

/// 修正起止时间。
///
/// 非 5 分钟对齐会被吸附；`start >= end` 或越界时按条目实际时长重算一次；
/// 仍然不成立则返回 `None`（调用方丢弃该条，方案 §5.5 第 3 行）。
fn normalize_bounds(raw_start: i64, raw_end: i64, item_minutes: i64) -> Option<(i64, i64)> {
    if !(0..=MAX_DAY_MINUTE).contains(&raw_start) || !(0..=MAX_DAY_MINUTE).contains(&raw_end) {
        return None;
    }

    let start = align_to_five(raw_start);
    let end = align_to_five(raw_end);
    if start < end {
        return Some((start, end));
    }

    // 倒置或长度为零：按条目时长从 start 重算。
    let repaired_end = align_to_five(start + item_minutes.max(TIME_ALIGN_MINUTES));
    if repaired_end > start && repaired_end <= MAX_DAY_MINUTE {
        Some((start, repaired_end))
    } else {
        None
    }
}

fn is_within_windows(start: i64, end: i64, windows: &[(i64, i64)]) -> bool {
    windows
        .iter()
        .any(|(window_start, window_end)| start >= *window_start && end <= *window_end)
}

fn overlaps(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> bool {
    a_start < b_end && b_start < a_end
}

fn item_id_for(date: &str, start_minute: i64, queue_item_id: i64) -> String {
    format!("{date}-{start_minute}-{queue_item_id}")
}

/// 逐条硬校验 + 回填。返回仍然可用的条目与全部警告。
pub fn validate(raw: &RawPlanResponse, plan_context: &PlanContext) -> ValidationOutcome {
    let mut warnings: Vec<AiPlanWarning> = Vec::new();
    let mut items: Vec<AiPlanItem> = Vec::new();
    // 同一队列条目在同一天只保留第一条（方案 §6.4）。
    let mut seen_item_dates: BTreeSet<(i64, String)> = BTreeSet::new();
    let mut used_ids: BTreeSet<String> = BTreeSet::new();

    for candidate in &raw.items {
        let Some(queue_item) = plan_context.item_by_id(candidate.item_id) else {
            warnings.push(
                AiPlanWarning::new(
                    WARN_UNKNOWN_TASK,
                    format!(
                        "模型给出的条目 {} 不在本次排期范围内，已忽略",
                        candidate.item_id
                    ),
                )
                .for_queue_item(Some(candidate.item_id)),
            );
            continue;
        };

        let Ok(date) = context::parse_date(&candidate.date) else {
            warnings.push(
                AiPlanWarning::new(
                    WARN_NO_WINDOW,
                    format!("日期 {} 无法解析，已忽略该条目", candidate.date),
                )
                .for_queue_item(Some(candidate.item_id)),
            );
            continue;
        };
        let date_string = context::date_string(date);

        if !seen_item_dates.insert((candidate.item_id, date_string.clone())) {
            warnings.push(
                AiPlanWarning::new(
                    WARN_DUPLICATE,
                    format!(
                        "「{}」在 {} 被排了多次，只保留第一条",
                        queue_item.title, date_string
                    ),
                )
                .for_queue_item(Some(candidate.item_id)),
            );
            continue;
        }

        let item_minutes = queue_item.effective_minutes(plan_context.default_block_minutes);
        let Some((start_minute, end_minute)) =
            normalize_bounds(candidate.start_minute, candidate.end_minute, item_minutes)
        else {
            warnings.push(
                AiPlanWarning::new(
                    WARN_NO_WINDOW,
                    format!("「{}」的时间区间不合法，已忽略", queue_item.title),
                )
                .for_queue_item(Some(candidate.item_id)),
            );
            continue;
        };

        let windows = context::windows_for_date(&plan_context.available_windows, date);
        if !is_within_windows(start_minute, end_minute, &windows) {
            warnings.push(
                AiPlanWarning::new(
                    WARN_NO_WINDOW,
                    format!(
                        "「{}」被排到了 {} 的可用时段之外，已忽略",
                        queue_item.title, date_string
                    ),
                )
                .for_queue_item(Some(candidate.item_id)),
            );
            continue;
        }

        let mut id = item_id_for(&date_string, start_minute, candidate.item_id);
        // 理论上不会撞（同日同条目已去重），留一个兜底避免前端 key 重复。
        let mut suffix = 1;
        while !used_ids.insert(id.clone()) {
            id = format!(
                "{date_string}-{start_minute}-{}-{suffix}",
                candidate.item_id
            );
            suffix += 1;
        }

        // 与已有日程的重叠：填 conflict_with，是否覆盖交给 apply 的 options 决定。
        let conflict_with: Vec<i64> = plan_context
            .existing_blocks
            .iter()
            .filter(|block| {
                block.date == date_string
                    && overlaps(
                        start_minute,
                        end_minute,
                        block.start_minute,
                        block.end_minute,
                    )
            })
            .map(|block| block.block_id)
            .collect();

        // 排到截止日之后：保留但告警（方案 §5.5）。
        if let Some(due_date) = queue_item.due_date.as_deref() {
            if date_string.as_str() > due_date {
                warnings.push(
                    AiPlanWarning::new(
                        WARN_DUE_RISK,
                        format!("「{}」被排到了截止日 {due_date} 之后", queue_item.title),
                    )
                    .for_queue_item(Some(candidate.item_id)),
                );
            }
        }

        items.push(AiPlanItem {
            id,
            // 展示 / 追溯用的清单来源，写库链路不依赖它。
            source_task_id: queue_item.source_task_id,
            // 排期单位就是队列条目，直接回填，apply 阶段不必再补建今日计划。
            source_today_item_id: Some(candidate.item_id),
            schedule_date: date_string,
            start_minute,
            end_minute,
            // 以下四项全部由后端回填，模型不返回。
            title: queue_item.title.clone(),
            category_key: queue_item.category_key.clone(),
            subject_id: queue_item.subject_id,
            priority: queue_item.priority.clone(),
            rationale: candidate
                .rationale
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().take(40).collect()),
            manually_adjusted: false,
            conflict_with,
        });
    }

    warnings.extend(detect_intra_draft_conflicts(&items));
    warnings.extend(detect_over_capacity(&items, plan_context));

    let mut unscheduled = raw.unscheduled.clone();
    // 被校验器丢弃的条目也要在「排不下」里体现，否则前端会显示「全部排成功」。
    let dropped: BTreeSet<i64> = warnings
        .iter()
        .filter(|warning| {
            matches!(
                warning.code.as_str(),
                WARN_UNKNOWN_TASK | WARN_NO_WINDOW | WARN_DUPLICATE
            )
        })
        .filter_map(|warning| warning.queue_item_id)
        .collect();
    for item_id in dropped {
        if unscheduled.iter().all(|entry| entry.item_id != item_id) {
            unscheduled.push(RawUnscheduledItem {
                item_id,
                reason: Some("未能排入可用时段".to_string()),
            });
        }
    }

    ValidationOutcome {
        items,
        warnings,
        unscheduled,
    }
}

/// 草案内部互相重叠：保留双方，但标 `conflict` 让前端高亮（方案 §5.5 第 4 行）。
fn detect_intra_draft_conflicts(items: &[AiPlanItem]) -> Vec<AiPlanWarning> {
    let mut warnings = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let clashes: Vec<&AiPlanItem> = items
            .iter()
            .enumerate()
            .filter(|(other_index, other)| {
                *other_index != index
                    && other.schedule_date == item.schedule_date
                    && overlaps(
                        item.start_minute,
                        item.end_minute,
                        other.start_minute,
                        other.end_minute,
                    )
            })
            .map(|(_, other)| other)
            .collect();

        if clashes.is_empty() {
            continue;
        }

        let names: Vec<&str> = clashes.iter().map(|other| other.title.as_str()).collect();
        warnings.push(
            AiPlanWarning::new(
                WARN_CONFLICT,
                format!(
                    "「{}」与草案内其它 {} 条安排重叠：{}",
                    item.title,
                    clashes.len(),
                    names.join("、")
                ),
            )
            .for_queue_item(item.source_today_item_id)
            .for_item(Some(item.id.clone())),
        );
    }
    warnings
}

/// 单日总时长超上限：保留但告警，`overflow_minutes` 由 stats 汇总。
fn detect_over_capacity(items: &[AiPlanItem], plan_context: &PlanContext) -> Vec<AiPlanWarning> {
    let mut totals: HashMap<String, i64> = HashMap::new();
    for item in items {
        *totals.entry(item.schedule_date.clone()).or_insert(0) += item.duration_minutes();
    }

    let mut warnings = Vec::new();
    let mut dates: Vec<&String> = totals.keys().collect();
    dates.sort();
    for date in dates {
        let total = totals[date];
        if total > plan_context.max_daily_minutes {
            warnings.push(AiPlanWarning::new(
                WARN_OVER_CAPACITY,
                format!(
                    "{date} 共排入 {total} 分钟，超出单日上限 {} 分钟",
                    plan_context.max_daily_minutes
                ),
            ));
        }
    }
    warnings
}

/// 单日超出容量的分钟数合计，用于 stats。
pub fn overflow_minutes(items: &[AiPlanItem], plan_context: &PlanContext) -> i64 {
    let mut totals: HashMap<String, i64> = HashMap::new();
    for item in items {
        *totals.entry(item.schedule_date.clone()).or_insert(0) += item.duration_minutes();
    }
    totals
        .values()
        .map(|total| (total - plan_context.max_daily_minutes).max(0))
        .sum()
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

    fn queue_item(item_id: i64, due_date: Option<&str>, minutes: i64) -> ContextQueueItem {
        ContextQueueItem {
            item_id,
            source_task_id: Some(item_id * 10),
            title: format!("队列条目{item_id}"),
            category_key: "math".to_string(),
            category_label: "数学".to_string(),
            subject_id: Some(3),
            priority: "high".to_string(),
            estimated_minutes: minutes,
            due_date: due_date.map(str::to_string),
            note: None,
        }
    }

    /// 2026-09-25 是周五，08:00–12:00 与 14:00–18:00 可用。
    fn context_with(items: Vec<ContextQueueItem>, blocks: Vec<ContextBlock>) -> PlanContext {
        PlanContext {
            generated_at: "2026-09-25T00:00:00Z".to_string(),
            horizon_days: 1,
            horizon_start: "2026-09-25".to_string(),
            horizon_end: "2026-09-25".to_string(),
            queue_items: items,
            existing_blocks: blocks,
            available_windows: vec![window(5, 480, 720), window(5, 840, 1080)],
            peak_windows: vec![],
            min_break_minutes: 10,
            max_daily_minutes: 480,
            default_block_minutes: 45,
        }
    }

    fn raw(items: Vec<RawPlanItem>) -> RawPlanResponse {
        RawPlanResponse {
            items,
            unscheduled: vec![],
        }
    }

    fn raw_item(item_id: i64, start: i64, end: i64) -> RawPlanItem {
        RawPlanItem {
            item_id,
            date: "2026-09-25".to_string(),
            start_minute: start,
            end_minute: end,
            rationale: None,
        }
    }

    #[test]
    fn backfills_title_and_category_from_context() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 480, 570)]), &plan_context);

        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].title, "队列条目41");
        assert_eq!(outcome.items[0].category_key, "math");
        assert_eq!(outcome.items[0].subject_id, Some(3));
        assert_eq!(outcome.items[0].priority, "high");
        // 排期单位是队列条目，因此条目 id 必须原样回填，且能反查到清单来源。
        assert_eq!(outcome.items[0].source_today_item_id, Some(41));
        assert_eq!(outcome.items[0].source_task_id, Some(410));
    }

    #[test]
    fn unknown_task_is_dropped_with_warning() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(999, 480, 570)]), &plan_context);

        assert!(outcome.items.is_empty());
        assert_eq!(outcome.warnings[0].code, WARN_UNKNOWN_TASK);
        assert_eq!(outcome.warnings[0].queue_item_id, Some(999));
    }

    #[test]
    fn duplicate_task_on_same_day_keeps_first_only() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(
            &raw(vec![raw_item(41, 480, 570), raw_item(41, 600, 690)]),
            &plan_context,
        );

        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].start_minute, 480);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_DUPLICATE));
    }

    #[test]
    fn out_of_window_item_is_dropped() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        // 12:00–14:00 是空档，不在可用时段内。
        let outcome = validate(&raw(vec![raw_item(41, 720, 840)]), &plan_context);

        assert!(outcome.items.is_empty());
        assert_eq!(outcome.warnings[0].code, WARN_NO_WINDOW);
    }

    #[test]
    fn unaligned_minutes_are_snapped_not_dropped() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 482, 571)]), &plan_context);

        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].start_minute, 480);
        assert_eq!(outcome.items[0].end_minute, 570);
    }

    #[test]
    fn inverted_range_is_repaired_with_task_duration() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 600, 500)]), &plan_context);

        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].start_minute, 600);
        assert_eq!(outcome.items[0].end_minute, 690, "应按条目时长补齐");
    }

    #[test]
    fn unestimated_task_falls_back_to_default_block_minutes() {
        let plan_context = context_with(vec![queue_item(41, None, 0)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 600, 600)]), &plan_context);

        assert_eq!(outcome.items[0].duration_minutes(), 45);
    }

    #[test]
    fn conflict_with_existing_blocks_is_recorded() {
        let plan_context = context_with(
            vec![queue_item(41, None, 90)],
            vec![ContextBlock {
                block_id: 77,
                date: "2026-09-25".to_string(),
                start_minute: 480,
                end_minute: 600,
                title: "已存在的英语真题".to_string(),
                locked: true,
            }],
        );
        let outcome = validate(&raw(vec![raw_item(41, 540, 660)]), &plan_context);

        assert_eq!(
            outcome.items.len(),
            1,
            "冲突条目保留，由 apply 决定是否覆盖"
        );
        assert_eq!(outcome.items[0].conflict_with, vec![77]);
    }

    #[test]
    fn due_date_risk_is_flagged_but_kept() {
        let plan_context = context_with(vec![queue_item(41, Some("2026-09-24"), 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 480, 570)]), &plan_context);

        assert_eq!(outcome.items.len(), 1);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_DUE_RISK));
    }

    #[test]
    fn intra_draft_overlap_is_flagged_for_both_items() {
        let plan_context = context_with(
            vec![queue_item(41, None, 90), queue_item(42, None, 90)],
            vec![],
        );
        let outcome = validate(
            &raw(vec![raw_item(41, 480, 600), raw_item(42, 540, 660)]),
            &plan_context,
        );

        assert_eq!(outcome.items.len(), 2, "重叠条目保留双方，前端高亮");
        let overlapping: Vec<&AiPlanWarning> = outcome
            .warnings
            .iter()
            .filter(|warning| warning.code == WARN_CONFLICT)
            .collect();
        assert_eq!(overlapping.len(), 2, "双方都应被标注");
    }

    #[test]
    fn over_capacity_is_flagged_but_items_are_kept() {
        let mut plan_context = context_with(
            vec![queue_item(41, None, 240), queue_item(42, None, 240)],
            vec![],
        );
        // 两段可用时段各 240 分钟，合计恰好 480，等于默认日容量 → 造不出溢出。
        // 把日容量压到 300 才能验「超容量只告警、不丢条目」。
        plan_context.max_daily_minutes = 300;
        let outcome = validate(
            &raw(vec![raw_item(41, 480, 720), raw_item(42, 840, 1080)]),
            &plan_context,
        );

        assert_eq!(outcome.items.len(), 2, "超容量只告警，不丢条目");
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.code == WARN_OVER_CAPACITY));
        assert_eq!(overflow_minutes(&outcome.items, &plan_context), 480 - 300);
    }

    #[test]
    fn unscheduled_gains_dropped_tasks() {
        let plan_context = context_with(vec![queue_item(41, None, 90)], vec![]);
        let outcome = validate(&raw(vec![raw_item(41, 720, 840)]), &plan_context);

        assert_eq!(outcome.unscheduled.len(), 1);
        assert_eq!(outcome.unscheduled[0].item_id, 41);
    }

    #[test]
    fn item_ids_are_stable_and_unique() {
        let plan_context = context_with(
            vec![queue_item(41, None, 90), queue_item(42, None, 90)],
            vec![],
        );
        let outcome = validate(
            &raw(vec![raw_item(41, 480, 570), raw_item(42, 480, 570)]),
            &plan_context,
        );

        let ids: BTreeSet<&String> = outcome.items.iter().map(|item| &item.id).collect();
        assert_eq!(ids.len(), 2, "不同条目同时间戳时 id 仍必须唯一");
    }
}
