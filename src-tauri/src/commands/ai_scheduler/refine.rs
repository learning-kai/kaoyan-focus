//! 可行性修复：模型负责「排什么、大概什么时候」，这里负责「一定排得进去」。
//!
//! 旧实现把模型给出的不可行时间（撞了三餐 / 已有日程 / 已经过去 / 超出单日上限）一律丢弃，
//! 模型漏掉的条目则直接消失——用户看到的就是「明明还有空，AI 却排不下」。现在的规则：
//!
//! 1. 可行的原样保留；
//! 2. 不可行的挪到最近的空档：同一天优先，其次之后几天，截止日前的更早日期兜底；
//!    整段放不下就拆成至多 `MAX_SEGMENTS_PER_ITEM` 段，未估时条目还可以适当压缩；
//! 3. 模型漏掉的条目用同一规则补排；
//! 4. 真的放不下才进 `unscheduled`，并说清楚卡在哪里。
//!
//! 本地规则排期就是「模型什么都没给」的特例（`Mode::Local`），两条路径因此共用一个可行性标准。

use super::context;
use super::models::*;
use super::slots::{self, DayFrame};
use super::validator;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 修复模型输出：挪动与补排都要告诉用户。
    Llm,
    /// 本地规则排期：所有条目都由这里放置，不产生「已自动调整」提示。
    Local,
}

pub struct RefineOutcome {
    pub response: RawPlanResponse,
    /// `adjusted` / `unknown_task` / `duplicate`。
    pub warnings: Vec<AiPlanWarning>,
}

/// 拆分预算的下限：同一条目各时段加起来允许的最短上限。
const SEGMENT_BUDGET_FLOOR_MINUTES: i64 = 120;

fn priority_rank(priority: &str) -> i64 {
    match priority {
        "high" => 0,
        "low" => 2,
        // medium 与脏数据都按中间档，不打乱顺序。
        _ => 1,
    }
}

/// 需要深度思考的科目：优先放进高效时段。
pub fn heavy_subject(item: &ContextQueueItem) -> bool {
    matches!(item.category_key.as_str(), "math" | "major")
}

/// 条目应占用的时长。用户填了预计时长就照办；未估时条目在自适应模式下按科目给一个合理值。
pub fn item_minutes(item: &ContextQueueItem, plan_context: &PlanContext) -> i64 {
    let raw = item.effective_minutes(plan_context.default_block_minutes);
    if !plan_context.planner_preferences.adaptive_durations || item.estimated_minutes > 0 {
        return slots::align_up(raw).max(slots::TIME_ALIGN_MINUTES);
    }
    let (min, max) = match item.category_key.as_str() {
        "math" | "major" => (45, 90),
        "english" | "politics" => (25, 60),
        _ => (25, 45),
    };
    slots::align_down(raw.clamp(min, max)).max(slots::TIME_ALIGN_MINUTES)
}

/// 同一条目所有时段加起来的上限。超出部分视为模型重复排期。
///
/// 校验器用同一个函数判定 `duplicate`，保证这里保留的拆分段不会在校验时被误删。
pub fn segment_budget(item: &ContextQueueItem, plan_context: &PlanContext) -> i64 {
    let expected = item_minutes(item, plan_context)
        .max(item.effective_minutes(plan_context.default_block_minutes));
    (expected * 5 / 4).max(SEGMENT_BUDGET_FLOOR_MINUTES)
}

/// 未估时且开启自适应时，允许为了塞进空档把时长压到原来的 2/3。用户估过时的条目不动。
fn can_shrink(item: &ContextQueueItem, plan_context: &PlanContext) -> bool {
    plan_context.planner_preferences.adaptive_durations && item.estimated_minutes <= 0
}

/// 补排 / 挪动的先后：优先级 → 错过次数 → 截止日 → 重科目 → 长任务 → id。
pub fn placement_order<'a>(
    items: impl IntoIterator<Item = &'a ContextQueueItem>,
    respect_priority: bool,
) -> Vec<&'a ContextQueueItem> {
    let mut ordered: Vec<&ContextQueueItem> = items.into_iter().collect();
    ordered.sort_by(|a, b| {
        let key = |item: &ContextQueueItem| {
            (
                if respect_priority {
                    priority_rank(&item.priority)
                } else {
                    0
                },
                -item.missed_count,
                item.due_date.clone().unwrap_or_else(|| "9999-12-31".to_string()),
                !heavy_subject(item),
                -item.estimated_minutes,
                item.item_id,
            )
        };
        key(a).cmp(&key(b))
    });
    ordered
}

fn short_date(date_key: &str) -> &str {
    date_key.get(5..).unwrap_or(date_key)
}

fn hhmm(minute: i64) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// 放置结果：一段或多段（同一天），以及是否为了塞进空档压缩过时长。
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub frame_index: usize,
    pub segments: Vec<(i64, i64)>,
    pub shrunk_from: Option<i64>,
}

/// 排期棋盘：每天的底板 + 本次已经放进去的区间。
pub struct Board<'a> {
    pub plan_context: &'a PlanContext,
    pub frames: Vec<DayFrame>,
    placed: Vec<Vec<(i64, i64)>>,
    break_minutes: i64,
}

