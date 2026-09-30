//! 空档引擎：把「可用时段 − 保留的已有日程 − 三餐 − 已经过去的时间」算成每天真正能排的空档。
//!
//! 三处共用这一份实现，保证「可行」的标准只有一个：
//! - `prompt`：直接把空档喂给模型，模型不必自己做区间减法（这正是它最容易算错的地方）；
//! - `refine`：用同一份空档判断模型给的时间能不能落地，落不了地就挪；
//! - 本地排期：从同一份空档里取位置。

use super::context;
use super::models::*;

pub const TIME_ALIGN_MINUTES: i64 = 5;
pub const DAY_MINUTES: i64 = 1440;
/// 提示词里不展示短于它的空档：塞不下任何有意义的任务，只会干扰模型。
pub const MIN_PROMPT_GAP_MINUTES: i64 = 15;

/// 休息节奏对最小间隔的影响。提示词、修复、本地排期三处必须一致。
pub fn break_minutes(plan_context: &PlanContext) -> i64 {
    let base = plan_context.min_break_minutes.max(0);
    match plan_context.planner_preferences.rest_style.as_str() {
        "gentle" => base.max(15),
        "focused" => base.min(5),
        _ => base,
    }
}

/// 向上对齐到 5 分钟。
pub fn align_up(minute: i64) -> i64 {
    (minute + TIME_ALIGN_MINUTES - 1).div_euclid(TIME_ALIGN_MINUTES) * TIME_ALIGN_MINUTES
}

/// 向下对齐到 5 分钟。
pub fn align_down(minute: i64) -> i64 {
    minute.div_euclid(TIME_ALIGN_MINUTES) * TIME_ALIGN_MINUTES
}

/// 某一天最早可以开始排的分钟。已过去的日期返回 `DAY_MINUTES`，即整天不可排。
pub fn earliest_start(clock: Option<&PlanClock>, date_key: &str) -> i64 {
    let Some(clock) = clock else {
        return 0;
    };
    match date_key.cmp(clock.date.as_str()) {
        std::cmp::Ordering::Less => DAY_MINUTES,
        std::cmp::Ordering::Equal => {
            align_up(clock.minute + NOW_BUFFER_MINUTES).clamp(0, DAY_MINUTES)
        }
        std::cmp::Ordering::Greater => 0,
    }
}

/// 把占用区间按最小间隔外扩：这样从「可用时段减去占用」得到的空档天然满足相邻间隔约束。
pub fn expand_by_break(ranges: &[(i64, i64)], break_minutes: i64) -> Vec<(i64, i64)> {
    ranges
        .iter()
        .map(|(start, end)| (start - break_minutes, end + break_minutes))
        .collect()
}

