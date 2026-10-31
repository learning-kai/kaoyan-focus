//! AI 复盘起草（日复盘 / 周复盘）。
//!
//! 复用 `commands::ai_scheduler` 已经建好的调用层：配置读取、DPAPI 密钥、HTTP 客户端、
//! 重试与退避、错误信封全部沿用，只补一条复盘专用链路——
//! 把这段时间实际发生了什么整理成提示词，让模型写三段中文，回给前端当草稿。
//!
//! 三个刻意的取舍：
//!
//! 1. 不落库。草稿只填进前端的三个输入框，用户改完再走既有的
//!    `save_daily_review` / `save_weekly_review`。不新增表、不碰同步包，
//!    也不会出现 AI 写的和你写的两份记录。
//! 2. 不新增配置。开关、密钥、模型、超时都复用 AI 排期那一份，用户不用配第二遍。
//! 3. 数据先本地汇总。发给模型的是聚合后的事实（时长、完成率、休息理由、卡点），
//!    不是原始记录流水；`context_notes` 会把用过哪些数据回显给用户看。

use super::ai_scheduler::{
    client,
    models::{
        AiSchedulerError, AiSchedulerSettings, ERR_BAD_REQUEST, ERR_DB_ERROR, ERR_MISSING_API_KEY,
    },
    settings as ai_settings,
};
use crate::storage::db::open_database;
use chrono::{Datelike, Duration, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// 复盘起草的输出上限。三段中文各 150 字左右，留足余量。
const REVIEW_MAX_TOKENS: i64 = 1200;

/// 单段文本的硬上限，防止模型灌水把输入框挤爆（前端 maxLength 是 1200）。
const MAX_SECTION_CHARS: usize = 1000;

/// 一次复盘起草最多列出的休息理由条数，避免提示词过长。
const MAX_BREAK_REASONS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewScope {
    Daily,
    Weekly,
}

impl ReviewScope {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Daily => "今天",
            Self::Weekly => "这一周",
        }
    }
}

/// 前端拿到的复盘草稿。字段与 `DailyReviewDraft` / `WeeklyReviewDraft` 的文本字段对应，
/// 直接填进输入框即可，用户可自由改写后再保存。
#[derive(Debug, Clone, Serialize)]
pub struct AiReviewDraft {
    /// `daily` | `weekly`
    pub scope: String,
    pub summary: String,
    pub blockers: String,
    pub focus: String,
    /// 本次用到的数据摘要，回显给用户看发了什么。
    pub context_notes: Vec<String>,
    /// 出这次结果的模型名。
    pub model: String,
}

/// 本地汇总出来的事实。既用于拼提示词，也用于模型没给某段时的兜底。
#[derive(Debug, Clone, Default)]
struct ReviewFacts {
    period_label: String,
    study_minutes: i64,
    focus_session_count: i64,
    interruption_count: i64,
    schedule_total: i64,
    schedule_completed: i64,
    today_total: i64,
    today_completed: i64,
    /// 正计时手动休息时填写的理由，按最近优先。
    break_reasons: Vec<String>,
    completed_titles: Vec<String>,
    unfinished_titles: Vec<String>,
}

impl ReviewFacts {
    fn completion_summary(&self) -> Option<String> {
        let total = self.schedule_total + self.today_total;
        if total <= 0 {
            return None;
        }
        let done = self.schedule_completed + self.today_completed;
        Some(format!("{done}/{total}"))
    }
}

