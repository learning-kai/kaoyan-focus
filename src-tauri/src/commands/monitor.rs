use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    storage::db::open_database,
    whitelist::matcher::{
        is_foreground_app_allowed, ProcessWhitelistRule, WebsiteWhitelistRule, WhitelistMatchResult,
    },
    windows::{
        foreground::{get_foreground_app, ForegroundApp},
        window_control::close_window,
    },
    AppState,
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
    let connection = open_database(&database_path(app)?)?;
    let active: Option<(String, bool, String)> = connection
        .query_row(
            "
            SELECT mode, whitelist_enabled, phase
            FROM study_modes
            WHERE status = 'active' AND current_session_id = ?1
            ORDER BY id DESC
            LIMIT 1
            ",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some((mode, whitelist_enabled, phase)) = active {
        if !study_mode_enforces_whitelist(&mode, whitelist_enabled, &phase) {
            return build_non_enforcing_focus_check(
                app,
                state,
                Some(session_id),
                non_enforcing_reason(&mode, whitelist_enabled),
            );
        }
    }

    check_focus_foreground_app_internal(app, state, Some(session_id), session_id)
}

pub fn check_focus_foreground_app_for_active_mode(
    app: &AppHandle,
    state: &AppState,
) -> Result<FocusAppCheck, String> {
    let connection = open_database(&database_path(app)?)?;
    let active: Option<(i64, Option<i64>, String, bool, String)> = connection
        .query_row(
            "
            SELECT id, current_session_id, mode, whitelist_enabled, phase
            FROM study_modes
            WHERE status = 'active'
            ORDER BY id DESC
            LIMIT 1
            ",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some((study_mode_id, session_id, mode, whitelist_enabled, phase)) = active else {
        return Err("No active study mode to monitor.".to_string());
    };
    if !study_mode_enforces_whitelist(&mode, whitelist_enabled, &phase) {
        return build_non_enforcing_focus_check(
            app,
            state,
            session_id,
            non_enforcing_reason(&mode, whitelist_enabled),
        );
    }
    if let Some(session_id) = session_id {
        return check_focus_foreground_app_for_session(app, state, session_id);
    }

    check_focus_foreground_app_internal(app, state, None, -study_mode_id)
}

fn study_mode_enforces_whitelist(mode: &str, whitelist_enabled: bool, phase: &str) -> bool {
    (mode == "strict" || whitelist_enabled) && matches!(phase, "focus" | "awaiting_break")
}

fn non_enforcing_reason(mode: &str, whitelist_enabled: bool) -> &'static str {
    if mode != "strict" && !whitelist_enabled {
        "白名单已关闭"
    } else {
        "白名单未执行"
    }
}

fn build_non_enforcing_focus_check(
    app: &AppHandle,
    state: &AppState,
    session_id: Option<i64>,
    reason: &'static str,
) -> Result<FocusAppCheck, String> {
    let foreground_app = get_foreground_app()?;
    *state
        .last_blocked_process
        .lock()
        .map_err(|error| error.to_string())? = None;
    let interruption_count = if let Some(session_id) = session_id {
        let connection = open_database(&database_path(app)?)?;
        connection
            .query_row(
                "SELECT interruption_count FROM focus_sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or(0)
    } else {
        0
    };

    Ok(FocusAppCheck {
        foreground_app,
        match_result: WhitelistMatchResult {
            allowed: true,
            reason: reason.to_string(),
            matched_process_name: None,
            detected_domain: None,
            matched_subject_id: None,
        },
        interruption_count,
        action_taken: None,
        close_error: None,
    })
}

fn check_focus_foreground_app_internal(
    app: &AppHandle,
    state: &AppState,
    session_id: Option<i64>,
    dedupe_scope_id: i64,
) -> Result<FocusAppCheck, String> {
    let foreground_app = get_foreground_app()?;
    let connection = open_database(&database_path(app)?)?;
    let whitelist_processes = enabled_whitelist_processes(&connection)?;
    let whitelist_websites = enabled_whitelist_websites(&connection)?;
    let match_result =
        is_foreground_app_allowed(&foreground_app, &whitelist_processes, &whitelist_websites);
    let mut action_taken = None;
    let mut close_error = None;

    if match_result.allowed {
        if maybe_auto_switch_subject(&connection, &match_result)? {
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = crate::commands::sync::sync_object_storage_after_external_change(
                    app,
                    "focus_state_change",
                );
            });
        }
        *state
            .last_blocked_process
            .lock()
            .map_err(|error| error.to_string())? = None;
    } else {
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
            let current = (
                dedupe_scope_id,
                foreground_app.process_name.to_ascii_lowercase(),
            );
            let should_record = last_blocked_process.as_ref() != Some(&current);
            *last_blocked_process = Some(current);
            should_record
        };

        if should_record {
            let now = Utc::now().to_rfc3339();
            if let Some(session_id) = session_id {
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
        }
    }

    let interruption_count = if let Some(session_id) = session_id {
        connection
            .query_row(
                "SELECT interruption_count FROM focus_sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?
    } else {
        0
    };

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

fn enabled_whitelist_processes(
    connection: &rusqlite::Connection,
) -> Result<Vec<ProcessWhitelistRule>, String> {
    let mut statement = connection
        .prepare("SELECT process_name, subject_id FROM whitelist_apps WHERE enabled = 1 AND match_type = 'process_name'")
        .map_err(|error| error.to_string())?;

    let processes = statement
        .query_map([], |row| {
            Ok(ProcessWhitelistRule {
                process_name: row.get(0)?,
                subject_id: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(processes)
}

fn enabled_whitelist_websites(
    connection: &rusqlite::Connection,
) -> Result<Vec<WebsiteWhitelistRule>, String> {
    let mut statement = connection
        .prepare("SELECT process_name, path, subject_id FROM whitelist_apps WHERE enabled = 1 AND match_type = 'website_domain'")
        .map_err(|error| error.to_string())?;

    let websites = statement
        .query_map([], |row| {
            let domain = row.get::<_, String>(0)?;
            let launch_url = row.get::<_, Option<String>>(1)?;
            let subject_id = row.get::<_, Option<i64>>(2)?;
            let launch_url = launch_url.or_else(|| legacy_specific_url(&domain));
            Ok(WebsiteWhitelistRule {
                domain,
                launch_url,
                subject_id,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(websites)
}

fn maybe_auto_switch_subject(
    connection: &rusqlite::Connection,
    match_result: &WhitelistMatchResult,
) -> Result<bool, String> {
    let Some(subject_id) = match_result.matched_subject_id else {
        return Ok(false);
    };

    let active: Option<(i64, Option<i64>, Option<i64>)> = connection
        .query_row(
            "
            SELECT id, subject_id, current_session_id
            FROM study_modes
            WHERE status = 'active'
            ORDER BY id DESC
            LIMIT 1
            ",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some((study_mode_id, current_subject_id, session_id)) = active else {
        return Ok(false);
    };
    if current_subject_id == Some(subject_id) {
        return Ok(false);
    }

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE study_modes
            SET subject_id = ?1,
                updated_at = ?2
            WHERE id = ?3 AND status = 'active'
            ",
            params![subject_id, now, study_mode_id],
        )
        .map_err(|error| error.to_string())?;

    if let Some(session_id) = session_id {
        connection
            .execute(
                "
                UPDATE focus_sessions
                SET subject_id = ?1,
                    updated_at = ?2
                WHERE id = ?3 AND status = 'running'
                ",
                params![subject_id, now, session_id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(true)
}

fn legacy_specific_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.contains('/') {
        Some(trimmed.to_string())
    } else {
        None
    }
}