impl<'a> Board<'a> {
    pub fn new(plan_context: &'a PlanContext) -> Self {
        let frames = slots::build_day_frames(plan_context);
        let placed = vec![Vec::new(); frames.len()];
        Self {
            plan_context,
            frames,
            placed,
            break_minutes: slots::break_minutes(plan_context),
        }
    }

    pub fn frame_index(&self, date_key: &str) -> Option<usize> {
        self.frames.iter().position(|frame| frame.date_key == date_key)
    }

    fn capacity_left(&self, index: usize) -> i64 {
        let placed_minutes: i64 = self.placed[index].iter().map(|(s, e)| e - s).sum();
        self.frames[index].capacity_left(placed_minutes, self.plan_context.max_daily_minutes)
    }

    fn gaps(&self, index: usize) -> Vec<(i64, i64)> {
        self.frames[index].free_gaps(&self.placed[index], self.break_minutes)
    }

    /// 区间完整落在某个空档内且不超当天容量。空档已扣掉固定安排、已放条目与休息间隔。
    fn fits(&self, index: usize, start: i64, end: i64) -> bool {
        end > start
            && end - start <= self.capacity_left(index)
            && self
                .gaps(index)
                .iter()
                .any(|(gap_start, gap_end)| start >= *gap_start && end <= *gap_end)
    }

    fn reserve(&mut self, index: usize, start: i64, end: i64) {
        self.placed[index].push((start, end));
    }

    pub fn commit(&mut self, placement: &Placement) {
        for (start, end) in &placement.segments {
            self.reserve(placement.frame_index, *start, *end);
        }
    }

