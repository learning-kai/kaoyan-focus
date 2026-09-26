//! 提示词构建（方案 §5.1 / §5.2 / §5.3）。
//!
//! 两条硬性约束（改提示词时别破坏）：
//! 1. `json_object` 档要求提示词里必须出现 "json" 字样并给出结构示例；
//! 2. 模型只能引用 `queue_items` 里出现过的 `item_id`，标题等字段由校验器回填。

use super::models::PlanContext;

/// 周几的中文显示名，1 = 周一 … 7 = 周日。
const WEEKDAY_NAMES: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

fn format_minute(minute: i64) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn format_window(weekday: i64, start: i64, end: i64) -> String {
    format!(
        "- {} {}-{}",
        WEEKDAY_NAMES
            .get((weekday - 1).clamp(0, 6) as usize)
            .copied()
            .unwrap_or("未知"),
        format_minute(start),
        format_minute(end)
    )
}

/// 合并后的可用时段按周几列出；空时段不输出。
fn format_windows(windows: &[(i64, i64, i64)]) -> String {
    if windows.is_empty() {
        "（无）".to_string()
    } else {
        windows
            .iter()
            .map(|(weekday, start, end)| format_window(*weekday, *start, *end))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 系统提示词。**对两种结构化输出档位共用同一份**：
/// `json_schema` 档靠 `response_format` 兜底；`json_object` 档靠这里的 "json" 字样与结构示例。
pub fn build_system_prompt() -> String {
    [
        "你是一名考研学生的日程排期助手。你的唯一任务是：把给定的队列条目安排进给定的可用时段，输出 json。",
        "",
        "硬性规则：",
        "1. 只能使用输入 queue_items 中出现过的 item_id，不得编造 id，也不得改写标题；",
        "2. 每条安排的 start_minute 与 end_minute 必须完整落在该日期的可用时段内，并且不与 existing_blocks 重叠；",
        "3. 不得把条目排到它的 due_date 之后；优先级高、截止日近的条目尽量靠前，优先安排进高效时段；",
        "4. 单日安排总时长不得超过 max_daily_minutes，相邻安排之间至少留出 min_break_minutes 分钟休息；学习块可以根据任务难度在合理范围内自行决定时长，不要机械地全部切成同样长度；",
        "5. 固定生活安排（早餐、午餐、晚餐）是不可占用的硬约束；它们已经列在 meal_windows 中，学习任务必须避开；",
        "6. 确实放不下的条目放进 unscheduled，并用一句话说明原因，绝不硬塞；",
        "7. 每个条目在同一天只安排一次；",
        "8. 只输出 json，不要输出任何解释文字、Markdown 代码块或注释。",
        "",
        "输出 json 的结构：",
        r#"{"items":[{"item_id":41,"date":"2026-09-25","start_minute":480,"end_minute":570,"rationale":"上午头脑清醒，先做数学"}],"unscheduled":[{"item_id":42,"reason":"截止日前没有足够长的可用时段"}]}"#,
        "",
        "字段说明：start_minute / end_minute 是距当天 00:00 的分钟数（例如 480 = 08:00）；",
        "rationale 是不超过 40 字的排期理由，可以留空字符串；unscheduled 可以是空数组。",
    ]
    .join("\n")
}

/// 用户提示词。把「可排什么 / 什么时候能排 / 已经占了什么 / 容量多少」一次性给全。
pub fn build_user_prompt(
    request: &super::models::AiPlanRequest,
    plan_context: &PlanContext,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    let mut header = format!(
        "[目标]\n规划起始日 {}，共 {} 天（{} ~ {}）。\n可用时段（分钟制，0 = 00:00）：\n{}\n高效时段：\n{}\n休息间隔 {} 分钟；单日上限 {} 分钟；未估时条目默认 {} 分钟。",
        plan_context.horizon_start,
        plan_context.horizon_days,
        plan_context.horizon_start,
        plan_context.horizon_end,
        format_windows(&windows_of(plan_context)),
        format_windows(&peaks_of(plan_context)),
        match plan_context.planner_preferences.rest_style.as_str() {
            "gentle" => plan_context.min_break_minutes.max(15),
            "focused" => plan_context.min_break_minutes.min(5),
            _ => plan_context.min_break_minutes,
        },
        plan_context.max_daily_minutes,
        plan_context.default_block_minutes,
    );
    let meals = if plan_context.planner_preferences.auto_meals {
        plan_context
            .planner_preferences
            .meal_windows
            .iter()
            .map(|meal| {
                format!(
                    "{} {}-{}",
                    meal.kind,
                    format_minute(meal.start_minute),
                    format_minute(meal.end_minute)
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    header.push_str(&format!(
        "\n管家偏好：自动安排三餐={}；自适应任务时长={}；休息节奏={}；每日目标学习 {} 分钟。\nmeal_windows：{}",
        plan_context.planner_preferences.auto_meals,
        plan_context.planner_preferences.adaptive_durations,
        plan_context.planner_preferences.rest_style,
        plan_context.planner_preferences.daily_target_minutes,
        if meals.is_empty() { "（无）".to_string() } else { meals.join("、") },
    ));
    if !plan_context
        .planner_preferences
        .memory_note
        .trim()
        .is_empty()
    {
        header.push_str(&format!(
            "\n长期偏好备注：{}",
            plan_context.planner_preferences.memory_note
        ));
    }
    if let Some(instruction) = request.extra_instruction.as_deref() {
        header.push_str(&format!("\n补充说明（请尽量遵守）：{instruction}"));
    }
    sections.push(header);

    let mut blocks = String::from("[existing_blocks]\n");
    if plan_context.existing_blocks.is_empty() {
        blocks.push_str("（无）");
    } else {
        let lines: Vec<String> = plan_context
            .existing_blocks
            .iter()
            .map(|block| {
                format!(
                    "{}: {}-{} {}{}",
                    block.date,
                    format_minute(block.start_minute),
                    format_minute(block.end_minute),
                    block.title,
                    if block.locked {
                        " (locked，不可占用)"
                    } else {
                        ""
                    },
                )
            })
            .collect();
        blocks.push_str(&lines.join("\n"));
    }
    sections.push(blocks);

    let mut queue = format!(
        "[queue_items]\n（以下 {} 条是今天要排的全部条目；只能引用这里出现过的 item_id）\n",
        plan_context.queue_items.len()
    );
    let lines: Vec<String> = plan_context
        .queue_items
        .iter()
        .map(|item| {
            let mut line = format!(
                "{{\"item_id\":{},\"title\":\"{}\",\"category\":\"{}\",\"priority\":\"{}\",\"estimated_minutes\":{},\"due_date\":\"{}\"",
                item.item_id,
                item.title.replace('"', "'"),
                item.category_label,
                item.priority,
                item.estimated_minutes,
                item.due_date.as_deref().unwrap_or("null"),
            );
            if let Some(note) = item.note.as_deref() {
                line.push_str(&format!(",\"note\":\"{}\"", note.replace('"', "'")));
            }
            line.push('}');
            line
        })
        .collect();
    queue.push_str(&lines.join("\n"));
    sections.push(queue);

    sections.push("[output]\n只输出符合上面结构的 json。".to_string());

    sections.join("\n\n")
}

/// 把按 weekday 存储的可用时段展开成 `(weekday, start, end)` 三元组，供模板渲染。
fn windows_of(plan_context: &PlanContext) -> Vec<(i64, i64, i64)> {
    plan_context
        .available_windows
        .iter()
        .map(|window| (window.weekday, window.start_minute, window.end_minute))
        .collect()
}

fn peaks_of(plan_context: &PlanContext) -> Vec<(i64, i64, i64)> {
    plan_context
        .peak_windows
        .iter()
        .map(|window| (window.weekday, window.start_minute, window.end_minute))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ai_scheduler::models::{
        AiPlanRequest, AiPlannerPreferences, AiTimeWindow, ContextBlock, ContextQueueItem,
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
    fn user_prompt_lists_queue_items_and_locked_blocks() {
        let request = AiPlanRequest::default();
        let prompt = build_user_prompt(
            &request,
            &plan_context(vec![queue_item(41, "数学 660 题", None)]),
        );

        assert!(prompt.contains("[queue_items]"));
        assert!(prompt.contains("\"item_id\":41"));
        assert!(prompt.contains("数学 660 题"));
        assert!(prompt.contains("2026-09-25: 10:00-11:00 英语真题 (locked，不可占用)"));
        // 高效时段与容量约束都要出现，模型才有依据。
        assert!(prompt.contains("08:00-10:00"));
        assert!(prompt.contains("单日上限 480 分钟"));
    }

    #[test]
    fn notes_are_only_sent_when_enabled() {
        let plan_context =
            plan_context(vec![queue_item(41, "带备注的条目", Some("第三章 前两节"))]);

        let mut request = AiPlanRequest::default();
        let with_note = build_user_prompt(&request, &plan_context);
        assert!(with_note.contains("第三章 前两节"));

        // 快照里的 note 由 context 决定；这里直接模拟「被清空后的快照」。
        let mut stripped = plan_context;
        stripped.queue_items[0].note = None;
        request.extra_instruction = None;
        let without_note = build_user_prompt(&request, &stripped);
        assert!(!without_note.contains("第三章 前两节"));
    }

    #[test]
    fn extra_instruction_is_included() {
        let request = AiPlanRequest {
            extra_instruction: Some("上午先做数学".to_string()),
            ..AiPlanRequest::default()
        };
        let prompt = build_user_prompt(&request, &plan_context(vec![]));
        assert!(prompt.contains("上午先做数学"));
    }
}
