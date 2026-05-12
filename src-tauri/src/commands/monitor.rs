use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    AppState,
    storage::db::open_database,
    whitelist::matcher::{is_foreground_app_allowed, WhitelistMatchResult},
    windows::{
        foreground::{get_foreground_app, ForegroundApp},
        window_control::close_window,
    },
};

#[derive(Debug, Clone, Serialize)]
pub struct FocusAppCheck {
    pub foreground_app: ForegroundApp,
    pub match_result: WhitelistMatchResult,
    pub interruption_count: i64,
    pub action_taken: Option<String>,
    pub close_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterruptionSummary {
    pub process_name: String,
    pub process_path: Option<String>,
    pub window_title: Option<String>,
    pub interruption_count: i64,
    pub last_interrupted_at: String,
}

#[tauri::command]
pub fn get_current_foreground_app() -> Result<ForegroundApp, String> {
    get_foreground_app()
}

#[tauri::command]
pub fn check_focus_foreground_app(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<FocusAppCheck, String> {
    check_focus_foreground_app_for_session(&app, state.inner(), session_id)
}

pub fn check_focus_foreground_app_for_session(
    app: &AppHandle,
    state: &AppState,
    session_id: i64,
) -> Result<FocusAppCheck, String> {
    let foreground_app = get_foreground_app()?;
    let connection = open_database(&database_path(app)?)?;
    let whitelist_process_names = enabled_whitelist_process_names(&connection)?;
    let whitelist_domains = enabled_whitelist_domains(&connection)?;
    let match_result = is_foreground_app_allowed(&foreground_app, &whitelist_process_names, &whitelist_domains);
    let mut action_taken = None;
    let mut close_error = None;

    if !match_result.allowed {
        match close_window(foreground_app.window()) {
            Ok(()) => {
                action_taken = Some("close_window".to_string());
            }
            Err(error) => {
                action_taken = Some("close_window_failed".to_string());
                close_error = Some(error);
            }
        }

        let should_record = {
            let mut last_blocked_process = state
                .last_blocked_process
                .lock()
                .map_err(|error| error.to_string())?;
            let current = (session_id, foreground_app.process_name.to_ascii_lowercase());
            let should_record = last_blocked_process.as_ref() != Some(&current);
            *last_blocked_process = Some(current);
            should_record
        };

        if should_record {
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "
                    INSERT INTO app_events (
                      session_id,
                      process_name,
                      process_path,
                      window_title,
                      event_type,
                      action_taken,
                      created_at
                    ) VALUES (?1, ?2, ?3, ?4, 'blocked_foreground_detected', ?5, ?6)
                    ",
                    params![
                        session_id,
                        foreground_app.process_name,
                        foreground_app.process_path,
                        foreground_app.window_title,
                        action_taken.as_deref().unwrap_or("close_window"),
                        now
                    ],
                )
                .map_err(|error| error.to_string())?;

            connection
                .execute(
                    "
                    UPDATE focus_sessions
                    SET interruption_count = interruption_count + 1,
                        updated_at = ?1
                    WHERE id = ?2
                    ",
                    params![now, session_id],
                )
                .map_err(|error| error.to_string())?;
        }
    } else {
        *state
            .last_blocked_process
            .lock()
            .map_err(|error| error.to_string())? = None;
    }

    let interruption_count = connection
        .query_row(
            "SELECT interruption_count FROM focus_sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(FocusAppCheck {
        foreground_app,
        match_result,
        interruption_count,
        action_taken,
        close_error,
    })
}

#[tauri::command]
pub fn list_interruption_summary(app: AppHandle) -> Result<Vec<InterruptionSummary>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let mut statement = connection
        .prepare(
            "
            SELECT latest.process_name,
                   latest.process_path,
                   latest.window_title,
                   (
                     SELECT COUNT(*)
                     FROM app_events counted
                     WHERE counted.event_type = 'blocked_foreground_detected'
                       AND lower(counted.process_name) = lower(latest.process_name)
                   ) AS interruption_count,
                   latest.created_at AS last_interrupted_at
            FROM app_events latest
            WHERE latest.event_type = 'blocked_foreground_detected'
              AND latest.id = (
                SELECT MAX(inner_event.id)
                FROM app_events inner_event
                WHERE inner_event.event_type = 'blocked_foreground_detected'
                  AND lower(inner_event.process_name) = lower(latest.process_name)
              )
            ORDER BY interruption_count DESC, latest.created_at DESC
            LIMIT 8
            ",
        )
        .map_err(|error| error.to_string())?;

    let summary = statement
        .query_map([], |row| {
            Ok(InterruptionSummary {
                process_name: row.get(0)?,
                process_path: row.get(1)?,
                window_title: row.get(2)?,
                interruption_count: row.get(3)?,
                last_interrupted_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(summary)
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn enabled_whitelist_process_names(connection: &rusqlite::Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT process_name FROM whitelist_apps WHERE enabled = 1 AND match_type = 'process_name'")
        .map_err(|error| error.to_string())?;

    let process_names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(process_names)
}

fn enabled_whitelist_domains(connection: &rusqlite::Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT process_name FROM whitelist_apps WHERE enabled = 1 AND match_type = 'website_domain'")
        .map_err(|error| error.to_string())?;

    let domains = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(domains)
}