/// 复盘页「让 AI 起草」的入口。
///
/// `review_date` 对日复盘是当天日期；对周复盘是该周内任意一天，后端归一到周一。
#[tauri::command]
pub fn draft_review_with_ai(
    app: AppHandle,
    scope: String,
    review_date: Option<String>,
) -> Result<AiReviewDraft, String> {
    let scope = ReviewScope::parse(&scope)
        .ok_or_else(|| bad_request(format!("未知的复盘范围：{scope}")).to_envelope())?;

    let connection =
        open_database(&database_path(&app)?).map_err(|error| db_error(error).to_envelope())?;

    // 与 AI 排期共用同一份配置与密钥。
    let settings =
        ai_settings::current_settings(&connection).map_err(|error| error.to_envelope())?;
    ensure_ai_ready(&settings)?;
    let api_key = ai_settings::load_api_key(&connection).map_err(|error| error.to_envelope())?;
    let base_url =
        client::normalize_base_url(&settings.base_url).map_err(|error| error.to_envelope())?;
    let http =
        client::build_client(settings.timeout_seconds).map_err(|error| error.to_envelope())?;

    let facts = load_facts(&connection, scope, review_date.as_deref())?;
    let system_prompt = build_system_prompt(scope);
    let user_prompt = build_user_prompt(scope, &facts);

    let raw = client::chat_text(
        &http,
        &base_url,
        &api_key,
        &settings,
        &system_prompt,
        &user_prompt,
        REVIEW_MAX_TOKENS,
    )
    .map_err(|error| error.to_envelope())?;

    let parsed = parse_sections(&raw);
    let (summary, blockers, focus) = apply_fallbacks(parsed, &facts);

    Ok(AiReviewDraft {
        scope: scope.as_str().to_string(),
        summary,
        blockers,
        focus,
        context_notes: build_context_notes(scope, &facts),
        model: settings.model.clone(),
    })
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn bad_request(message: impl Into<String>) -> AiSchedulerError {
    AiSchedulerError::new(ERR_BAD_REQUEST, message, false)
}

fn db_error(message: impl std::fmt::Display) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, message.to_string(), false)
}

/// 与排期链路同一套就绪检查，提示文案指向 AI 复盘。
fn ensure_ai_ready(settings: &AiSchedulerSettings) -> Result<(), String> {
    let problem = if !settings.enabled {
        Some((
            ERR_BAD_REQUEST,
            "AI 复盘尚未开启，请先在 设置 → 集成 → AI 排期 中打开",
        ))
    } else if !settings.privacy_acknowledged {
        Some((
            ERR_BAD_REQUEST,
            "请先在 设置 → 集成 → AI 排期 中确认数据使用说明",
        ))
    } else if !settings.api_key_configured {
        Some((
            ERR_MISSING_API_KEY,
            "请先在 设置 → 集成 → AI 排期 中填写 API Key",
        ))
    } else if settings.base_url.is_empty() || settings.model.is_empty() {
        Some((
            ERR_BAD_REQUEST,
            "请先在 设置 → 集成 → AI 排期 中填写接口地址与模型名",
        ))
    } else {
        None
    };

    match problem {
        Some((code, message)) => Err(AiSchedulerError::new(code, message, false).to_envelope()),
        None => Ok(()),
    }
}

/// 把这段时间的原始记录压成一份事实快照。全部在本地完成，发出去的只有这些聚合值。
fn load_facts(
    connection: &Connection,
    scope: ReviewScope,
    review_date: Option<&str>,
) -> Result<ReviewFacts, String> {
    match scope {
        ReviewScope::Daily => load_daily_facts(connection, review_date),
        ReviewScope::Weekly => load_weekly_facts(connection, review_date),
    }
}

fn load_daily_facts(
    connection: &Connection,
    review_date: Option<&str>,
) -> Result<ReviewFacts, String> {
    let date = match review_date.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => validate_date(value)?,
        None => Local::now().date_naive().format("%Y-%m-%d").to_string(),
    };
    let next_day = shift_date(&date, 1)?;

    let (study_seconds, focus_session_count) = connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0), COUNT(*)
            FROM focus_sessions
            WHERE status = 'finished'
              AND actual_seconds >= 60
              AND date(started_at, '+8 hours') = ?1
            ",
            params![date],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| error.to_string())?;

    let interruption_count = connection
        .query_row(
            "
            SELECT COALESCE(SUM(interruption_count), 0)
            FROM focus_sessions
            WHERE date(started_at, '+8 hours') = ?1
            ",
            params![date],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    let (schedule_total, schedule_completed) = connection
        .query_row(
            "
            SELECT COUNT(*), COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0)
            FROM schedule_blocks
            WHERE schedule_date = ?1
            ",
            params![date],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| error.to_string())?;

    let (today_total, today_completed) = connection
        .query_row(
            "
            SELECT COUNT(*), COALESCE(SUM(CASE WHEN completed = 1 THEN 1 ELSE 0 END), 0)
            FROM today_plan_items
            WHERE today_date = ?1
            ",
            params![date],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| error.to_string())?;

    Ok(ReviewFacts {
        period_label: date.clone(),
        study_minutes: study_seconds / 60,
        focus_session_count,
        interruption_count,
        schedule_total,
        schedule_completed,
        today_total,
        today_completed,
        break_reasons: load_break_reasons(connection, &date, &next_day)?,
        completed_titles: load_titles(connection, &date, true)?,
        unfinished_titles: load_titles(connection, &date, false)?,
    })
}

