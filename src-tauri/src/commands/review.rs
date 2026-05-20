use crate::{
    storage::db::open_database,
    sync_package::{ensure_sync_meta_for_local_id, mark_entity_deleted},
};
use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::thread;
use tauri::{AppHandle, Manager};

const ENTITY_DAILY_REVIEW: &str = "daily_review";
const ENTITY_WEEKLY_REVIEW: &str = "weekly_review";
const MIN_RECORDED_FOCUS_SECONDS: i64 = 60;

fn trigger_shared_sync(app: &AppHandle, trigger: &'static str) {
    let app = app.clone();
    thread::spawn(move || {
        let _ = crate::commands::sync::sync_object_storage_after_external_change(app, trigger);
    });
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyReview {
    pub id: i64,
    pub review_date: String,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub tomorrow_focus: Option<String>,
    pub mood_score: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyReviewSummary {
    pub study_seconds: i64,
    pub focus_session_count: i64,
    pub interruption_count: i64,
    pub schedule_total: i64,
    pub schedule_completed: i64,
    pub today_total: i64,
    pub today_completed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyReview {
    pub id: i64,
    pub week_start_date: String,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub next_week_focus: Option<String>,
    pub mood_score: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyReviewSummary {
    pub study_seconds: i64,
    pub focus_session_count: i64,
    pub interruption_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyReviewPageData {
    pub week_start_date: String,
    pub week_end_date: String,
    pub review: Option<WeeklyReview>,
    pub summary: WeeklyReviewSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyReviewPageData {
    pub review_date: String,
    pub review: Option<DailyReview>,
    pub summary: DailyReviewSummary,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewDraft {
    pub review_date: String,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub tomorrow_focus: Option<String>,
    pub mood_score: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReviewDraft {
    pub week_start_date: String,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub next_week_focus: Option<String>,
    pub mood_score: i64,
}

#[tauri::command]
pub fn get_daily_review_page_data(
    app: AppHandle,
    review_date: Option<String>,
) -> Result<DailyReviewPageData, String> {
    let connection = open_database(&database_path(&app)?)?;
    let date = parse_date_or_today(review_date.as_deref())?;
    Ok(DailyReviewPageData {
        review_date: date.clone(),
        review: get_daily_review(&connection, &date)?,
        summary: get_daily_summary(&connection, &date)?,
    })
}

#[tauri::command]
pub fn save_daily_review(app: AppHandle, draft: DailyReviewDraft) -> Result<DailyReview, String> {
    let connection = open_database(&database_path(&app)?)?;
    let review_date = validate_date(&draft.review_date)?;
    let now = Utc::now().to_rfc3339();
    let mood_score = draft.mood_score.clamp(1, 5);
    connection
        .execute(
            "
            INSERT INTO daily_reviews (
              review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(review_date) DO UPDATE SET
              summary = excluded.summary,
              blockers = excluded.blockers,
              tomorrow_focus = excluded.tomorrow_focus,
              mood_score = excluded.mood_score,
              updated_at = excluded.updated_at
            ",
            params![
                review_date,
                normalize_optional_string(draft.summary),
                normalize_optional_string(draft.blockers),
                normalize_optional_string(draft.tomorrow_focus),
                mood_score,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;

    let review = get_daily_review(&connection, &review_date)?
        .ok_or_else(|| "复盘保存失败。".to_string())?;
    ensure_sync_meta_for_local_id(
        &connection,
        ENTITY_DAILY_REVIEW,
        review.id,
        Some(format!("daily_review:{review_date}")),
        Utc::now().timestamp_millis(),
    )?;
    trigger_shared_sync(&app, "daily_review_change");
    Ok(review)
}

#[tauri::command]
pub fn delete_daily_review(app: AppHandle, review_id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM daily_reviews WHERE id = ?1",
            params![review_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Err("复盘记录不存在。".to_string());
    }

    mark_entity_deleted(
        &connection,
        ENTITY_DAILY_REVIEW,
        review_id,
        Utc::now().timestamp_millis(),
    )?;
    connection
        .execute(
            "DELETE FROM daily_reviews WHERE id = ?1",
            params![review_id],
        )
        .map_err(|error| error.to_string())?;
    trigger_shared_sync(&app, "daily_review_change");
    Ok(())
}

#[tauri::command]
pub fn get_weekly_review_page_data(
    app: AppHandle,
    week_date: Option<String>,
) -> Result<WeeklyReviewPageData, String> {
    let connection = open_database(&database_path(&app)?)?;
    let week_start_date = parse_week_start_or_current(week_date.as_deref())?;
    let week_end_date = shift_iso_date(&week_start_date, 6)?;
    Ok(WeeklyReviewPageData {
        week_start_date: week_start_date.clone(),
        week_end_date: week_end_date.clone(),
        review: get_weekly_review(&connection, &week_start_date)?,
        summary: get_weekly_summary(&connection, &week_start_date, &week_end_date)?,
    })
}

#[tauri::command]
pub fn save_weekly_review(
    app: AppHandle,
    draft: WeeklyReviewDraft,
) -> Result<WeeklyReview, String> {
    let connection = open_database(&database_path(&app)?)?;
    let week_start_date = week_start_monday(&validate_date(&draft.week_start_date)?)?;
    let now = Utc::now().to_rfc3339();
    let mood_score = draft.mood_score.clamp(1, 5);
    connection
        .execute(
            "
            INSERT INTO weekly_reviews (
              week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(week_start_date) DO UPDATE SET
              summary = excluded.summary,
              blockers = excluded.blockers,
              next_week_focus = excluded.next_week_focus,
              mood_score = excluded.mood_score,
              updated_at = excluded.updated_at
            ",
            params![
                week_start_date,
                normalize_optional_string(draft.summary),
                normalize_optional_string(draft.blockers),
                normalize_optional_string(draft.next_week_focus),
                mood_score,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;

    let review = get_weekly_review(&connection, &week_start_date)?
        .ok_or_else(|| "周复盘保存失败。".to_string())?;
    ensure_sync_meta_for_local_id(
        &connection,
        ENTITY_WEEKLY_REVIEW,
        review.id,
        Some(format!("weekly_review:{week_start_date}")),
        Utc::now().timestamp_millis(),
    )?;
    trigger_shared_sync(&app, "weekly_review_change");
    Ok(review)
}

#[tauri::command]
pub fn delete_weekly_review(app: AppHandle, review_id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM weekly_reviews WHERE id = ?1",
            params![review_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Err("周复盘记录不存在。".to_string());
    }

    mark_entity_deleted(
        &connection,
        ENTITY_WEEKLY_REVIEW,
        review_id,
        Utc::now().timestamp_millis(),
    )?;
    connection
        .execute(
            "DELETE FROM weekly_reviews WHERE id = ?1",
            params![review_id],
        )
        .map_err(|error| error.to_string())?;
    trigger_shared_sync(&app, "weekly_review_change");
    Ok(())
}

fn get_daily_review(
    connection: &Connection,
    review_date: &str,
) -> Result<Option<DailyReview>, String> {
    connection
        .query_row(
            "
            SELECT id, review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            FROM daily_reviews
            WHERE review_date = ?1
            ",
            params![review_date],
            row_to_daily_review,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_daily_summary(
    connection: &Connection,
    review_date: &str,
) -> Result<DailyReviewSummary, String> {
    let study_seconds = connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished' AND actual_seconds >= ?2 AND started_at LIKE ?1 || '%'
            ",
            params![review_date, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let focus_session_count = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM focus_sessions
            WHERE status = 'finished' AND actual_seconds >= ?2 AND started_at LIKE ?1 || '%'
            ",
            params![review_date, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let interruption_count = connection
        .query_row(
            "
            SELECT COALESCE(SUM(interruption_count), 0)
            FROM focus_sessions
            WHERE started_at LIKE ?1 || '%'
            ",
            params![review_date],
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
            params![review_date],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let (today_total, today_completed) = connection
        .query_row(
            "
            SELECT COUNT(*), COALESCE(SUM(CASE WHEN completed = 1 THEN 1 ELSE 0 END), 0)
            FROM today_plan_items
            WHERE today_date = ?1
            ",
            params![review_date],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;

    Ok(DailyReviewSummary {
        study_seconds,
        focus_session_count,
        interruption_count,
        schedule_total,
        schedule_completed,
        today_total,
        today_completed,
    })
}

fn get_weekly_review(
    connection: &Connection,
    week_start_date: &str,
) -> Result<Option<WeeklyReview>, String> {
    connection
        .query_row(
            "
            SELECT id, week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            FROM weekly_reviews
            WHERE week_start_date = ?1
            ",
            params![week_start_date],
            row_to_weekly_review,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_weekly_summary(
    connection: &Connection,
    week_start_date: &str,
    week_end_date: &str,
) -> Result<WeeklyReviewSummary, String> {
    let next_day = shift_iso_date(week_end_date, 1)?;
    let study_seconds = connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished'
              AND actual_seconds >= ?3
              AND started_at >= ?1
              AND started_at < ?2
            ",
            params![week_start_date, next_day, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let focus_session_count = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM focus_sessions
            WHERE status = 'finished'
              AND actual_seconds >= ?3
              AND started_at >= ?1
              AND started_at < ?2
            ",
            params![week_start_date, next_day, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let interruption_count = connection
        .query_row(
            "
            SELECT COALESCE(SUM(interruption_count), 0)
            FROM focus_sessions
            WHERE started_at >= ?1 AND started_at < ?2
            ",
            params![week_start_date, next_day],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(WeeklyReviewSummary {
        study_seconds,
        focus_session_count,
        interruption_count,
    })
}

fn row_to_daily_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<DailyReview> {
    Ok(DailyReview {
        id: row.get(0)?,
        review_date: row.get(1)?,
        summary: row.get(2)?,
        blockers: row.get(3)?,
        tomorrow_focus: row.get(4)?,
        mood_score: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_weekly_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<WeeklyReview> {
    Ok(WeeklyReview {
        id: row.get(0)?,
        week_start_date: row.get(1)?,
        summary: row.get(2)?,
        blockers: row.get(3)?,
        next_week_focus: row.get(4)?,
        mood_score: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn parse_date_or_today(value: Option<&str>) -> Result<String, String> {
    match value.map(str::trim).filter(|item| !item.is_empty()) {
        Some(value) => validate_date(value),
        None => Ok(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    }
}

fn parse_week_start_or_current(value: Option<&str>) -> Result<String, String> {
    let date = match value.map(str::trim).filter(|item| !item.is_empty()) {
        Some(value) => validate_date(value)?,
        None => Local::now().date_naive().format("%Y-%m-%d").to_string(),
    };
    week_start_monday(&date)
}

fn validate_date(value: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%Y-%m-%d").to_string())
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())
}

fn week_start_monday(value: &str) -> Result<String, String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())?;
    let offset = i64::from(date.weekday().num_days_from_monday());
    Ok((date - Duration::days(offset))
        .format("%Y-%m-%d")
        .to_string())
}

fn shift_iso_date(value: &str, days: i64) -> Result<String, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| (date + Duration::days(days)).format("%Y-%m-%d").to_string())
        .map_err(|_| "日期格式应为 YYYY-MM-DD。".to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}