    /// 截止日：还来得及时是硬约束；horizon 里第一个可排日都已经晚于截止日时，
    /// 说明已经逾期，这时不设约束，改为尽早安排（逾期不该让任务从计划里消失）。
    fn strict_deadline<'i>(&self, item: &'i ContextQueueItem) -> Option<&'i str> {
        let due = item.due_date.as_deref()?;
        let first_open = self.frames.iter().find(|frame| !frame.past)?;
        (first_open.date_key.as_str() <= due).then_some(due)
    }

    /// 候选日期。`late = false` 只取截止日（含）之前；`late = true` 只取之后（逾期兜底）。
    /// 有偏好日期时：偏好日 → 之后几天 → 之前几天；否则按日期先后。
    fn candidate_days(&self, deadline: Option<&str>, preferred: Option<usize>, late: bool) -> Vec<usize> {
        let allowed = |index: usize| {
            let frame = &self.frames[index];
            if frame.past {
                return false;
            }
            match (deadline, late) {
                (Some(due), false) => frame.date_key.as_str() <= due,
                (Some(due), true) => frame.date_key.as_str() > due,
                (None, false) => true,
                (None, true) => false,
            }
        };
        let order: Vec<usize> = match preferred {
            Some(pivot) if pivot < self.frames.len() => std::iter::once(pivot)
                .chain(pivot + 1..self.frames.len())
                .chain((0..pivot).rev())
                .collect(),
            _ => (0..self.frames.len()).collect(),
        };
        order.into_iter().filter(|index| allowed(*index)).collect()
    }

    /// 在某一天找一个能放下 `duration` 的起点。
    ///
    /// - 给了 `near`：取离它最近的可行起点（修复模型输出时尽量少挪）；
    /// - `prefer_peak`：优先落在高效时段里（从高效时段开头开始，避免把前面的空档切碎）；
    /// - 否则：按时间先后，最早的可行起点。
    fn slot_in_day(&self, index: usize, duration: i64, near: Option<i64>, prefer_peak: bool) -> Option<i64> {
        if duration <= 0 || duration > self.capacity_left(index) {
            return None;
        }
        let gaps: Vec<(i64, i64)> = self
            .gaps(index)
            .into_iter()
            .filter(|(start, end)| end - start >= duration)
            .collect();
        if let Some(target) = near {
            return gaps
                .iter()
                .map(|(start, end)| slots::align_down(target.clamp(*start, end - duration)).max(*start))
                .min_by_key(|start| ((start - target).abs(), *start));
        }
        let frame = &self.frames[index];
        let mut candidates: Vec<(bool, i64)> = gaps
            .iter()
            .map(|(start, end)| {
                let latest = end - duration;
                let peak_start = prefer_peak
                    .then(|| {
                        frame
                            .peaks
                            .iter()
                            .find(|(peak_start, peak_end)| slots::overlaps(*start, *end, *peak_start, *peak_end))
                            .map(|(peak_start, _)| slots::align_up(*peak_start).clamp(*start, latest))
                    })
                    .flatten();
                let begin = peak_start.unwrap_or(*start);
                let in_peak = prefer_peak && frame.touches_peak(begin, begin + duration);
                (!in_peak, begin)
            })
            .collect();
        candidates.sort_unstable();
        candidates.first().map(|(_, start)| *start)
    }

    /// 在某一天把 `duration` 拆成 2..=MAX 段塞进不同空档。优先均分，均分放不下再贪心。
    fn split_in_day(&self, index: usize, duration: i64) -> Option<Vec<(i64, i64)>> {
        if duration < MIN_SEGMENT_MINUTES * 2 || duration > self.capacity_left(index) {
            return None;
        }
        let gaps: Vec<(i64, i64)> = self
            .gaps(index)
            .into_iter()
            .filter(|(start, end)| end - start >= MIN_SEGMENT_MINUTES)
            .collect();
        for pieces in 2..=MAX_SEGMENTS_PER_ITEM as i64 {
            let even = slots::align_up((duration + pieces - 1) / pieces);
            for cap in [Some(even), None] {
                if let Some(segments) = fill_gaps(&gaps, duration, cap) {
                    if (segments.len() as i64) <= pieces && segments.len() > 1 {
                        return Some(segments);
                    }
                }
            }
        }
        None
    }

    /// 为了塞进当天最大的空档，把未估时条目压到不短于原时长的 2/3。
    fn shrink_in_day(&self, index: usize, duration: i64) -> Option<(i64, i64)> {
        let floor = slots::align_up(duration * 2 / 3).max(MIN_SEGMENT_MINUTES);
        let capacity = self.capacity_left(index);
        self.gaps(index)
            .into_iter()
            .map(|(start, end)| (start, slots::align_down((end - start).min(capacity).min(duration))))
            .filter(|(_, length)| *length >= floor && *length < duration)
            .max_by_key(|(start, length)| (*length, -*start))
            .map(|(start, length)| (start, start + length))
    }

    /// 找放置位置：先整段，再拆段，最后（允许时）压缩；截止日前都放不下才考虑逾期日期。
    pub fn find_placement(
        &self,
        item: &ContextQueueItem,
        duration: i64,
        preferred: Option<(usize, i64)>,
        allow_split: bool,
    ) -> Option<Placement> {
        let deadline = self.strict_deadline(item);
        let prefer_peak = heavy_subject(item) || item.priority == "high";
        for late in [false, true] {
            let days = self.candidate_days(deadline, preferred.map(|(day, _)| day), late);
            // 模型给过时刻就在每个候选日都尽量贴近它（它可能在遵守「上午做数学」之类的要求）。
            let near = preferred.map(|(_, minute)| minute);
            for &day in &days {
                if let Some(start) = self.slot_in_day(day, duration, near, prefer_peak) {
                    return Some(Placement {
                        frame_index: day,
                        segments: vec![(start, start + duration)],
                        shrunk_from: None,
                    });
                }
            }
            if allow_split {
                for &day in &days {
                    if let Some(segments) = self.split_in_day(day, duration) {
                        return Some(Placement {
                            frame_index: day,
                            segments,
                            shrunk_from: None,
                        });
                    }
                }
            }
            if can_shrink(item, self.plan_context) {
                for &day in &days {
                    if let Some(segment) = self.shrink_in_day(day, duration) {
                        return Some(Placement {
                            frame_index: day,
                            segments: vec![segment],
                            shrunk_from: Some(duration),
                        });
                    }
                }
            }
        }
        None
    }

    /// 模型给的时间为什么不能用。返回 `None` 表示可行。
    fn diagnose(&self, index: usize, start: i64, end: i64, deadline: Option<&str>) -> Option<String> {
        let frame = &self.frames[index];
        if frame.past {
            return Some("日期已经过去".to_string());
        }
        if let Some(due) = deadline {
            if frame.date_key.as_str() > due {
                return Some(format!("晚于截止日 {}", short_date(due)));
            }
        }
        if !frame.inside_windows(start, end) {
            let original = context::windows_for_date(&self.plan_context.available_windows, frame.date);
            let inside_original = original
                .iter()
                .any(|(window_start, window_end)| start >= *window_start && end <= *window_end);
            let reason = if inside_original {
                "时间已经过去"
            } else {
                "不在可用时段内"
            };
            return Some(reason.to_string());
        }
        if slots::meal_ranges(self.plan_context)
            .iter()
            .any(|(meal_start, meal_end)| slots::overlaps(start, end, *meal_start, *meal_end))
        {
            return Some("与用餐时间冲突".to_string());
        }
        if let Some(block) = self.plan_context.existing_blocks.iter().find(|block| {
            block.date == frame.date_key
                && !block.replaceable
                && slots::overlaps(start, end, block.start_minute, block.end_minute)
        }) {
            return Some(format!("与已有日程「{}」冲突", block.title));
        }
        if end - start > self.capacity_left(index) {
            return Some("超出当天学习上限".to_string());
        }
        (!self.fits(index, start, end)).then(|| "与相邻安排重叠或没留休息".to_string())
    }

    /// 放不下时给用户的具体原因。
    fn failure_reason(&self, item: &ContextQueueItem, duration: i64) -> String {
        let open: Vec<usize> = (0..self.frames.len())
            .filter(|index| !self.frames[*index].past && !self.frames[*index].windows.is_empty())
            .collect();
        if open.is_empty() {
            return "规划范围内已经没有可用时段".to_string();
        }
        if let Some(due) = self.strict_deadline(item) {
            if open.iter().all(|index| self.frames[*index].date_key.as_str() > due) {
                return format!("截止日 {} 前没有可用时段", short_date(due));
            }
        }
        if open.iter().all(|index| self.capacity_left(*index) < MIN_SEGMENT_MINUTES.min(duration)) {
            return "已达到每天的学习上限".to_string();
        }
        let longest = open
            .iter()
            .flat_map(|index| self.gaps(*index))
            .map(|(start, end)| end - start)
            .max()
            .unwrap_or(0);
        format!("需要 {duration} 分钟，最长的空档只有 {longest} 分钟")
    }
}

