use crate::{
    storage::db::open_database,
    whitelist::app::WhitelistApp,
    windows::running_processes::{list_running_processes as read_running_processes, RunningProcess},
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct RecentBlockedApp {
    pub process_name: String,
    pub process_path: Option<String>,
    pub window_title: Option<String>,
    pub blocked_count: i64,
    pub last_blocked_at: String,
}

#[tauri::command]
pub fn create_whitelist_app(
    app: AppHandle,
    name: String,
    process_name: String,
    path: Option<String>,
    note: Option<String>,
) -> Result<WhitelistApp, String> {
    let name = name.trim();
    let process_name = process_name.trim();
    let path = path.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    });

    if name.is_empty() {
        return Err("软件名称不能为空".to_string());
    }

    if process_name.is_empty() {
        return Err("进程名不能为空".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    let existing_id = connection
        .query_row(
            "
            SELECT id
            FROM whitelist_apps
            WHERE lower(process_name) = lower(?1)
            ORDER BY id DESC
            LIMIT 1
            ",
            params![process_name],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(existing_id) = existing_id {
        connection
            .execute(
                "
                UPDATE whitelist_apps
                SET name = ?1,
                    path = COALESCE(?2, path),
                    note = ?3,
                    enabled = 1,
                    updated_at = ?4
                WHERE id = ?5
                ",
                params![name, path, note, now, existing_id],
            )
            .map_err(|error| error.to_string())?;

        return get_whitelist_app_by_id(&connection, existing_id);
    }

    connection
        .execute(
            "
            INSERT INTO whitelist_apps (
              name,
              process_name,
              path,
              match_type,
              note,
              enabled,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, 'process_name', ?4, 1, ?5, ?5)
            ",
            params![name, process_name, path, note, now],
        )
        .map_err(|error| error.to_string())?;

    get_whitelist_app_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn create_whitelist_website(
    app: AppHandle,
    name: String,
    domain: String,
    note: Option<String>,
) -> Result<WhitelistApp, String> {
    let name = name.trim();
    let domain = normalize_domain(&domain);

    if name.is_empty() {
        return Err("网站名称不能为空".to_string());
    }

    if domain.is_empty() || !domain.contains('.') {
        return Err("网站域名不正确，例如 baidu.com".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    let existing_id = connection
        .query_row(
            "
            SELECT id
            FROM whitelist_apps
            WHERE match_type = 'website_domain'
              AND lower(process_name) = lower(?1)
            ORDER BY id DESC
            LIMIT 1
            ",
            params![domain],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(existing_id) = existing_id {
        connection
            .execute(
                "
                UPDATE whitelist_apps
                SET name = ?1,
                    note = ?2,
                    enabled = 1,
                    updated_at = ?3
                WHERE id = ?4
                ",
                params![name, note, now, existing_id],
            )
            .map_err(|error| error.to_string())?;

        return get_whitelist_app_by_id(&connection, existing_id);
    }

    connection
        .execute(
            "
            INSERT INTO whitelist_apps (
              name,
              process_name,
              path,
              match_type,
              note,
              enabled,
              created_at,
              updated_at
            ) VALUES (?1, ?2, NULL, 'website_domain', ?3, 1, ?4, ?4)
            ",
            params![name, domain, note, now],
        )
        .map_err(|error| error.to_string())?;

    get_whitelist_app_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn list_running_processes() -> Result<Vec<RunningProcess>, String> {
    read_running_processes()
}

#[tauri::command]
pub fn list_whitelist_apps(app: AppHandle) -> Result<Vec<WhitelistApp>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let mut statement = connection
        .prepare(
            "
            SELECT id, name, process_name, path, match_type, note, enabled, created_at, updated_at
            FROM whitelist_apps
            ORDER BY enabled DESC, id DESC
            ",
        )
        .map_err(|error| error.to_string())?;

    let apps = statement
        .query_map([], row_to_whitelist_app)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(apps)
}

#[tauri::command]
pub fn list_recent_blocked_apps(app: AppHandle) -> Result<Vec<RecentBlockedApp>, String> {
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
                   ) AS blocked_count,
                   latest.created_at AS last_blocked_at
            FROM app_events latest
            LEFT JOIN whitelist_apps w
              ON lower(w.process_name) = lower(latest.process_name)
             AND w.enabled = 1
             AND w.match_type = 'process_name'
            WHERE latest.event_type = 'blocked_foreground_detected'
              AND w.id IS NULL
              AND latest.id = (
                SELECT MAX(inner_event.id)
                FROM app_events inner_event
                WHERE inner_event.event_type = 'blocked_foreground_detected'
                  AND lower(inner_event.process_name) = lower(latest.process_name)
              )
            ORDER BY latest.created_at DESC
            LIMIT 12
            ",
        )
        .map_err(|error| error.to_string())?;

    let apps = statement
        .query_map([], |row| {
            Ok(RecentBlockedApp {
                process_name: row.get(0)?,
                process_path: row.get(1)?,
                window_title: row.get(2)?,
                blocked_count: row.get(3)?,
                last_blocked_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(apps)
}

fn normalize_domain(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

#[tauri::command]
pub fn set_whitelist_app_enabled(app: AppHandle, id: i64, enabled: bool) -> Result<WhitelistApp, String> {
    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;

    connection
        .execute(
            "
            UPDATE whitelist_apps
            SET enabled = ?1,
                updated_at = ?2
            WHERE id = ?3
            ",
            params![enabled as i64, now, id],
        )
        .map_err(|error| error.to_string())?;

    get_whitelist_app_by_id(&connection, id)
}

#[tauri::command]
pub fn delete_whitelist_app(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    connection
        .execute("DELETE FROM whitelist_apps WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn get_whitelist_app_by_id(connection: &rusqlite::Connection, id: i64) -> Result<WhitelistApp, String> {
    connection
        .query_row(
            "
            SELECT id, name, process_name, path, match_type, note, enabled, created_at, updated_at
            FROM whitelist_apps
            WHERE id = ?1
            ",
            params![id],
            row_to_whitelist_app,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "白名单应用不存在".to_string())
}

fn row_to_whitelist_app(row: &rusqlite::Row<'_>) -> rusqlite::Result<WhitelistApp> {
    let enabled: i64 = row.get(6)?;

    Ok(WhitelistApp {
        id: row.get(0)?,
        name: row.get(1)?,
        process_name: row.get(2)?,
        path: row.get(3)?,
        match_type: row.get(4)?,
        note: row.get(5)?,
        enabled: enabled != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