/// 从可用时段中挖掉占用区间，返回剩余空档。
pub fn subtract_ranges(windows: &[(i64, i64)], occupied: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut free: Vec<(i64, i64)> = windows.to_vec();
    for (occupied_start, occupied_end) in occupied {
        let mut next: Vec<(i64, i64)> = Vec::new();
        for (start, end) in free {
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

fn clip_start(ranges: &[(i64, i64)], earliest: i64) -> Vec<(i64, i64)> {
    ranges
        .iter()
        .filter_map(|(start, end)| {
            let start = (*start).max(earliest);
            (start < *end).then_some((start, *end))
        })
        .collect()
}

pub fn overlaps(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> bool {
    a_start < b_end && b_start < a_end
}

/// 已有日程块是不是一顿饭。餐食不计入学习时长上限。
pub fn is_meal_title(title: &str, plan_context: &PlanContext) -> bool {
    let trimmed = title.trim();
    matches!(trimmed, "早餐" | "午餐" | "晚餐")
        || plan_context
            .planner_preferences
            .meal_windows
            .iter()
            .any(|meal| meal.kind == trimmed)
}

/// 三餐的固定区间（仅 `auto_meals` 打开时）。
pub fn meal_ranges(plan_context: &PlanContext) -> Vec<(i64, i64)> {
    if !plan_context.planner_preferences.auto_meals {
        return Vec::new();
    }
    plan_context
        .planner_preferences
        .meal_windows
        .iter()
        .filter(|meal| meal.end_minute > meal.start_minute)
        .map(|meal| (meal.start_minute, meal.end_minute))
        .collect()
}

/// 一天的排期底板。
#[derive(Debug, Clone)]
pub struct DayFrame {
    pub date: chrono::NaiveDate,
    pub date_key: String,
    /// 可用时段：已按星期过滤、合并，并裁掉已经过去的部分。
    pub windows: Vec<(i64, i64)>,
    /// 高效时段，同样裁掉已过去的部分。
    pub peaks: Vec<(i64, i64)>,
    /// 不可占用的区间：保留下来的已有日程 + 三餐。
    pub fixed: Vec<(i64, i64)>,
    /// 保留下来的已有**学习**日程分钟数（不含三餐），计入单日上限。
    pub existing_study_minutes: i64,
    /// 整天都已经过去。
    pub past: bool,
}

impl DayFrame {
    /// 扣掉固定安排与 `placed`（本次已放进去的条目）后的空档，起点对齐到 5 分钟。
    pub fn free_gaps(&self, placed: &[(i64, i64)], break_minutes: i64) -> Vec<(i64, i64)> {
        let mut occupied = self.fixed.clone();
        occupied.extend_from_slice(placed);
        let occupied = expand_by_break(&context::merge_ranges(occupied), break_minutes);
        subtract_ranges(&self.windows, &occupied)
            .into_iter()
            .filter_map(|(start, end)| {
                let start = align_up(start);
                (start < end).then_some((start, end))
            })
            .collect()
    }

    /// 当天还能再排多少分钟学习。
    pub fn capacity_left(&self, placed_minutes: i64, max_daily_minutes: i64) -> i64 {
        (max_daily_minutes - self.existing_study_minutes - placed_minutes).max(0)
    }

    pub fn touches_peak(&self, start: i64, end: i64) -> bool {
        self.peaks
            .iter()
            .any(|(peak_start, peak_end)| overlaps(start, end, *peak_start, *peak_end))
    }

    pub fn inside_windows(&self, start: i64, end: i64) -> bool {
        self.windows
            .iter()
            .any(|(window_start, window_end)| start >= *window_start && end <= *window_end)
    }
}

/// 为 horizon 内每一天建底板。
pub fn build_day_frames(plan_context: &PlanContext) -> Vec<DayFrame> {
    let Ok(start) = context::parse_date(&plan_context.horizon_start) else {
        return Vec::new();
    };
    let meals = meal_ranges(plan_context);

    context::horizon_dates(start, plan_context.horizon_days)
        .into_iter()
        .map(|date| {
            let date_key = context::date_string(date);
            let earliest = earliest_start(plan_context.clock.as_ref(), &date_key);
            let windows = clip_start(
                &context::windows_for_date(&plan_context.available_windows, date),
                earliest,
            );
            let peaks = clip_start(
                &context::peaks_for_date(&plan_context.peak_windows, date),
                earliest,
            );

            let kept: Vec<&ContextBlock> = plan_context
                .existing_blocks
                .iter()
                .filter(|block| block.date == date_key && !block.replaceable)
                .collect();
            let mut fixed: Vec<(i64, i64)> = kept
                .iter()
                .map(|block| (block.start_minute, block.end_minute))
                .collect();
            fixed.extend(meals.iter().copied());
            let existing_study_minutes = kept
                .iter()
                .filter(|block| !is_meal_title(&block.title, plan_context))
                .map(|block| (block.end_minute - block.start_minute).max(0))
                .sum();

            DayFrame {
                date,
                date_key,
                windows,
                peaks,
                fixed,
                existing_study_minutes,
                past: earliest >= DAY_MINUTES,
            }
        })
        .collect()
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

    fn block(date: &str, start: i64, end: i64, title: &str, replaceable: bool) -> ContextBlock {
        ContextBlock {
            block_id: start,
            date: date.to_string(),
            start_minute: start,
            end_minute: end,
            title: title.to_string(),
            locked: !replaceable,
            replaceable,
        }
    }

    /// 2026-09-30 是周三，08:00–22:00 可用。
    fn context_on_wednesday(clock: Option<PlanClock>) -> PlanContext {
        PlanContext {
            generated_at: String::new(),
            horizon_days: 2,
            horizon_start: "2026-09-30".to_string(),
            horizon_end: "2026-10-01".to_string(),
            queue_items: Vec::new(),
            existing_blocks: Vec::new(),
            available_windows: vec![window(3, 480, 1320), window(4, 480, 1320)],
            peak_windows: vec![window(3, 480, 660)],
            min_break_minutes: 10,
            max_daily_minutes: 480,
            default_block_minutes: 45,
            planner_preferences: AiPlannerPreferences::default(),
            clock,
            already_scheduled: Vec::new(),
            replaceable_block_ids: Vec::new(),
            source_request: None,
        }
    }

    #[test]
    fn today_is_clipped_to_now_plus_buffer() {
        let plan_context = context_on_wednesday(Some(PlanClock {
            date: "2026-09-30".to_string(),
            minute: 14 * 60 + 23,
        }));
        let frames = build_day_frames(&plan_context);

        // 14:23 + 10 分钟缓冲 → 向上对齐到 14:35。
        assert_eq!(frames[0].windows, vec![(875, 1320)]);
        // 高效时段 08:00–11:00 已经整段过去。
        assert!(frames[0].peaks.is_empty());
        // 明天不受影响。
        assert_eq!(frames[1].windows, vec![(480, 1320)]);
    }

    #[test]
    fn past_days_have_no_room() {
        let plan_context = context_on_wednesday(Some(PlanClock {
            date: "2026-10-01".to_string(),
            minute: 60,
        }));
        let frames = build_day_frames(&plan_context);

        assert!(frames[0].past);
        assert!(frames[0].windows.is_empty());
        assert!(!frames[1].past);
    }

    #[test]
    fn free_gaps_skip_meals_blocks_and_keep_break() {
        let mut plan_context = context_on_wednesday(None);
        plan_context.existing_blocks = vec![block("2026-09-30", 600, 660, "英语真题", false)];
        let frames = build_day_frames(&plan_context);
        let gaps = frames[0].free_gaps(&[], break_minutes(&plan_context));

        // 早餐 07:00–07:40 在窗口外；10:00–11:00 已有日程；12:00–13:00 午餐；18:00–19:00 晚餐。
        // 每个占用区间两侧各留 10 分钟休息。
        assert_eq!(
            gaps,
            vec![(480, 590), (670, 710), (790, 1070), (1150, 1320)]
        );
        assert_eq!(frames[0].existing_study_minutes, 60);
    }

    #[test]
    fn replaceable_blocks_do_not_occupy_space_or_capacity() {
        let mut plan_context = context_on_wednesday(None);
        plan_context.planner_preferences.auto_meals = false;
        plan_context.existing_blocks = vec![block("2026-09-30", 480, 600, "AI 上次排的", true)];
        let frames = build_day_frames(&plan_context);

        assert_eq!(frames[0].free_gaps(&[], 10), vec![(480, 1320)]);
        assert_eq!(frames[0].existing_study_minutes, 0);
    }

    #[test]
    fn meal_blocks_do_not_count_toward_study_capacity() {
        let mut plan_context = context_on_wednesday(None);
        plan_context.existing_blocks = vec![block("2026-09-30", 720, 780, "午餐", false)];
        let frames = build_day_frames(&plan_context);

        assert_eq!(frames[0].existing_study_minutes, 0);
        assert_eq!(frames[0].capacity_left(120, 480), 360);
    }

    #[test]
    fn rest_style_adjusts_break() {
        let mut plan_context = context_on_wednesday(None);
        plan_context.planner_preferences.rest_style = "gentle".to_string();
        assert_eq!(break_minutes(&plan_context), 15);
        plan_context.planner_preferences.rest_style = "focused".to_string();
        assert_eq!(break_minutes(&plan_context), 5);
    }

    #[test]
    fn alignment_helpers_round_in_the_right_direction() {
        assert_eq!(align_up(481), 485);
        assert_eq!(align_up(480), 480);
        assert_eq!(align_down(484), 480);
    }
}
