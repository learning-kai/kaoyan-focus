use crate::storage::db::open_database;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct Alarm {
    pub id: i64,
    pub title: String,
    pub note: Option<String>,
    pub alarm_date: String,
    pub alarm_time: String,
    pub alarm_at: String,
    pub enabled: bool,
    pub status: String,
    pub fired_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmDraft {
    pub title: String,
    pub note: Option<String>,
    pub alarm_date: String,
    pub alarm_time: String,
    pub enabled: bool,
}

#[tauri::command]
pub fn list_alarms(app: AppHandle) -> Result<Vec<Alarm>, String> {
    let connection = open_database(&database_path(&app)?)?;
    list_all_alarms(&connection)
}

#[tauri::command]
pub fn create_alarm(app: AppHandle, draft: AlarmDraft) -> Result<Alarm, String> {
    let connection = open_database(&database_path(&app)?)?;
    let normalized = normalize_alarm_draft(draft)?;
    let now = Utc::now().to_rfc3339();

    connection
        .execute(
            "
            INSERT INTO alarms (
              title, note, alarm_date, alarm_time, alarm_at, enabled, status,
              fired_at, dismissed_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'scheduled', NULL, NULL, ?7, ?7)
            ",
            params![
                normalized.title,
                normalized.note,
                normalized.alarm_date,
                normalized.alarm_time,
                normalized.alarm_at,
                if normalized.enabled { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    get_alarm_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn update_alarm(app: AppHandle, id: i64, draft: AlarmDraft) -> Result<Alarm, String> {
    let connection = open_database(&database_path(&app)?)?;
    let normalized = normalize_alarm_draft(draft)?;
    ensure_alarm_exists(&connection, id)?;

    connection
        .execute(
            "
            UPDATE alarms
            SET title = ?1,
                note = ?2,
                alarm_date = ?3,
                alarm_time = ?4,
                alarm_at = ?5,
                enabled = ?6,
                status = 'scheduled',
                fired_at = NULL,
                dismissed_at = NULL,
                updated_at = ?7
            WHERE id = ?8
            ",
            params![
                normalized.title,
                normalized.note,
                normalized.alarm_date,
                normalized.alarm_time,
                normalized.alarm_at,
                if normalized.enabled { 1 } else { 0 },
                Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    get_alarm_by_id(&connection, id)
}

#[tauri::command]
pub fn delete_alarm(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    ensure_alarm_exists(&connection, id)?;
    connection
        .execute("DELETE FROM alarms WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_alarm_enabled(app: AppHandle, id: i64, enabled: bool) -> Result<Alarm, String> {
    let connection = open_database(&database_path(&app)?)?;
    ensure_alarm_exists(&connection, id)?;
    let now = Utc::now().to_rfc3339();

    if enabled {
        connection
            .execute(
                "
                UPDATE alarms
                SET enabled = 1,
                    status = 'scheduled',
                    fired_at = NULL,
                    dismissed_at = NULL,
                    updated_at = ?1
                WHERE id = ?2
                ",
                params![now, id],
            )
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "
                UPDATE alarms
                SET enabled = 0,
                    status = CASE WHEN status = 'ringing' THEN 'dismissed' ELSE status END,
                    dismissed_at = CASE WHEN status = 'ringing' THEN ?1 ELSE dismissed_at END,
                    updated_at = ?1
                WHERE id = ?2
                ",
                params![now, id],
            )
            .map_err(|error| error.to_string())?;
    }

    get_alarm_by_id(&connection, id)
}

#[tauri::command]
pub fn dismiss_alarm(app: AppHandle, id: i64) -> Result<Alarm, String> {
    let connection = open_database(&database_path(&app)?)?;
    ensure_alarm_exists(&connection, id)?;
    let now = Utc::now().to_rfc3339();

    connection
        .execute(
            "
            UPDATE alarms
            SET enabled = 0,
                status = 'dismissed',
                fired_at = COALESCE(fired_at, ?1),
                dismissed_at = ?1,
                updated_at = ?1
            WHERE id = ?2
            ",
            params![now, id],
        )
        .map_err(|error| error.to_string())?;

    get_alarm_by_id(&connection, id)
}

#[tauri::command]
pub fn trigger_due_alarms(app: AppHandle) -> Result<Vec<Alarm>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();

    connection
        .execute(
            "
            UPDATE alarms
            SET status = 'ringing',
                fired_at = COALESCE(fired_at, ?1),
                updated_at = ?1
            WHERE enabled = 1
              AND status = 'scheduled'
              AND alarm_at <= ?1
            ",
            params![now],
        )
        .map_err(|error| error.to_string())?;

    list_ringing_alarms(&connection)
}

#[tauri::command]
pub fn get_next_alarm(app: AppHandle) -> Result<Option<Alarm>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();
    connection
        .query_row(
            "
            SELECT id, title, note, alarm_date, alarm_time, alarm_at, enabled, status,
                   fired_at, dismissed_at, created_at, updated_at
            FROM alarms
            WHERE enabled = 1
              AND status = 'scheduled'
              AND alarm_at > ?1
            ORDER BY alarm_at ASC, id ASC
            LIMIT 1
            ",
            params![now],
            row_to_alarm,
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn has_active_alarm(app: AppHandle) -> Result<bool, String> {
    app_has_active_alarm(&app)
}

pub(crate) fn app_has_active_alarm(app: &AppHandle) -> Result<bool, String> {
    let connection = open_database(&database_path(app)?)?;
    let count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM alarms
            WHERE status = 'ringing'
               OR (enabled = 1 AND status = 'scheduled')
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count > 0)
}

struct NormalizedAlarmDraft {
    title: String,
    note: Option<String>,
    alarm_date: String,
    alarm_time: String,
    alarm_at: String,
    enabled: bool,
}

fn normalize_alarm_draft(draft: AlarmDraft) -> Result<NormalizedAlarmDraft, String> {
    let title = draft.title.trim().to_string();
    if title.is_empty() {
        return Err("闹钟标题不能为空。".to_string());
    }

    let date = NaiveDate::parse_from_str(draft.alarm_date.trim(), "%Y-%m-%d")
        .map_err(|_| "闹钟日期必须使用 YYYY-MM-DD。".to_string())?;
    let time = NaiveTime::parse_from_str(draft.alarm_time.trim(), "%H:%M")
        .map_err(|_| "闹钟时间必须使用 HH:MM。".to_string())?;
    let local_date_time = Local
        .from_local_datetime(&NaiveDateTime::new(date, time))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&NaiveDateTime::new(date, time))
                .earliest()
        })
        .ok_or_else(|| "无法识别这个本地闹钟时间。".to_string())?;

    Ok(NormalizedAlarmDraft {
        title,
        note: normalize_optional_string(draft.note),
        alarm_date: date.format("%Y-%m-%d").to_string(),
        alarm_time: time.format("%H:%M").to_string(),
        alarm_at: local_date_time.with_timezone(&Utc).to_rfc3339(),
        enabled: draft.enabled,
    })
}

fn list_all_alarms(connection: &Connection) -> Result<Vec<Alarm>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, note, alarm_date, alarm_time, alarm_at, enabled, status,
                   fired_at, dismissed_at, created_at, updated_at
            FROM alarms
            ORDER BY
              CASE
                WHEN status = 'ringing' THEN 0
                WHEN enabled = 1 AND status = 'scheduled' THEN 1
                ELSE 2
              END,
              alarm_at ASC,
              id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], row_to_alarm)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn list_ringing_alarms(connection: &Connection) -> Result<Vec<Alarm>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, note, alarm_date, alarm_time, alarm_at, enabled, status,
                   fired_at, dismissed_at, created_at, updated_at
            FROM alarms
            WHERE status = 'ringing'
            ORDER BY alarm_at ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], row_to_alarm)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn get_alarm_by_id(connection: &Connection, id: i64) -> Result<Alarm, String> {
    connection
        .query_row(
            "
            SELECT id, title, note, alarm_date, alarm_time, alarm_at, enabled, status,
                   fired_at, dismissed_at, created_at, updated_at
            FROM alarms
            WHERE id = ?1
            ",
            params![id],
            row_to_alarm,
        )
        .map_err(|error| error.to_string())
}

fn ensure_alarm_exists(connection: &Connection, id: i64) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM alarms WHERE id = ?1",
            params![id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("闹钟不存在。".to_string())
    }
}

fn row_to_alarm(row: &rusqlite::Row<'_>) -> rusqlite::Result<Alarm> {
    Ok(Alarm {
        id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        alarm_date: row.get(3)?,
        alarm_time: row.get(4)?,
        alarm_at: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        status: row.get(7)?,
        fired_at: row.get(8)?,
        dismissed_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}
