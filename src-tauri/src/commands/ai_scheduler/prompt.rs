//! 提示词构建（方案 §5.1 / §5.2 / §5.3）。
//!
//! 三条硬性约束（改提示词时别破坏）：
//! 1. `json_object` 档要求提示词里必须出现 "json" 字样并给出结构示例；
//! 2. 模型只能引用 `queue_items` 里出现过的 `item_id`，标题等字段由校验器回填；
//! 3. 空档由 `slots` 预先算好直接给模型。不要再让模型自己拿「可用时段 − 已有日程 − 三餐」
//!    做区间减法——那是它最容易算错、进而被整条丢弃的地方。

use super::context;
use super::models::{AiPlanItem, AiPlanRequest, PlanContext};
use super::slots::{self, MIN_PROMPT_GAP_MINUTES};
use serde_json::json;

/// 周几的中文显示名，1 = 周一 … 7 = 周日。
const WEEKDAY_NAMES: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

fn format_minute(minute: i64) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn format_ranges(ranges: &[(i64, i64)]) -> String {
    if ranges.is_empty() {
        return "无".to_string();
    }
    ranges
        .iter()
        .map(|(start, end)| format!("{}-{}", format_minute(*start), format_minute(*end)))
        .collect::<Vec<_>>()
        .join("、")
}

fn weekday_name(date: chrono::NaiveDate) -> &'static str {
    WEEKDAY_NAMES[(context::weekday_of(date) - 1).clamp(0, 6) as usize]
}

fn rest_style_label(style: &str) -> &'static str {
    match style {
        "gentle" => "轻松（多留休息）",
        "focused" => "紧凑（少休息）",
        _ => "均衡",
    }
}

/// 「按反馈调整」时附带的上一版草案与用户反馈。
pub struct PlanRevision<'a> {
    pub previous_items: &'a [AiPlanItem],
    pub feedback: &'a str,
}

/// 系统提示词。**对两种结构化输出档位共用同一份**：
/// `json_schema` 档靠 `response_format` 兜底；`json_object` 档靠这里的 "json" 字样与结构示例。
pub fn build_system_prompt() -> String {
    [
        "你是一名考研学生的日程排期助手。你的任务是：把 queue_items 安排进每天给定的空档，输出 json。",
        "",
        "硬性规则（违反的安排会被系统挪走或丢弃）：",
        "1. 只能使用 queue_items 中出现过的 item_id，不得编造 id，也不得改写标题；",
        "2. 每条安排必须完整落在当天列出的某个「空档」内。空档已经扣掉了已有日程、三餐、已经过去的时间和休息间隔，直接用即可；",
        "3. 同一个空档里放多条时，相邻两条之间至少留出休息间隔；",
        "4. 每天新增的学习总时长不得超过当天「还可排」的分钟数；",
        "5. 不得排到 due_date 之后；overdue=true 的条目已经逾期，放进最早的空档；",
        "6. 每个条目只排一次；只有空档不够长时才拆成至多 3 段、每段不少于 25 分钟，各段时长之和等于该条目时长；",
        "7. 实在放不下的条目放进 unscheduled，用一句话说明原因，绝不硬塞；",
        "8. 只输出 json，不要输出任何解释文字、Markdown 代码块或注释。",
        "",
        "排期偏好（在不违反硬性规则的前提下尽量做到）：",
        "- 优先级高、截止日近、missed_count 大（之前排了却没完成）的条目靠前；missed_count 大的条目放在容易开始的时段、时长宜短；",
        "- 数学、专业课这类需要深度思考的放进高效时段；背诵、单词、政治等记忆类放进精力较低的时段；",
        "- 不同科目穿插，同一科目连续不超过 2 小时；",
        "- 每天总量尽量贴近「目标学习时长」，但不要为了凑时长把条目切碎；",
        "- 用户的补充说明优先于以上偏好，但不能违反硬性规则。",
        "",
        "关于时长：adaptive_durations=true 时，estimated_minutes 是基准，可按难度和截止日以 5 分钟为单位调整（通常 25-90 分钟），明显调整时在 rationale 里说明；adaptive_durations=false 时严格使用 estimated_minutes；estimated_minutes=0 表示未估时，使用默认时长。",
        "",
        "输出 json 的结构：",
        r#"{"summary":"上午数学进高效时段，下午英语政治穿插，晚上背单词收尾","items":[{"item_id":41,"date":"2026-09-25","start_minute":480,"end_minute":570,"rationale":"上午头脑清醒，先做数学"}],"unscheduled":[{"item_id":42,"reason":"截止日前没有足够长的空档"}]}"#,
        "",
        "字段说明：start_minute / end_minute 是距当天 00:00 的分钟数（例如 480 = 08:00），5 分钟对齐；",
        "summary 是不超过 60 字的整体安排思路；rationale 是不超过 40 字的单条理由，可以留空字符串；unscheduled 可以是空数组。",
    ]
    .join("\n")
}