fn load_weekly_facts(
    connection: &Connection,
    week_date: Option<&str>,
) -> Result<ReviewFacts, String> {
    let anchor = match week_date.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => validate_date(value)?,
        None => Local::now().date_naive().format("%Y-%m-%d").to_string(),
    };
    let start = week_start_monday(&anchor)?;
    let end_exclusive = shift_date(&start, 7)?;

    let (study_seconds, focus_session_count) = connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0), COUNT(*)
            FROM focus_sessions
            WHERE status = 'finished'
              AND actual_seconds >= 60
              AND date(started_at, '+8 hours') >= ?1
              AND date(started_at, '+8 hours') < ?2
            ",
            params![start, end_exclusive],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| error.to_string())?;

    let interruption_count = connection
        .query_row(
            "
            SELECT COALESCE(SUM(interruption_count), 0)
            FROM focus_sessions
            WHERE date(started_at, '+8 hours') >= ?1
              AND date(started_at, '+8 hours') < ?2
            ",
            params![start, end_exclusive],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    // 周视图沿用复盘页口径：只统计学习记录，计划完成情况看日复盘。
    Ok(ReviewFacts {
        period_label: format!("{start} ~ {}", shift_date(&start, 6)?),
        study_minutes: study_seconds / 60,
        focus_session_count,
        interruption_count,
        schedule_total: 0,
        schedule_completed: 0,
        today_total: 0,
        today_completed: 0,
        break_reasons: load_break_reasons(connection, &start, &end_exclusive)?,
        completed_titles: query_week_titles(connection, &start, &end_exclusive, true)?,
        unfinished_titles: query_week_titles(connection, &start, &end_exclusive, false)?,
    })
}

/// 正计时手动休息的理由，按最近优先。这是这个应用里最能反映为什么停下来的信号。
fn load_break_reasons(
    connection: &Connection,
    start: &str,
    end_exclusive: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT break_reason
            FROM focus_sessions
            WHERE break_reason IS NOT NULL
              AND TRIM(break_reason) != ''
              AND date(started_at, '+8 hours') >= ?1
              AND date(started_at, '+8 hours') < ?2
            ORDER BY started_at DESC
            LIMIT ?3
            ",
        )
        .map_err(|error| error.to_string())?;

    let reasons = statement
        .query_map(
            params![start, end_exclusive, MAX_BREAK_REASONS as i64],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(reasons)
}

fn load_titles(
    connection: &Connection,
    date: &str,
    completed: bool,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT title FROM today_plan_items
            WHERE today_date = ?1 AND completed = ?2
            UNION ALL
            SELECT title FROM schedule_blocks
            WHERE schedule_date = ?1
              AND status = CASE WHEN ?2 = 1 THEN 'completed' ELSE 'planned' END
            ORDER BY title
            ",
        )
        .map_err(|error| error.to_string())?;

    let titles = statement
        .query_map(params![date, i64::from(completed)], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(titles)
}

