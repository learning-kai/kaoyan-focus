use crate::{
    AppState,
    focus::{session::FocusSession, subject::Subject},
    storage::db::open_database,
};
use chrono::{DateTime, Datelike, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize)]
pub struct FocusStatsSummary {
    pub today_seconds: i64,
    pub week_seconds: i64,
    pub month_seconds: i64,
    pub interruption_count: i64,
    pub subjects: Vec<SubjectStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectStats {
    pub subject: Subject,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSessionRecovery {
    pub recovery_status: String,
    pub session: FocusSession,
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
}

#[tauri::command]
pub fn start_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    planned_seconds: i64,
    mode: String,
    subject_id: Option<i64>,
) -> Result<FocusSession, String> {
    if planned_seconds <= 0 {
        return Err("专注时长必须大于 0 秒".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    connection
        .execute(
            "
            INSERT INTO focus_sessions (
              mode,
              subject_id,
              planned_seconds,
              actual_seconds,
              started_at,
              status,
              interruption_count,
              emergency_exit_count,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, 0, ?4, 'running', 0, 0, ?4, ?4)
            ",
            params![mode, subject_id, planned_seconds, now],
        )
        .map_err(|error| error.to_string())?;

    let session = get_focus_session_by_id(&connection, connection.last_insert_rowid())?;
    *state.active_session_id.lock().map_err(|error| error.to_string())? = Some(session.id);
    Ok(session)
}

#[tauri::command]
pub fn finish_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let now = Utc::now().to_rfc3339();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'finished',
                end_reason = 'completed',
                updated_at = ?2
            WHERE id = ?3
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state.active_session_id.lock().map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(session)
}

#[tauri::command]
pub fn emergency_exit_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;
    let session = get_focus_session_by_id(&connection, session_id)?;

    if session.status != "running" {
        return Err("只有进行中的严格模式专注可以应急退出".to_string());
    }

    if session.mode != "strict" {
        return Err("只有严格模式支持应急退出".to_string());
    }

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'emergency_exited',
                end_reason = 'emergency_exit',
                emergency_exit_count = emergency_exit_count + 1,
                updated_at = ?2
            WHERE id = ?3
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let updated_session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state.active_session_id.lock().map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(updated_session)
}

#[tauri::command]
pub fn interrupt_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let now = Utc::now().to_rfc3339();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'interrupted',
                end_reason = 'user_marked_interrupted',
                updated_at = ?2
            WHERE id = ?3 AND status = 'running'
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state.active_session_id.lock().map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(session)
}

#[tauri::command]
pub fn recover_active_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<FocusSessionRecovery>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let session = connection
        .query_row(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            WHERE status = 'running'
            ORDER BY id DESC
            LIMIT 1
            ",
            [],
            row_to_focus_session,
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some(session) = session else {
        *state.active_session_id.lock().map_err(|error| error.to_string())? = None;
        return Ok(None);
    };

    let started_at = DateTime::parse_from_rfc3339(&session.started_at)
        .map_err(|error| error.to_string())?
        .with_timezone(&Utc);
    let elapsed_seconds = (Utc::now() - started_at).num_seconds().max(0);

    if elapsed_seconds >= session.planned_seconds {
        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "
                UPDATE focus_sessions
                SET actual_seconds = ?1,
                    ended_at = ?2,
                    status = 'interrupted',
                    end_reason = 'recovered_after_due',
                    updated_at = ?2
                WHERE id = ?3
                ",
                params![session.planned_seconds.max(0), now, session.id],
            )
            .map_err(|error| error.to_string())?;

        *state.active_session_id.lock().map_err(|error| error.to_string())? = None;
        let updated_session = get_focus_session_by_id(&connection, session.id)?;
        return Ok(Some(FocusSessionRecovery {
            recovery_status: "interrupted_after_due".to_string(),
            session: updated_session,
            elapsed_seconds,
            remaining_seconds: 0,
        }));
    }

    *state.active_session_id.lock().map_err(|error| error.to_string())? = Some(session.id);

    Ok(Some(FocusSessionRecovery {
        recovery_status: "resumed".to_string(),
        remaining_seconds: session.planned_seconds - elapsed_seconds,
        session,
        elapsed_seconds,
    }))
}