/// 按时间先后把 `duration` 填进空档，每段不短于 `MIN_SEGMENT_MINUTES`、不长于 `cap`。
fn fill_gaps(gaps: &[(i64, i64)], duration: i64, cap: Option<i64>) -> Option<Vec<(i64, i64)>> {
    let mut remaining = duration;
    let mut segments: Vec<(i64, i64)> = Vec::new();
    for (start, end) in gaps {
        if remaining <= 0 || segments.len() >= MAX_SEGMENTS_PER_ITEM {
            break;
        }
        let mut chunk = slots::align_down((end - start).min(remaining).min(cap.unwrap_or(i64::MAX)));
        let leftover = remaining - chunk;
        if leftover > 0 && leftover < MIN_SEGMENT_MINUTES {
            chunk -= MIN_SEGMENT_MINUTES - leftover;
        }
        if chunk < MIN_SEGMENT_MINUTES {
            continue;
        }
        segments.push((*start, start + chunk));
        remaining -= chunk;
    }
    (remaining == 0).then_some(segments)
}

fn describe_segments(segments: &[(i64, i64)], date_key: Option<&str>) -> String {
    let times = segments
        .iter()
        .map(|(start, end)| format!("{}-{}", hhmm(*start), hhmm(*end)))
        .collect::<Vec<_>>()
        .join("、");
    match date_key {
        Some(date) => format!("{} {times}", short_date(date)),
        None => times,
    }
}

/// 规则化的排期理由：只陈述实际用到的规则，不编造。≤40 字。
fn local_rationale(item: &ContextQueueItem, board: &Board<'_>, placement: &Placement) -> String {
    let frame = &board.frames[placement.frame_index];
    let (start, end) = placement.segments[0];
    if placement.segments.len() > 1 {
        return format!("没有整段空档，拆成 {} 段，中间留出休息", placement.segments.len());
    }
    if placement.shrunk_from.is_some() {
        return "未估时，按空档长度压缩到可完成的量".to_string();
    }
    if let Some(due) = item.due_date.as_deref() {
        if due < frame.date_key.as_str() {
            return "已逾期，尽早补上".to_string();
        }
        if due == frame.date_key {
            return "今天截止，优先完成".to_string();
        }
    }
    if item.missed_count > 0 {
        return format!("之前错过 {} 次，放在更容易开始的时段", item.missed_count);
    }
    if heavy_subject(item) && frame.touches_peak(start, end) {
        return "需要深度思考，放进高效时段".to_string();
    }
    if item.priority == "high" {
        return "高优先级，先占靠前的时段".to_string();
    }
    "按优先级依次排入空档，前后留出休息".to_string()
}

struct Candidate {
    item_id: i64,
    date: Option<String>,
    bounds: Option<(i64, i64)>,
    duration: i64,
    rationale: Option<String>,
}

struct Pending {
    minutes: i64,
    preferred: Option<(usize, i64)>,
    original: String,
    reason: String,
}

/// 第 0 步：去掉编造的 id 与超预算的重复段，把时间规范成 5 分钟对齐。
fn collect_candidates(
    raw: &RawPlanResponse,
    plan_context: &PlanContext,
    warnings: &mut Vec<AiPlanWarning>,
) -> Vec<Candidate> {
    let mut totals: BTreeMap<i64, (usize, i64)> = BTreeMap::new();
    let mut candidates = Vec::new();
    for raw_item in &raw.items {
        let Some(item) = plan_context.item_by_id(raw_item.item_id) else {
            warnings.push(
                AiPlanWarning::new(
                    WARN_UNKNOWN_TASK,
                    format!("模型给出的条目 {} 不在本次排期范围内，已忽略", raw_item.item_id),
                )
                .for_queue_item(Some(raw_item.item_id)),
            );
            continue;
        };
        let fallback = item_minutes(item, plan_context);
        let bounds = validator::normalize_bounds(raw_item.start_minute, raw_item.end_minute, fallback);
        let duration = bounds.map(|(start, end)| end - start).unwrap_or(fallback);
        let (count, used) = totals.entry(item.item_id).or_insert((0, 0));
        if *count > 0 && (*count >= MAX_SEGMENTS_PER_ITEM || *used + duration > segment_budget(item, plan_context)) {
            warnings.push(
                AiPlanWarning::new(
                    WARN_DUPLICATE,
                    format!("「{}」被重复安排，多出的时段已去掉", item.title),
                )
                .for_queue_item(Some(item.item_id)),
            );
            continue;
        }
        *count += 1;
        *used += duration;
        candidates.push(Candidate {
            item_id: item.item_id,
            date: context::parse_date(&raw_item.date).ok().map(context::date_string),
            bounds,
            duration,
            rationale: raw_item.rationale.clone(),
        });
    }
    candidates
}