fn query_week_titles(
    connection: &Connection,
    start: &str,
    end_exclusive: &str,
    completed: bool,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT title FROM today_plan_items
            WHERE today_date >= ?1 AND today_date < ?2 AND completed = ?3
            UNION ALL
            SELECT title FROM schedule_blocks
            WHERE schedule_date >= ?1 AND schedule_date < ?2
              AND status = CASE WHEN ?3 = 1 THEN 'completed' ELSE 'planned' END
            ORDER BY title
            ",
        )
        .map_err(|error| error.to_string())?;

    let titles = statement
        .query_map(params![start, end_exclusive, i64::from(completed)], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(titles)
}

fn build_system_prompt(scope: ReviewScope) -> String {
    let (summary_field, focus_field) = match scope {
        ReviewScope::Daily => ("今日总结", "明日重点"),
        ReviewScope::Weekly => ("本周总结", "下周重点"),
    };

    format!(
        "你是一名带过十几届考研生的督学教练，说话直接、有脾气、不惯着。用户要的是被骂醒，不是被安慰。\n\n输出格式（必须严格遵守）：\n1. 只输出三行，每行格式为「字段名：内容」，字段名依次是「{summary_field}」「问题卡点」「{focus_field}」。\n2. 不要输出任何解释、前言、markdown 标记或额外空行。\n3. 每段 100-180 个汉字。\n4. 只能使用用户数据中出现的科目、事项、时间和数字；不要编造没发生过的事，也不要编造数据里没有的细节。\n5. 如果某段确实没素材可写，就写「暂无数据」。\n\n语气与判断标准（这是重点，必须做到）：\n- 不要当老好人。数据里暴露出问题时，直接点名，用「你」称呼，把责任落到具体行为上，而不是笼统评价状态。\n- 禁止空转话术：不要写「总体不错」「继续加油」「保持节奏」「注意劳逸结合」「建议适当」这类没有信息量的缓冲句。每一句都要能落到具体的时间、科目、数字或行为上。\n- 批评要具体到可反驳：不要写「效率不高」，要写「{period}只专注了 N 分钟，却记了 M 次干扰」；不要写「计划没完成」，要把没完成的事项名点出来。\n- 允许尖锐、允许难听，但每一条批评都必须有数据里的数字或事实支撑；找不到支撑就先不说，不要为了凶而编。\n- 「问题卡点」是火力最集中的一段：先指出{period}最要命的一个问题，再说明它正在怎样拖垮结果。不要罗列一堆小毛病，也不要安抚。\n- 「{focus_field}」只给动作，不给口号：先做什么、做多久、做到什么程度算完成，写清楚。\n- 「{summary_field}」也要诚实：做得好就说做得好，但同样要带数字，不要顺手夸奖。\n- 你的目标是把时间抢回来，不是给他打分。批评行为，不否定人品；不做人身攻击，不涉及隐私，不涉及外貌、家庭或健康。",
        period = scope.label(),
        summary_field = summary_field,
        focus_field = focus_field,
    )
}

fn build_user_prompt(scope: ReviewScope, facts: &ReviewFacts) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("统计区间：{}", facts.period_label));
    lines.push(format!(
        "学习时长：{} 分钟（共 {} 条有效专注记录）",
        facts.study_minutes, facts.focus_session_count
    ));
    lines.push(format!("干扰次数：{} 次", facts.interruption_count));

    // 把「该被追问的地方」提前算好：直接给出数字对比，
    // 模型就不会用「总体不错」蒙混过去，而是能直接拿数字说话。
    if facts.focus_session_count > 0 {
        let average = facts.study_minutes / facts.focus_session_count.max(1);
        lines.push(format!("平均每条专注：{average} 分钟"));
    }
    if facts.study_minutes > 0 {
        let per_hour = (facts.interruption_count as f64) * 60.0 / (facts.study_minutes as f64);
        lines.push(format!("折合每小时干扰：{per_hour:.1} 次"));
    }

    if let Some(completion) = facts.completion_summary() {
        lines.push(format!("计划完成：{completion}"));
        let total = facts.schedule_total + facts.today_total;
        let done = facts.schedule_completed + facts.today_completed;
        if total > 0 && done < total {
            lines.push(format!("未完成数量：{} 项", total - done));
        }
    }
    if !facts.completed_titles.is_empty() {
        lines.push(format!(
            "已完成的事项：{}",
            facts.completed_titles.join("、")
        ));
    }
    if !facts.unfinished_titles.is_empty() {
        lines.push(format!(
            "未完成的事项：{}",
            facts.unfinished_titles.join("、")
        ));
    }
    if !facts.break_reasons.is_empty() {
        lines.push(format!(
            "手动休息时填写的理由（最能反映卡点）：{}",
            facts.break_reasons.join("；")
        ));
    }

    lines.push(String::new());
    lines.push(match scope {
        ReviewScope::Daily => "请对着上面的数字写今天的复盘。先说今天到底推进了什么（带数字），再直接点出最要命的那个卡点并说明它正在怎样拖垮结果，最后给出明天能直接执行的动作。不要客气。".to_string(),
        ReviewScope::Weekly => "请对着上面的数字写这一周的复盘。先说这周真正推进了什么（带数字），再直接点出反复出现、一直没解决的那个阻力并说明他为什么没解决，最后给出下周能直接执行的动作。不要客气。".to_string(),
    });

    lines.join("\n")
}

