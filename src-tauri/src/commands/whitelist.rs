use crate::{
    storage::db::open_database,
    whitelist::app::WhitelistApp,
    windows::potplayer::{
        get_current_potplayer_media as read_current_potplayer_media, PotPlayerMediaInfo,
        POTPLAYER_DEFAULT_PROCESS_NAME,
    },
    windows::running_processes::{
        list_running_processes as read_running_processes, RunningProcess,
    },
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
    subject_id: Option<i64>,
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
    validate_subject_id(&connection, subject_id)?;
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
                    subject_id = ?3,
                    note = ?4,
                    enabled = 1,
                    updated_at = ?5
                WHERE id = ?6
                ",
                params![name, path, subject_id, note, now, existing_id],
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
              subject_id,
              note,
              enabled,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, 'process_name', ?4, ?5, 1, ?6, ?6)
            ",
            params![name, process_name, path, subject_id, note, now],
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
    subject_id: Option<i64>,
) -> Result<WhitelistApp, String> {
    let name = name.trim();
    let launch_url = website_launch_url(&domain);
    let domain = website_primary_domain(&domain);

    if name.is_empty() {
        return Err("网站名称不能为空".to_string());
    }

    if domain.is_empty() || !domain.contains('.') {
        return Err(
            "网站域名不正确，例如 baidu.com，或填写完整网址 https://www.bilibili.com/video"
                .to_string(),
        );
    }

    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    validate_subject_id(&connection, subject_id)?;
    let is_specific_url = launch_url
        .as_deref()
        .is_some_and(website_url_has_specific_path);
    let existing_id = if is_specific_url {
        connection
            .query_row(
                "
                SELECT id
                FROM whitelist_apps
                WHERE match_type = 'website_domain'
                  AND path = ?1
                ORDER BY id DESC
                LIMIT 1
                ",
                params![launch_url.as_deref()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    } else {
        connection
            .query_row(
                "
                SELECT id
                FROM whitelist_apps
                WHERE match_type = 'website_domain'
                  AND lower(process_name) = lower(?1)
                  AND (
                    path IS NULL
                    OR path = ''
                    OR lower(path) = lower(?2)
                  )
                ORDER BY id DESC
                LIMIT 1
                ",
                params![domain, launch_url.as_deref()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    }
    .map_err(|error| error.to_string())?;

    if let Some(existing_id) = existing_id {
        connection
            .execute(
                "
                UPDATE whitelist_apps
                SET name = ?1,
                    path = ?2,
                    subject_id = ?3,
                    note = ?4,
                    enabled = 1,
                    updated_at = ?5
                WHERE id = ?6
                ",
                params![name, launch_url, subject_id, note, now, existing_id],
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
              subject_id,
              note,
              enabled,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, 'website_domain', ?4, ?5, 1, ?6, ?6)
            ",
            params![name, domain, launch_url, subject_id, note, now],
        )
        .map_err(|error| error.to_string())?;

    get_whitelist_app_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn create_potplayer_video_whitelist_file(
    app: AppHandle,
    name: String,
    video_path: String,
    note: Option<String>,
    subject_id: Option<i64>,
) -> Result<WhitelistApp, String> {
    create_potplayer_video_whitelist(
        app,
        name,
        video_path,
        "potplayer_video_file",
        note,
        subject_id,
    )
}

#[tauri::command]
pub fn create_potplayer_video_whitelist_directory(
    app: AppHandle,
    name: String,
    directory_path: String,
    note: Option<String>,
    subject_id: Option<i64>,
) -> Result<WhitelistApp, String> {
    create_potplayer_video_whitelist(
        app,
        name,
        directory_path,
        "potplayer_video_directory",
        note,
        subject_id,
    )
}

#[tauri::command]
pub fn get_current_potplayer_media() -> Result<PotPlayerMediaInfo, String> {
    read_current_potplayer_media()
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
            SELECT id, name, process_name, path, match_type, subject_id, note, enabled, created_at, updated_at
            FROM whitelist_apps
            ORDER BY enabled DESC, COALESCE(subject_id, 0), id DESC
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
    let trimmed = value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("//");
    let host = trimmed
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();

    host.trim_start_matches("www.")
        .trim_start_matches("*.")
        .trim_end_matches('.')
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn normalize_file_path(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches(['\\', '/'])
        .to_string()
}

fn website_launch_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 多片段「包含模式」（含空白分隔）原样保留，交由匹配器逐一子串命中。
    if trimmed.split_whitespace().count() > 1 {
        return Some(normalize_whitespace(trimmed));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("https://{}", trimmed.trim_start_matches("//")))
    }
}

/// 把任意空白（换行 / 制表符 / 连续空格）折叠成单个空格，便于存储与展示。
fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 从用户输入中提取用于域名匹配的主域名：取第一个能解析出 host 的片段。
fn website_primary_domain(value: &str) -> String {
    value
        .split_whitespace()
        .map(normalize_domain)
        .find(|domain| domain.contains('.'))
        .unwrap_or_default()
}

fn website_url_has_specific_path(value: &str) -> bool {
    let trimmed = value.trim();

    // 多片段「包含模式」或带 query 关键词，视为具体规则，按完整字符串去重。
    if trimmed.split_whitespace().count() > 1 || trimmed.contains('=') {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    let scheme_length = if lower.starts_with("http://") {
        "http://".len()
    } else if lower.starts_with("https://") {
        "https://".len()
    } else {
        0
    };
    let without_scheme = &trimmed[scheme_length..];
    let Some(path_start) = without_scheme.find(['/', '?', '#']) else {
        return false;
    };
    let path = without_scheme[path_start..]
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_end_matches('/');

    !path.is_empty() && path != "/"
}

fn create_potplayer_video_whitelist(
    app: AppHandle,
    name: String,
    media_path: String,
    match_type: &str,
    note: Option<String>,
    subject_id: Option<i64>,
) -> Result<WhitelistApp, String> {
    let name = name.trim();
    let media_path = normalize_file_path(&media_path);

    if name.is_empty() {
        return Err("PotPlayer 规则名称不能为空".to_string());
    }

    if media_path.is_empty() {
        return Err("PotPlayer 视频路径不能为空".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    validate_subject_id(&connection, subject_id)?;
    let existing_id = connection
        .query_row(
            "
            SELECT id
            FROM whitelist_apps
            WHERE match_type = ?1
              AND lower(process_name) = lower(?2)
              AND lower(path) = lower(?3)
            ORDER BY id DESC
            LIMIT 1
            ",
            params![match_type, POTPLAYER_DEFAULT_PROCESS_NAME, media_path],
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
                    subject_id = ?2,
                    note = ?3,
                    enabled = 1,
                    updated_at = ?4
                WHERE id = ?5
                ",
                params![name, subject_id, note, now, existing_id],
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
              subject_id,
              note,
              enabled,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)
            ",
            params![
                name,
                POTPLAYER_DEFAULT_PROCESS_NAME,
                media_path,
                match_type,
                subject_id,
                note,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    get_whitelist_app_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn set_whitelist_app_enabled(
    app: AppHandle,
    id: i64,
    enabled: bool,
) -> Result<WhitelistApp, String> {
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
pub fn update_whitelist_subject(
    app: AppHandle,
    id: i64,
    subject_id: Option<i64>,
) -> Result<WhitelistApp, String> {
    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    validate_subject_id(&connection, subject_id)?;

    let changed = connection
        .execute(
            "
            UPDATE whitelist_apps
            SET subject_id = ?1,
                updated_at = ?2
            WHERE id = ?3
            ",
            params![subject_id, now, id],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("白名单条目不存在".to_string());
    }

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

fn get_whitelist_app_by_id(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<WhitelistApp, String> {
    connection
        .query_row(
            "
            SELECT id, name, process_name, path, match_type, subject_id, note, enabled, created_at, updated_at
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
    let enabled: i64 = row.get(7)?;

    Ok(WhitelistApp {
        id: row.get(0)?,
        name: row.get(1)?,
        process_name: row.get(2)?,
        path: row.get(3)?,
        match_type: row.get(4)?,
        subject_id: row.get(5)?,
        note: row.get(6)?,
        enabled: enabled != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn validate_subject_id(
    connection: &rusqlite::Connection,
    subject_id: Option<i64>,
) -> Result<(), String> {
    if let Some(subject_id) = subject_id {
        let exists = connection
            .query_row(
                "SELECT 1 FROM subjects WHERE id = ?1 AND enabled = 1",
                params![subject_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err("科目不存在或已停用".to_string());
        }
    }
    Ok(())
}