fn push_placement(
    items: &mut Vec<RawPlanItem>,
    board: &Board<'_>,
    item: &ContextQueueItem,
    placement: &Placement,
    rationale: String,
) {
    let date_key = board.frames[placement.frame_index].date_key.clone();
    for (start, end) in &placement.segments {
        items.push(RawPlanItem {
            item_id: item.item_id,
            date: date_key.clone(),
            start_minute: *start,
            end_minute: *end,
            rationale: Some(rationale.clone()),
        });
    }
}

/// 「已自动调整」提示挂在第一段上，前端据此显示徽标。
fn adjusted_warning(board: &Board<'_>, item: &ContextQueueItem, placement: &Placement, message: String) -> AiPlanWarning {
    let date_key = &board.frames[placement.frame_index].date_key;
    let first_start = placement.segments[0].0;
    AiPlanWarning::new(WARN_ADJUSTED, message)
        .for_queue_item(Some(item.item_id))
        .for_item(Some(validator::item_id_for(date_key, first_start, item.item_id)))
}

fn placement_text(board: &Board<'_>, placement: &Placement) -> String {
    let date_key = &board.frames[placement.frame_index].date_key;
    let date = (board.frames.len() > 1).then_some(date_key.as_str());
    let mut text = describe_segments(&placement.segments, date);
    if placement.segments.len() > 1 {
        text.push_str(&format!("（拆成 {} 段）", placement.segments.len()));
    }
    if let Some(original) = placement.shrunk_from {
        let now: i64 = placement.segments.iter().map(|(s, e)| e - s).sum();
        text.push_str(&format!("（由 {original} 分钟压缩为 {now} 分钟）"));
    }
    text
}