/// 把模型的三行输出解析成三段。容忍全角冒号、markdown 加粗和多余空行。
fn parse_sections(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut summary = None;
    let mut blockers = None;
    let mut focus = None;

    for line in raw.lines() {
        let cleaned = line
            .trim()
            .trim_start_matches(['-', '*', '#', ' '])
            .replace("**", "");
        if cleaned.is_empty() {
            continue;
        }

        let Some((label, value)) = split_label(&cleaned) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        if summary.is_none() && is_summary_label(&label) {
            summary = Some(value.to_string());
        } else if blockers.is_none() && is_blocker_label(&label) {
            blockers = Some(value.to_string());
        } else if focus.is_none() && is_focus_label(&label) {
            focus = Some(value.to_string());
        }
    }

    // 模型完全没按格式来时，退化成整段当总结，至少不让用户拿到空。
    if summary.is_none() && blockers.is_none() && focus.is_none() {
        let fallback: String = raw.trim().chars().take(MAX_SECTION_CHARS).collect();
        if !fallback.is_empty() {
            summary = Some(fallback);
        }
    }

    (summary, blockers, focus)
}

fn split_label(line: &str) -> Option<(String, &str)> {
    if let Some(index) = line.find('\u{ff1a}') {
        return Some((
            line[..index].trim().to_string(),
            &line[index + '\u{ff1a}'.len_utf8()..],
        ));
    }
    if let Some(index) = line.find(':') {
        return Some((line[..index].trim().to_string(), &line[index + 1..]));
    }
    None
}

fn is_summary_label(label: &str) -> bool {
    label.contains("今日总结") || label.contains("本周总结") || label.contains("总结")
}

fn is_blocker_label(label: &str) -> bool {
    label.contains("卡点") || label.contains("问题") || label.contains("困难")
}

fn is_focus_label(label: &str) -> bool {
    label.contains("明日重点") || label.contains("下周重点") || label.contains("重点")
}

/// 缺哪段就用本地事实兜底，保证三个框都有可编辑的起点。
fn apply_fallbacks(
    parsed: (Option<String>, Option<String>, Option<String>),
    facts: &ReviewFacts,
) -> (String, String, String) {
    let (summary, blockers, focus) = parsed;

    let summary = summary
        .map(|text| clamp_section(&text))
        .unwrap_or_else(|| clamp_section(&local_summary(facts)));
    let blockers = blockers
        .map(|text| clamp_section(&text))
        .unwrap_or_else(|| clamp_section(&local_blockers(facts)));
    let focus = focus
        .map(|text| clamp_section(&text))
        .unwrap_or_else(|| clamp_section(&local_focus(facts)));

    (summary, blockers, focus)
}

fn local_summary(facts: &ReviewFacts) -> String {
    if facts.focus_session_count == 0 {
        return "暂无数据：这段时间没有记录到有效专注。".to_string();
    }
    let mut text = format!(
        "这段时间专注 {} 分钟，共 {} 条有效记录",
        facts.study_minutes, facts.focus_session_count
    );
    if let Some(completion) = facts.completion_summary() {
        text.push_str(&format!("，计划完成 {completion}"));
    }
    text.push('\u{3002}');
    text
}