#[tauri::command]
pub fn list_focus_sessions(app: AppHandle) -> Result<Vec<FocusSession>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            ORDER BY id DESC
            LIMIT 20
            ",
        )
        .map_err(|error| error.to_string())?;

    let sessions = statement
        .query_map([], row_to_focus_session)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(sessions)
}

#[tauri::command]
pub fn list_subjects(app: AppHandle) -> Result<Vec<Subject>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, name, color, enabled, created_at, updated_at
            FROM subjects
            WHERE enabled = 1
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let subjects = statement
        .query_map([], row_to_subject)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(subjects)
}

#[tauri::command]
pub fn get_focus_stats_summary(app: AppHandle) -> Result<FocusStatsSummary, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;
    let now = Utc::now();
    let today_prefix = now.format("%Y-%m-%d").to_string();
    let week_start = now.date_naive() - chrono::Days::new(now.weekday().num_days_from_monday() as u64);
    let month_prefix = now.format("%Y-%m").to_string();

    let today_seconds = sum_seconds_by_like(&connection, &today_prefix)?;
    let week_seconds = sum_seconds_since(&connection, &week_start.format("%Y-%m-%d").to_string())?;
    let month_seconds = sum_seconds_by_like(&connection, &month_prefix)?;
    let interruption_count = total_interruptions(&connection)?;
    let subjects = subject_stats(&connection)?;

    Ok(FocusStatsSummary {
        today_seconds,
        week_seconds,
        month_seconds,
        interruption_count,
        subjects,
    })
}

fn sum_seconds_by_like(connection: &rusqlite::Connection, prefix: &str) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished' AND started_at LIKE ?1 || '%'
            ",
            params![prefix],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn sum_seconds_since(connection: &rusqlite::Connection, date_prefix: &str) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished' AND started_at >= ?1
            ",
            params![date_prefix],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn total_interruptions(connection: &rusqlite::Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(interruption_count), 0) FROM focus_sessions",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn subject_stats(connection: &rusqlite::Connection) -> Result<Vec<SubjectStats>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT s.id, s.name, s.color, s.enabled, s.created_at, s.updated_at,
                   COALESCE(SUM(f.actual_seconds), 0) AS total_seconds
            FROM subjects s
            LEFT JOIN focus_sessions f ON f.subject_id = s.id AND f.status = 'finished'
            WHERE s.enabled = 1
            GROUP BY s.id, s.name, s.color, s.enabled, s.created_at, s.updated_at
            ORDER BY s.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let stats = statement
        .query_map([], |row| {
            Ok(SubjectStats {
                subject: Subject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    enabled: {
                        let enabled: i64 = row.get(3)?;
                        enabled != 0
                    },
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                },
                total_seconds: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(stats)
}

fn get_focus_session_by_id(connection: &rusqlite::Connection, session_id: i64) -> Result<FocusSession, String> {
    connection
        .query_row(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            WHERE id = ?1
            ",
            params![session_id],
            row_to_focus_session,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "专注记录不存在".to_string())
}

fn row_to_focus_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<FocusSession> {
    Ok(FocusSession {
        id: row.get(0)?,
        mode: row.get(1)?,
        subject_id: row.get(2)?,
        planned_seconds: row.get(3)?,
        actual_seconds: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        status: row.get(7)?,
        end_reason: row.get(8)?,
        interruption_count: row.get(9)?,
        emergency_exit_count: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn row_to_subject(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subject> {
    let enabled: i64 = row.get(3)?;

    Ok(Subject {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        enabled: enabled != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