/// 用户提示词。把「现在几点 / 每天哪些空档 / 已经占了什么 / 要排什么」一次性给全。
pub fn build_user_prompt(
    request: &AiPlanRequest,
    plan_context: &PlanContext,
    revision: Option<&PlanRevision<'_>>,
) -> String {
    let preferences = &plan_context.planner_preferences;
    let break_minutes = slots::break_minutes(plan_context);
    let frames = slots::build_day_frames(plan_context);
    let mut sections: Vec<String> = Vec::new();

    let mut header = String::from("[目标]\n");
    if let Some(clock) = plan_context.clock.as_ref() {
        header.push_str(&format!(
            "现在是 {} {}，已经过去的时间不能再排。\n",
            clock.date,
            format_minute(clock.minute)
        ));
    }
    header.push_str(&format!(
        "规划 {} 天（{} ~ {}）。相邻安排之间至少休息 {} 分钟；每天学习不超过 {} 分钟，目标学习 {} 分钟；未估时条目默认 {} 分钟。\nadaptive_durations={}；休息节奏：{}。",
        plan_context.horizon_days,
        plan_context.horizon_start,
        plan_context.horizon_end,
        break_minutes,
        plan_context.max_daily_minutes,
        preferences.daily_target_minutes.min(plan_context.max_daily_minutes),
        plan_context.default_block_minutes,
        preferences.adaptive_durations,
        rest_style_label(&preferences.rest_style),
    ));
    if !preferences.memory_note.trim().is_empty() {
        header.push_str(&format!("\n长期偏好：{}", preferences.memory_note.trim()));
    }
    if let Some(instruction) = request.extra_instruction.as_deref() {
        header.push_str(&format!("\n本次补充说明（请尽量遵守）：{instruction}"));
    }
    sections.push(header);

    let mut days = String::from(
        "[每天的空档]\n（空档已扣掉已有日程、三餐、已过去的时间和休息间隔；只能排在这些空档里）",
    );
    for frame in &frames {
        let label = format!("{} {}", frame.date_key, weekday_name(frame.date));
        if frame.past {
            days.push_str(&format!("\n- {label}：已经过去，不可排"));
            continue;
        }
        let gaps: Vec<(i64, i64)> = frame
            .free_gaps(&[], break_minutes)
            .into_iter()
            .filter(|(start, end)| end - start >= MIN_PROMPT_GAP_MINUTES)
            .collect();
        let capacity = frame.capacity_left(0, plan_context.max_daily_minutes);
        if gaps.is_empty() || capacity == 0 {
            days.push_str(&format!("\n- {label}：没有可排的空档"));
            continue;
        }
        let free_minutes: i64 = gaps.iter().map(|(start, end)| end - start).sum();
        days.push_str(&format!(
            "\n- {label}：空档共 {free_minutes} 分钟，还可排 {} 分钟学习；空档 {}；高效时段 {}",
            capacity.min(free_minutes),
            format_ranges(&gaps),
            format_ranges(&frame.peaks),
        ));
    }
    sections.push(days);

    let kept: Vec<String> = plan_context
        .existing_blocks
        .iter()
        .filter(|block| !block.replaceable)
        .map(|block| {
            format!(
                "- {} {}-{} {}",
                block.date,
                format_minute(block.start_minute),
                format_minute(block.end_minute),
                block.title
            )
        })
        .collect();
    sections.push(format!(
        "[已有日程]\n（仅供参考，不可占用）\n{}",
        if kept.is_empty() {
            "（无）".to_string()
        } else {
            kept.join("\n")
        }
    ));

    let first_open = frames
        .iter()
        .find(|frame| !frame.past)
        .map(|frame| frame.date_key.clone());
    let lines: Vec<String> = plan_context
        .queue_items
        .iter()
        .map(|item| {
            let mut entry = json!({
                "item_id": item.item_id,
                "title": item.title,
                "category": item.category_label,
                "priority": item.priority,
                "estimated_minutes": item.estimated_minutes,
            });
            if let Some(due) = item.due_date.as_deref() {
                entry["due_date"] = json!(due);
                if first_open.as_deref().is_some_and(|open| due < open) {
                    entry["overdue"] = json!(true);
                }
            }
            if item.missed_count > 0 {
                entry["missed_count"] = json!(item.missed_count);
            }
            if let Some(note) = item.note.as_deref() {
                entry["note"] = json!(note);
            }
            entry.to_string()
        })
        .collect();
    sections.push(format!(
        "[queue_items]\n（以下 {} 条是要排的全部条目；只能引用这里出现过的 item_id）\n{}",
        plan_context.queue_items.len(),
        lines.join("\n")
    ));

    if let Some(revision) = revision {
        let previous: Vec<String> = revision
            .previous_items
            .iter()
            .filter(|item| item.kind != "meal")
            .filter_map(|item| {
                item.source_today_item_id.map(|item_id| {
                    format!(
                        "- item_id={item_id} {} {} {}-{}",
                        item.title,
                        item.schedule_date,
                        format_minute(item.start_minute),
                        format_minute(item.end_minute)
                    )
                })
            })
            .collect();
        sections.push(format!(
            "[上一版草案]\n{}\n\n[用户对上一版的反馈]\n{}\n请按反馈调整：反馈没提到的部分尽量保持不变，硬性规则仍然必须满足。",
            if previous.is_empty() {
                "（空）".to_string()
            } else {
                previous.join("\n")
            },
            revision.feedback
        ));
    }

    sections.push("[output]\n只输出符合上面结构的 json。".to_string());
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ai_scheduler::models::{
        AiPlannerPreferences, AiTimeWindow, ContextBlock, ContextQueueItem, PlanClock,
    };

    fn queue_item(item_id: i64, title: &str, note: Option<&str>) -> ContextQueueItem {
        ContextQueueItem {
            item_id,
            source_task_id: Some(item_id * 10),
            title: title.to_string(),
            category_key: "math".to_string(),
            category_label: "数学".to_string(),
            subject_id: Some(3),
            priority: "high".to_string(),
            estimated_minutes: 90,
            due_date: Some("2026-09-26".to_string()),
            note: note.map(str::to_string),
            missed_count: 0,
        }
    }

    fn plan_context(items: Vec<ContextQueueItem>) -> PlanContext {
        PlanContext {
            generated_at: "2026-09-25T00:00:00Z".to_string(),
            horizon_days: 1,
            horizon_start: "2026-09-25".to_string(),
            horizon_end: "2026-09-25".to_string(),
            queue_items: items,
            existing_blocks: vec![ContextBlock {
                block_id: 7,
                date: "2026-09-25".to_string(),
                start_minute: 600,
                end_minute: 660,
                title: "英语真题".to_string(),
                locked: true,
                replaceable: false,
            }],
            available_windows: vec![AiTimeWindow {
                weekday: 5,
                start_minute: 480,
                end_minute: 720,
            }],
            peak_windows: vec![AiTimeWindow {
                weekday: 5,
                start_minute: 480,
                end_minute: 600,
            }],
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

    #[test]
    fn system_prompt_keeps_json_keyword_and_schema_example() {
        // `json_object` 档的硬性要求：出现 "json" 字样 + 结构示例。
        let prompt = build_system_prompt();
        assert!(prompt.to_lowercase().contains("json"));
        assert!(prompt.contains("\"item_id\""));
        assert!(prompt.contains("unscheduled"));
    }

    #[test]
    fn user_prompt_lists_queue_items_and_precomputed_gaps() {
        let request = AiPlanRequest::default();
        let prompt = build_user_prompt(
            &request,
            &plan_context(vec![queue_item(41, "数学 660 题", None)]),
            None,
        );

        assert!(prompt.contains("[queue_items]"));
        assert!(prompt.contains("\"item_id\":41"));
        assert!(prompt.contains("数学 660 题"));
        assert!(prompt.contains("- 2026-09-25 10:00-11:00 英语真题"));
        // 空档已扣掉 10:00–11:00 的已有日程及两侧 10 分钟休息，模型不必自己做区间减法。
        assert!(
            prompt.contains("2026-09-25 周五：空档共 160 分钟，还可排 160 分钟学习；空档 08:00-09:50、11:10-12:00；高效时段 08:00-10:00"),
            "{prompt}"
        );
        assert!(prompt.contains("每天学习不超过 480 分钟"));
    }

    #[test]
    fn past_days_and_now_are_spelled_out() {
        let mut context = plan_context(vec![queue_item(41, "数学", None)]);
        context.clock = Some(PlanClock {
            date: "2026-09-26".to_string(),
            minute: 9 * 60,
        });
        let prompt = build_user_prompt(&AiPlanRequest::default(), &context, None);

        assert!(prompt.contains("现在是 2026-09-26 09:00"));
        assert!(prompt.contains("2026-09-25 周五：已经过去，不可排"));
    }

    #[test]
    fn missed_and_overdue_signals_reach_the_model() {
        let mut item = queue_item(41, "数学", None);
        item.missed_count = 2;
        item.due_date = Some("2026-09-24".to_string());
        let prompt = build_user_prompt(&AiPlanRequest::default(), &plan_context(vec![item]), None);

        assert!(prompt.contains("\"missed_count\":2"));
        assert!(prompt.contains("\"overdue\":true"));
    }

    #[test]
    fn titles_with_quotes_stay_valid_json() {
        let prompt = build_user_prompt(
            &AiPlanRequest::default(),
            &plan_context(vec![queue_item(41, "背\"核心\"词汇", None)]),
            None,
        );
        let line = prompt
            .lines()
            .find(|line| line.contains("\"item_id\":41"))
            .expect("queue line");
        let parsed: serde_json::Value = serde_json::from_str(line).expect("valid json line");
        assert_eq!(parsed["title"], "背\"核心\"词汇");
    }

    #[test]
    fn revision_includes_previous_draft_and_feedback() {
        let context = plan_context(vec![queue_item(41, "数学", None)]);
        let previous = vec![AiPlanItem {
            id: "a".to_string(),
            source_task_id: None,
            source_today_item_id: Some(41),
            schedule_date: "2026-09-25".to_string(),
            start_minute: 480,
            end_minute: 570,
            title: "数学".to_string(),
            category_key: "math".to_string(),
            subject_id: None,
            priority: "high".to_string(),
            rationale: None,
            manually_adjusted: false,
            conflict_with: Vec::new(),
            kind: "study".to_string(),
        }];
        let revision = PlanRevision {
            previous_items: &previous,
            feedback: "数学挪到下午",
        };
        let prompt = build_user_prompt(&AiPlanRequest::default(), &context, Some(&revision));

        assert!(prompt.contains("- item_id=41 数学 2026-09-25 08:00-09:30"));
        assert!(prompt.contains("数学挪到下午"));
    }

    #[test]
    fn notes_are_only_sent_when_enabled() {
        let plan_context =
            plan_context(vec![queue_item(41, "带备注的条目", Some("第三章 前两节"))]);

        let mut request = AiPlanRequest::default();
        let with_note = build_user_prompt(&request, &plan_context, None);
        assert!(with_note.contains("第三章 前两节"));

        // 快照里的 note 由 context 决定；这里直接模拟「被清空后的快照」。
        let mut stripped = plan_context;
        stripped.queue_items[0].note = None;
        request.extra_instruction = None;
        let without_note = build_user_prompt(&request, &stripped, None);
        assert!(!without_note.contains("第三章 前两节"));
    }

    #[test]
    fn extra_instruction_is_included() {
        let request = AiPlanRequest {
            extra_instruction: Some("上午先做数学".to_string()),
            ..AiPlanRequest::default()
        };
        let prompt = build_user_prompt(&request, &plan_context(vec![]), None);
        assert!(prompt.contains("上午先做数学"));
    }
}