fn local_blockers(facts: &ReviewFacts) -> String {
    if let Some(reason) = facts.break_reasons.first() {
        return format!("手动休息时写下的理由：{reason}。");
    }
    if facts.interruption_count > 0 {
        return format!("这段时间记录了 {} 次干扰。", facts.interruption_count);
    }
    "暂无数据：这段时间没有记录到明确的卡点。".to_string()
}

fn local_focus(facts: &ReviewFacts) -> String {
    if let Some(title) = facts.unfinished_titles.first() {
        return format!("先把「{title}」收掉。");
    }
    "暂无数据：还没有安排好的下一步。".to_string()
}

fn clamp_section(text: &str) -> String {
    text.trim().chars().take(MAX_SECTION_CHARS).collect()
}

fn build_context_notes(scope: ReviewScope, facts: &ReviewFacts) -> Vec<String> {
    let mut notes = vec![format!(
        "{}的学习时长与专注记录",
        if scope == ReviewScope::Daily {
            "当天"
        } else {
            "本周"
        }
    )];
    if facts.schedule_total + facts.today_total > 0 {
        notes.push("计划完成情况".to_string());
    }
    if !facts.completed_titles.is_empty() || !facts.unfinished_titles.is_empty() {
        notes.push("事项标题".to_string());
    }
    if !facts.break_reasons.is_empty() {
        notes.push("休息理由".to_string());
    }
    notes
}

fn validate_date(value: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%Y-%m-%d").to_string())
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())
}

fn shift_date(value: &str, days: i64) -> Result<String, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| (date + Duration::days(days)).format("%Y-%m-%d").to_string())
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())
}

