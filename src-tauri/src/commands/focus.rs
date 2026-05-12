use crate::{focus::session::FocusSession, storage::db::open_database, AppState};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn start_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    planned_seconds: i64,
    mode: String,
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
              planned_seconds,
              actual_seconds,
              started_at,
              status,
              interruption_count,
              emergency_exit_count,
              created_at,
              updated_at
            ) VALUES (?1, ?2, 0, ?3, 'running', 0, 0, ?3, ?3)
            ",
            params![mode, planned_seconds, now],
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
            SELECT id, mode, planned_seconds, actual_seconds, started_at, ended_at,
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

fn get_focus_session_by_id(connection: &rusqlite::Connection, session_id: i64) -> Result<FocusSession, String> {
    connection
        .query_row(
            "
            SELECT id, mode, planned_seconds, actual_seconds, started_at, ended_at,
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
        planned_seconds: row.get(2)?,
        actual_seconds: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        status: row.get(6)?,
        end_reason: row.get(7)?,
        interruption_count: row.get(8)?,
        emergency_exit_count: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}