/// 主入口。`respect_priority` 决定挪动 / 补排时是否按优先级先后。
pub fn make_feasible(
    raw: &RawPlanResponse,
    plan_context: &PlanContext,
    respect_priority: bool,
    mode: Mode,
) -> RefineOutcome {
    let mut board = Board::new(plan_context);
    let mut warnings: Vec<AiPlanWarning> = Vec::new();
    let mut items: Vec<RawPlanItem> = Vec::new();
    let mut unscheduled: Vec<RawUnscheduledItem> = Vec::new();

    let mut candidates = collect_candidates(raw, plan_context, &mut warnings);
    // 按时间先后逐条落位：前面的安排先占位，尽量保留模型的整体顺序。
    candidates.sort_by(|a, b| (&a.date, a.bounds).cmp(&(&b.date, b.bounds)));

    // 第 1 步：可行的原样保留，不可行的记下来。
    let mut pending: BTreeMap<i64, Pending> = BTreeMap::new();
    let mut kept_segments: BTreeMap<i64, usize> = BTreeMap::new();
    for candidate in candidates {
        let Some(item) = plan_context.item_by_id(candidate.item_id) else {
            continue;
        };
        let deadline = board.strict_deadline(item);
        let index = candidate.date.as_deref().and_then(|date| board.frame_index(date));
        let problem = match (candidate.date.as_deref(), index, candidate.bounds) {
            (None, _, _) => "日期无法识别".to_string(),
            (Some(_), None, _) => "不在本次规划范围内".to_string(),
            (Some(_), Some(_), None) => "时间区间不合法".to_string(),
            (Some(_), Some(index), Some((start, end))) => match board.diagnose(index, start, end, deadline) {
                None => {
                    board.reserve(index, start, end);
                    *kept_segments.entry(item.item_id).or_insert(0) += 1;
                    items.push(RawPlanItem {
                        item_id: item.item_id,
                        date: candidate.date.clone().unwrap_or_default(),
                        start_minute: start,
                        end_minute: end,
                        rationale: candidate.rationale.clone(),
                    });
                    continue;
                }
                Some(problem) => problem,
            },
        };

        let original = match (candidate.date.as_deref(), candidate.bounds) {
            (Some(date), Some((start, end))) => {
                format!("原定 {} {}-{}", short_date(date), hhmm(start), hhmm(end))
            }
            _ => "原定时间".to_string(),
        };
        let preferred = index.map(|index| (index, candidate.bounds.map(|(start, _)| start).unwrap_or(0)));
        pending
            .entry(item.item_id)
            .and_modify(|entry| entry.minutes += candidate.duration)
            .or_insert(Pending {
                minutes: candidate.duration,
                preferred,
                original,
                reason: problem,
            });
    }

    // 第 2 步：把不可行的挪到最近的空档。已经保留了分段的条目不再拆，避免总段数失控。
    for item in placement_order(
        plan_context.queue_items.iter().filter(|item| pending.contains_key(&item.item_id)),
        respect_priority,
    ) {
        let entry = &pending[&item.item_id];
        let allow_split = !kept_segments.contains_key(&item.item_id);
        // 与校验器同一口径：已保留段 + 挪动段不得超过拆分预算，否则拆出来的段会被当成重复删掉。
        let kept_minutes: i64 = items
            .iter()
            .filter(|kept| kept.item_id == item.item_id)
            .map(|kept| kept.end_minute - kept.start_minute)
            .sum();
        let budget_left = slots::align_down(segment_budget(item, plan_context) - kept_minutes);
        let minutes = entry.minutes.min(budget_left).max(slots::TIME_ALIGN_MINUTES);
        match board.find_placement(item, minutes, entry.preferred, allow_split) {
            Some(placement) => {
                let rationale = local_rationale(item, &board, &placement);
                if mode == Mode::Llm {
                    let message = format!(
                        "「{}」{}，{}，已调整到 {}",
                        item.title,
                        entry.original,
                        entry.reason,
                        placement_text(&board, &placement)
                    );
                    warnings.push(adjusted_warning(&board, item, &placement, message));
                }
                push_placement(&mut items, &board, item, &placement, rationale);
                board.commit(&placement);
            }
            None => unscheduled.push(RawUnscheduledItem {
                item_id: item.item_id,
                reason: Some(format!(
                    "{}，{}，也找不到别的空档：{}",
                    entry.original,
                    entry.reason,
                    board.failure_reason(item, minutes)
                )),
            }),
        }
    }

    // 第 3 步：模型漏掉的条目按同一规则补排。
    let mentioned: BTreeSet<i64> = raw
        .items
        .iter()
        .map(|item| item.item_id)
        .chain(raw.unscheduled.iter().map(|item| item.item_id))
        .collect();
    for item in placement_order(
        plan_context.queue_items.iter().filter(|item| !mentioned.contains(&item.item_id)),
        respect_priority,
    ) {
        let minutes = item_minutes(item, plan_context);
        match board.find_placement(item, minutes, None, true) {
            Some(placement) => {
                let rationale = local_rationale(item, &board, &placement);
                if mode == Mode::Llm {
                    let message = format!(
                        "「{}」被模型漏排，已自动补到 {}",
                        item.title,
                        placement_text(&board, &placement)
                    );
                    warnings.push(adjusted_warning(&board, item, &placement, message));
                }
                push_placement(&mut items, &board, item, &placement, rationale);
                board.commit(&placement);
            }
            None => unscheduled.push(RawUnscheduledItem {
                item_id: item.item_id,
                reason: Some(board.failure_reason(item, minutes)),
            }),
        }
    }

    // 第 4 步：模型明确说「排不下」的尊重它（可能是在遵守用户的「今天少排点」），
    // 只去掉编造的 id 和实际已经排上的条目。
    let placed_ids: BTreeSet<i64> = items.iter().map(|item| item.item_id).collect();
    for entry in &raw.unscheduled {
        let known = plan_context.item_by_id(entry.item_id).is_some();
        let listed = unscheduled.iter().any(|existing| existing.item_id == entry.item_id);
        if known && !listed && !placed_ids.contains(&entry.item_id) {
            unscheduled.push(entry.clone());
        }
    }

    items.sort_by(|a, b| (&a.date, a.start_minute, a.item_id).cmp(&(&b.date, b.start_minute, b.item_id)));
    RefineOutcome {
        response: RawPlanResponse {
            summary: raw
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().take(60).collect()),
            items,
            unscheduled,
        },
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = "2026-09-30";
    const NEXT_DAY: &str = "2026-10-01";

    fn window(weekday: i64, start: i64, end: i64) -> AiTimeWindow {
        AiTimeWindow {
            weekday,
            start_minute: start,
            end_minute: end,
        }
    }

    fn item(item_id: i64, category: &str, priority: &str, minutes: i64) -> ContextQueueItem {
        ContextQueueItem {
            item_id,
            source_task_id: None,
            title: format!("条目{item_id}"),
            category_key: category.to_string(),
            category_label: category.to_string(),
            subject_id: None,
            priority: priority.to_string(),
            estimated_minutes: minutes,
            due_date: None,
            note: None,
            missed_count: 0,
        }
    }

    /// 2026-09-30（周三）与 10-01（周四）08:00–22:00 可用，周三高效时段 08:00–11:00，
    /// 默认三餐，休息 10 分钟。无「现在」裁剪。
    fn wednesday(items: Vec<ContextQueueItem>) -> PlanContext {
        PlanContext {
            generated_at: String::new(),
            horizon_days: 1,
            horizon_start: DAY.to_string(),
            horizon_end: DAY.to_string(),
            queue_items: items,
            existing_blocks: Vec::new(),
            available_windows: vec![window(3, 480, 1320), window(4, 480, 1320)],
            peak_windows: vec![window(3, 480, 660)],
            min_break_minutes: 10,
            max_daily_minutes: 480,
            default_block_minutes: 45,
            planner_preferences: AiPlannerPreferences::default(),
            clock: None,
            already_scheduled: Vec::new(),
            replaceable_block_ids: Vec::new(),
            source_request: None,
        }
    }

    fn two_days(mut plan_context: PlanContext) -> PlanContext {
        plan_context.horizon_days = 2;
        plan_context.horizon_end = NEXT_DAY.to_string();
        plan_context
    }

    fn raw_item(item_id: i64, date: &str, start: i64, end: i64) -> RawPlanItem {
        RawPlanItem {
            item_id,
            date: date.to_string(),
            start_minute: start,
            end_minute: end,
            rationale: None,
        }
    }

    fn raw(items: Vec<RawPlanItem>) -> RawPlanResponse {
        RawPlanResponse {
            summary: None,
            items,
            unscheduled: Vec::new(),
        }
    }

    fn adjusted(outcome: &RefineOutcome) -> Vec<&AiPlanWarning> {
        outcome
            .warnings
            .iter()
            .filter(|warning| warning.code == WARN_ADJUSTED)
            .collect()
    }

    #[test]
    fn feasible_model_output_is_kept_as_is() {
        let plan_context = wednesday(vec![item(1, "math", "high", 90)]);
        let outcome = make_feasible(&raw(vec![raw_item(1, DAY, 480, 570)]), &plan_context, true, Mode::Llm);

        assert_eq!(outcome.response.items, vec![raw_item(1, DAY, 480, 570)]);
        assert!(outcome.warnings.is_empty());
        assert!(outcome.response.unscheduled.is_empty());
    }

    #[test]
    fn item_clashing_with_lunch_moves_to_nearest_gap_instead_of_being_dropped() {
        let plan_context = wednesday(vec![item(1, "english", "medium", 60)]);
        // 12:30–13:30 撞了午餐 12:00–13:00。
        let outcome = make_feasible(&raw(vec![raw_item(1, DAY, 750, 810)]), &plan_context, true, Mode::Llm);

        // 午餐外扩休息后占 11:50–13:10，离 12:30 最近的可行起点是 13:10。
        assert_eq!(outcome.response.items.len(), 1);
        assert_eq!(outcome.response.items[0].start_minute, 790);
        assert_eq!(outcome.response.items[0].end_minute, 850);
        let notes = adjusted(&outcome);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].message.contains("与用餐时间冲突"), "{}", notes[0].message);
        assert_eq!(notes[0].item_id.as_deref(), Some("2026-09-30-790-1"));
    }

    #[test]
    fn slot_in_the_past_is_moved_after_now() {
        let mut plan_context = wednesday(vec![item(1, "english", "medium", 60)]);
        plan_context.clock = Some(PlanClock {
            date: DAY.to_string(),
            minute: 14 * 60,
        });
        let outcome = make_feasible(&raw(vec![raw_item(1, DAY, 540, 600)]), &plan_context, true, Mode::Llm);

        // 14:00 + 10 分钟缓冲 → 14:10 起才能排。
        assert_eq!(outcome.response.items[0].start_minute, 850);
        assert!(adjusted(&outcome)[0].message.contains("时间已经过去"));
    }

    #[test]
    fn omitted_items_are_backfilled_and_flagged() {
        let plan_context = wednesday(vec![item(1, "math", "high", 60), item(2, "english", "medium", 45)]);
        let outcome = make_feasible(&raw(vec![raw_item(1, DAY, 480, 540)]), &plan_context, true, Mode::Llm);

        assert_eq!(outcome.response.items.len(), 2);
        // 条目 1 占 08:00–09:00，加 10 分钟休息 → 条目 2 从 09:10 开始。
        assert_eq!(outcome.response.items[1].item_id, 2);
        assert_eq!(outcome.response.items[1].start_minute, 550);
        assert!(adjusted(&outcome)[0].message.contains("漏排"));
    }

    #[test]
    fn local_mode_places_everything_without_adjusted_notes() {
        let plan_context = wednesday(vec![item(1, "math", "high", 60), item(2, "english", "medium", 45)]);
        let outcome = make_feasible(&RawPlanResponse::default(), &plan_context, true, Mode::Local);

        assert_eq!(outcome.response.items.len(), 2);
        assert!(outcome.warnings.is_empty());
        assert!(outcome.response.items.iter().all(|item| item.rationale.is_some()));
    }

    #[test]
    fn over_capacity_spills_to_next_day() {
        let mut plan_context = two_days(wednesday(vec![item(1, "math", "high", 90), item(2, "math", "high", 90)]));
        plan_context.max_daily_minutes = 120;
        let outcome = make_feasible(
            &raw(vec![raw_item(1, DAY, 480, 570), raw_item(2, DAY, 600, 690)]),
            &plan_context,
            true,
            Mode::Llm,
        );

        assert_eq!(outcome.response.items.len(), 2);
        assert_eq!(outcome.response.items[1].date, NEXT_DAY);
        assert_eq!(outcome.response.items[1].start_minute, 600, "尽量保留模型给的时刻");
        assert!(adjusted(&outcome)[0].message.contains("超出当天学习上限"));
    }

    #[test]
    fn deadline_pulls_item_back_before_due() {
        let mut due_today = item(1, "math", "high", 90);
        due_today.due_date = Some(DAY.to_string());
        let plan_context = two_days(wednesday(vec![due_today]));
        let outcome = make_feasible(&raw(vec![raw_item(1, NEXT_DAY, 480, 570)]), &plan_context, true, Mode::Llm);

        assert_eq!(outcome.response.items[0].date, DAY);
        assert!(adjusted(&outcome)[0].message.contains("晚于截止日"));
    }

    #[test]
    fn long_task_is_split_when_no_single_gap_fits() {
        let mut plan_context = wednesday(vec![item(1, "math", "high", 240)]);
        // 08:00–16:40，中间夹着午餐 → 空档 08:00–11:50 与 13:10–16:40，都放不下整段 240 分钟。
        plan_context.available_windows = vec![window(3, 480, 1000)];
        let outcome = make_feasible(&RawPlanResponse::default(), &plan_context, true, Mode::Local);

        let segments: Vec<(i64, i64)> = outcome
            .response
            .items
            .iter()
            .map(|item| (item.start_minute, item.end_minute))
            .collect();
        assert_eq!(segments, vec![(480, 600), (790, 910)], "均分成两段");
        assert!(outcome.response.unscheduled.is_empty());
        assert!(outcome.response.items[0]
            .rationale
            .as_deref()
            .unwrap_or_default()
            .contains("拆成 2 段"));
    }

    #[test]
    fn unknown_ids_warn_but_do_not_show_up_as_unscheduled() {
        let plan_context = wednesday(vec![item(1, "math", "high", 60)]);
        let mut response = raw(vec![raw_item(999, DAY, 480, 540)]);
        response.unscheduled.push(RawUnscheduledItem {
            item_id: 998,
            reason: None,
        });
        let outcome = make_feasible(&response, &plan_context, true, Mode::Llm);

        assert!(outcome.warnings.iter().any(|warning| warning.code == WARN_UNKNOWN_TASK));
        assert!(outcome.response.unscheduled.is_empty(), "编造的 id 不该冒充「没排上的任务」");
        assert_eq!(outcome.response.items[0].item_id, 1, "真实条目照常补排");
    }

    #[test]
    fn explicit_unscheduled_decision_is_respected() {
        let plan_context = wednesday(vec![item(1, "math", "high", 60)]);
        let response = RawPlanResponse {
            summary: None,
            items: Vec::new(),
            unscheduled: vec![RawUnscheduledItem {
                item_id: 1,
                reason: Some("按要求今天少排".to_string()),
            }],
        };
        let outcome = make_feasible(&response, &plan_context, true, Mode::Llm);

        assert!(outcome.response.items.is_empty());
        assert_eq!(outcome.response.unscheduled[0].reason.as_deref(), Some("按要求今天少排"));
    }

    #[test]
    fn repeated_segments_beyond_budget_are_dropped() {
        let plan_context = wednesday(vec![item(1, "math", "high", 90)]);
        let outcome = make_feasible(
            &raw(vec![raw_item(1, DAY, 480, 570), raw_item(1, DAY, 800, 890)]),
            &plan_context,
            true,
            Mode::Llm,
        );

        assert_eq!(outcome.response.items.len(), 1);
        assert!(outcome.warnings.iter().any(|warning| warning.code == WARN_DUPLICATE));
    }

    #[test]
    fn failure_reason_names_the_bottleneck() {
        let mut plan_context = wednesday(vec![item(1, "math", "high", 200)]);
        plan_context.available_windows = vec![window(3, 480, 600)];
        let outcome = make_feasible(&RawPlanResponse::default(), &plan_context, true, Mode::Local);

        assert!(outcome.response.items.is_empty());
        assert_eq!(
            outcome.response.unscheduled[0].reason.as_deref(),
            Some("需要 200 分钟，最长的空档只有 120 分钟")
        );
    }

    #[test]
    fn heavy_subject_takes_the_peak_window_first() {
        let mut plan_context = wednesday(vec![item(1, "english", "medium", 60), item(2, "math", "medium", 60)]);
        plan_context.planner_preferences.auto_meals = false;
        plan_context.peak_windows = vec![window(3, 840, 960)];
        let outcome = make_feasible(&RawPlanResponse::default(), &plan_context, true, Mode::Local);

        let math = outcome.response.items.iter().find(|item| item.item_id == 2).expect("math");
        let english = outcome.response.items.iter().find(|item| item.item_id == 1).expect("english");
        assert_eq!(math.start_minute, 840, "数学进高效时段 14:00");
        assert_eq!(english.start_minute, 480, "英语按时间先后");
        assert_eq!(math.rationale.as_deref(), Some("需要深度思考，放进高效时段"));
    }

    #[test]
    fn overdue_item_rolls_forward_instead_of_disappearing() {
        let mut overdue = item(1, "math", "high", 60);
        overdue.due_date = Some("2026-09-28".to_string());
        let mut plan_context = wednesday(vec![overdue]);
        plan_context.clock = Some(PlanClock {
            date: DAY.to_string(),
            minute: 8 * 60,
        });
        let outcome = make_feasible(&RawPlanResponse::default(), &plan_context, true, Mode::Local);

        assert_eq!(outcome.response.items.len(), 1);
        assert_eq!(outcome.response.items[0].start_minute, 490, "现在 08:00 + 10 分钟缓冲");
        assert_eq!(outcome.response.items[0].rationale.as_deref(), Some("已逾期，尽早补上"));
    }
}