fn week_start_monday(value: &str) -> Result<String, String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())?;
    let offset = i64::from(date.weekday().num_days_from_monday());
    shift_date(value, -offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_facts() -> ReviewFacts {
        ReviewFacts {
            period_label: "2026-10-30".to_string(),
            study_minutes: 210,
            focus_session_count: 4,
            interruption_count: 3,
            schedule_total: 4,
            schedule_completed: 3,
            today_total: 2,
            today_completed: 1,
            break_reasons: vec!["眼睛酸了".to_string()],
            completed_titles: vec!["英语阅读".to_string()],
            unfinished_titles: vec!["数学真题".to_string()],
        }
    }

    #[test]
    fn parse_sections_accepts_full_width_colon_and_bold() {
        let raw = "**今日总结**：推进了英语阅读\n问题卡点：数学卡在极限\n明日重点：先做真题";
        let (summary, blockers, focus) = parse_sections(raw);
        assert_eq!(summary.as_deref(), Some("推进了英语阅读"));
        assert_eq!(blockers.as_deref(), Some("数学卡在极限"));
        assert_eq!(focus.as_deref(), Some("先做真题"));
    }

    #[test]
    fn parse_sections_accepts_ascii_colon_and_bullets() {
        let raw = "- 今日总结: 完成两套卷子\n- 问题卡点: 注意力涣散\n- 明日重点: 复盘错题";
        let (summary, blockers, focus) = parse_sections(raw);
        assert_eq!(summary.as_deref(), Some("完成两套卷子"));
        assert_eq!(blockers.as_deref(), Some("注意力涣散"));
        assert_eq!(focus.as_deref(), Some("复盘错题"));
    }

    #[test]
    fn parse_sections_falls_back_to_whole_text_when_unstructured() {
        let raw = "今天状态一般，做了两套卷子。";
        let (summary, blockers, focus) = parse_sections(raw);
        assert!(summary.is_some());
        assert!(blockers.is_none());
        assert!(focus.is_none());
    }

    #[test]
    fn apply_fallbacks_fills_missing_sections_from_local_facts() {
        let facts = sample_facts();
        let (summary, blockers, focus) = apply_fallbacks((None, None, None), &facts);
        assert!(summary.contains("210"));
        assert!(blockers.contains("眼睛酸了"));
        assert!(focus.contains("数学真题"));
    }

    #[test]
    fn apply_fallbacks_keeps_model_text_when_present() {
        let facts = sample_facts();
        let (summary, _, _) = apply_fallbacks(
            (
                Some("模型写的总结".to_string()),
                Some("模型写的卡点".to_string()),
                Some("模型写的重点".to_string()),
            ),
            &facts,
        );
        assert_eq!(summary, "模型写的总结");
    }

    #[test]
    fn clamp_section_limits_length_by_chars() {
        let long = "字".repeat(MAX_SECTION_CHARS + 50);
        assert_eq!(clamp_section(&long).chars().count(), MAX_SECTION_CHARS);
    }

    #[test]
    fn week_start_monday_normalizes_any_day_in_week() {
        assert_eq!(week_start_monday("2026-10-30").unwrap(), "2026-10-26");
        assert_eq!(week_start_monday("2026-10-26").unwrap(), "2026-10-26");
        assert_eq!(week_start_monday("2026-11-01").unwrap(), "2026-10-26");
    }

    #[test]
    fn scope_parse_rejects_unknown_values() {
        assert_eq!(ReviewScope::parse("daily"), Some(ReviewScope::Daily));
        assert_eq!(ReviewScope::parse(" weekly "), Some(ReviewScope::Weekly));
        assert_eq!(ReviewScope::parse("monthly"), None);
        assert_eq!(ReviewScope::parse(""), None);
    }

    #[test]
    fn user_prompt_includes_break_reasons_and_unfinished_titles() {
        let facts = sample_facts();
        let prompt = build_user_prompt(ReviewScope::Daily, &facts);
        assert!(prompt.contains("眼睛酸了"));
        assert!(prompt.contains("数学真题"));
        assert!(prompt.contains("210"));
    }

    #[test]
    fn system_prompt_demands_blunt_criticism_and_bans_small_talk() {
        let prompt = build_system_prompt(ReviewScope::Daily);
        // 必须明确要求「骂醒」而不是安抚。
        assert!(prompt.contains("骂醒"));
        assert!(prompt.contains("老好人"));
        // 必须点名禁用空话，否则模型一定回退到安全话术。
        for banned in ["总体不错", "继续加油", "保持节奏"] {
            assert!(prompt.contains(banned), "应明确禁用这类空话：{banned}");
        }
        // 必须要求每条批评都有数据支撑。
        assert!(prompt.contains("数据里的数字"));
        // 边界：骂行为不骂人，避免模型失控成人身攻击。
        assert!(prompt.contains("批评行为，不否定人品"));
        assert!(prompt.contains("不做人身攻击"));
        assert!(prompt.contains("不涉及隐私"));
    }

    #[test]
    fn system_prompt_uses_scope_specific_field_names() {
        let daily = build_system_prompt(ReviewScope::Daily);
        assert!(daily.contains("今日总结"));
        assert!(daily.contains("明日重点"));
        let weekly = build_system_prompt(ReviewScope::Weekly);
        assert!(weekly.contains("本周总结"));
        assert!(weekly.contains("下周重点"));
    }

    #[test]
    fn user_prompt_supplies_numbers_to_argue_with() {
        let prompt = build_user_prompt(ReviewScope::Daily, &sample_facts());
        // 210 分钟 / 4 条 = 平均每条专注时长，模型可以直接拿它说事。
        assert!(prompt.contains("平均每条专注"));
        // 干扰密度是「效率」最直接的可反驳证据。
        assert!(prompt.contains("折合每小时干扰"));
        // 计划没做完时要把缺口数量摆出来。
        assert!(prompt.contains("未完成数量"));
    }

    #[test]
    fn context_notes_list_only_data_actually_sent() {
        let facts = ReviewFacts {
            period_label: "2026-10-30".to_string(),
            study_minutes: 60,
            focus_session_count: 1,
            ..Default::default()
        };
        let notes = build_context_notes(ReviewScope::Daily, &facts);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("学习时长"));
    }
}
