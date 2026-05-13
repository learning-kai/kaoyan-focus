use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::storage::db::open_database;
use chrono::Utc;
use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE},
    Method, StatusCode, Url,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const WEBDAV_URL_KEY: &str = "webdav_url";
const WEBDAV_USERNAME_KEY: &str = "webdav_username";
const WEBDAV_PASSWORD_KEY: &str = "webdav_password";
const WEBDAV_REMOTE_PATH_KEY: &str = "webdav_remote_path";
const DEFAULT_REMOTE_PATH: &str = "kaoyan-focus/kaoyan-focus.sqlite3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavSettings {
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebDavStatus {
    pub configured: bool,
    pub url: String,
    pub username: String,
    pub remote_path: String,
    pub remote_exists: bool,
    pub remote_size: Option<u64>,
    pub last_modified: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebDavSyncResult {
    pub success: bool,
    pub message: String,
    pub remote_url: String,
    pub bytes: u64,
    pub backup_path: Option<String>,
}

#[tauri::command]
pub fn get_webdav_settings(app: AppHandle) -> Result<WebDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;

    Ok(WebDavSettings {
        url: get_setting(&connection, WEBDAV_URL_KEY, "")?,
        username: get_setting(&connection, WEBDAV_USERNAME_KEY, "")?,
        password: get_setting(&connection, WEBDAV_PASSWORD_KEY, "")?,
        remote_path: get_setting(&connection, WEBDAV_REMOTE_PATH_KEY, DEFAULT_REMOTE_PATH)?,
    })
}

#[tauri::command]
pub fn save_webdav_settings(app: AppHandle, settings: WebDavSettings) -> Result<WebDavSettings, String> {
    let normalized = normalize_settings(settings)?;
    let connection = open_database(&database_path(&app)?)?;
    persist_webdav_settings(&connection, &normalized)?;

    Ok(normalized)
}

#[tauri::command]
pub fn test_webdav_connection(app: AppHandle, settings: WebDavSettings) -> Result<WebDavStatus, String> {
    let normalized = save_webdav_settings(app, settings)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?, remote_url.clone(), &normalized)
        .header("Depth", "0")
        .body("")
        .send()
        .map_err(|error| format!("连接 WebDAV 失败：{error}"))?;

    let status = response.status();
    if status == StatusCode::OK || status.as_u16() == 207 {
        let headers = response.headers().clone();
        return Ok(WebDavStatus {
            configured: true,
            url: normalized.url,
            username: normalized.username,
            remote_path: normalized.remote_path,
            remote_exists: true,
            remote_size: content_length(&headers),
            last_modified: header_string(&headers, "last-modified"),
            message: "WebDAV 连接成功，远端同步文件可访问。".to_string(),
        });
    }

    if status == StatusCode::NOT_FOUND {
        return Ok(WebDavStatus {
            configured: true,
            url: normalized.url,
            username: normalized.username,
            remote_path: normalized.remote_path,
            remote_exists: false,
            remote_size: None,
            last_modified: None,
            message: "WebDAV 连接成功，远端文件尚未创建。可以先上传本地数据。".to_string(),
        });
    }

    Err(format!("WebDAV 返回异常状态：{}", status.as_u16()))
}

#[tauri::command]
pub fn upload_database_to_webdav(app: AppHandle, settings: WebDavSettings) -> Result<WebDavSyncResult, String> {
    let normalized = save_webdav_settings(app.clone(), settings)?;
    let database_path = database_path(&app)?;
    let bytes = fs::read(&database_path).map_err(|error| format!("读取本地数据库失败：{error}"))?;

    if bytes.is_empty() {
        return Err("本地数据库为空，已取消上传。".to_string());
    }

    let client = webdav_client()?;
    ensure_remote_directories(&client, &normalized)?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::PUT, remote_url.clone(), &normalized)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|error| format!("上传到 WebDAV 失败：{error}"))?;

    if response.status().is_success() || response.status() == StatusCode::CREATED || response.status() == StatusCode::NO_CONTENT {
        return Ok(WebDavSyncResult {
            success: true,
            message: "本地数据已上传到 WebDAV。".to_string(),
            remote_url: remote_url.to_string(),
            bytes: fs::metadata(&database_path).map(|meta| meta.len()).unwrap_or(0),
            backup_path: None,
        });
    }

    Err(format!("上传失败，WebDAV 返回状态：{}", response.status().as_u16()))
}

#[tauri::command]
pub fn download_database_from_webdav(app: AppHandle, settings: WebDavSettings) -> Result<WebDavSyncResult, String> {
    let normalized = normalize_settings(settings)?;
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::GET, remote_url.clone(), &normalized)
        .send()
        .map_err(|error| format!("从 WebDAV 下载失败：{error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Err("远端同步文件不存在，请先在另一台设备上传数据。".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("下载失败，WebDAV 返回状态：{}", response.status().as_u16()));
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("读取远端数据失败：{error}"))?;

    if bytes.is_empty() {
        return Err("远端数据库文件为空，已取消恢复。".to_string());
    }

    let app_data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let temp_path = app_data_dir.join("kaoyan-focus.webdav-download.tmp");
    fs::write(&temp_path, &bytes).map_err(|error| format!("写入临时文件失败：{error}"))?;
    validate_sqlite_database(&temp_path)?;

    let backup_path = backup_database_path(&app_data_dir);
    let backup_created = local_database_path.exists();
    if backup_created {
        fs::copy(&local_database_path, &backup_path).map_err(|error| format!("备份本地数据库失败：{error}"))?;
    }

    fs::rename(&temp_path, &local_database_path).or_else(|_| {
        fs::copy(&temp_path, &local_database_path)?;
        fs::remove_file(&temp_path)
    }).map_err(|error| format!("替换本地数据库失败：{error}"))?;

    let connection = open_database(&local_database_path)?;
    persist_webdav_settings(&connection, &normalized)?;

    Ok(WebDavSyncResult {
        success: true,
        message: "已从 WebDAV 恢复数据，本地旧数据库已备份。".to_string(),
        remote_url: remote_url.to_string(),
        bytes: bytes.len() as u64,
        backup_path: backup_created.then(|| backup_path.to_string_lossy().to_string()),
    })
}

fn normalize_settings(settings: WebDavSettings) -> Result<WebDavSettings, String> {
    let url = settings.url.trim().trim_end_matches('/').to_string();
    let username = settings.username.trim().to_string();
    let password = settings.password;
    let remote_path = settings
        .remote_path
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/");

    if url.is_empty() {
        return Err("请填写 WebDAV 地址。".to_string());
    }

    Url::parse(&url).map_err(|_| "WebDAV 地址格式不正确，请填写 http 或 https 地址。".to_string())?;

    if remote_path.is_empty() {
        return Err("请填写远端文件路径。".to_string());
    }

    Ok(WebDavSettings {
        url,
        username,
        password,
        remote_path,
    })
}

fn webdav_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

fn webdav_request(
    client: &Client,
    method: Method,
    url: Url,
    settings: &WebDavSettings,
) -> reqwest::blocking::RequestBuilder {
    let request = client.request(method, url);
    if settings.username.is_empty() && settings.password.is_empty() {
        request
    } else {
        request.basic_auth(&settings.username, Some(&settings.password))
    }
}

fn remote_file_url(settings: &WebDavSettings) -> Result<Url, String> {
    let base = format!("{}/", settings.url.trim_end_matches('/'));
    let mut url = Url::parse(&base).map_err(|error| error.to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "WebDAV 地址不支持路径拼接。".to_string())?;
        for segment in settings.remote_path.split('/').filter(|segment| !segment.is_empty()) {
            segments.push(segment);
        }
    }
    Ok(url)
}

fn ensure_remote_directories(client: &Client, settings: &WebDavSettings) -> Result<(), String> {
    let mut current = Url::parse(&format!("{}/", settings.url.trim_end_matches('/')))
        .map_err(|error| error.to_string())?;
    let parts: Vec<&str> = settings.remote_path.split('/').filter(|part| !part.is_empty()).collect();

    if parts.len() <= 1 {
        return Ok(());
    }

    for part in &parts[..parts.len() - 1] {
        {
            let mut segments = current
                .path_segments_mut()
                .map_err(|_| "WebDAV 地址不支持目录创建。".to_string())?;
            segments.push(part);
        }

        let response = webdav_request(
            client,
            Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
            current.clone(),
            settings,
        )
        .send()
        .map_err(|error| format!("创建远端目录失败：{error}"))?;

        if response.status().is_success()
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
            || response.status() == StatusCode::CONFLICT
        {
            continue;
        }

        return Err(format!("创建远端目录失败，WebDAV 返回状态：{}", response.status().as_u16()));
    }

    Ok(())
}

fn validate_sqlite_database(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| format!("远端文件不是有效 SQLite 数据库：{error}"))?;
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("校验远端数据库失败：{error}"))
        .and_then(|result| {
            if result == "ok" {
                Ok(())
            } else {
                Err(format!("远端数据库完整性检查失败：{result}"))
            }
        })
}

fn ensure_no_active_runtime(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let connection = open_database(path)?;
    let active_study_modes: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM study_modes WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let running_sessions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM focus_sessions WHERE status = 'running'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    if active_study_modes > 0 || running_sessions > 0 {
        return Err("当前有进行中的学习模式，请等本次学习自然完成后再从 WebDAV 恢复数据。".to_string());
    }

    Ok(())
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn backup_database_path(app_data_dir: &Path) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    app_data_dir.join(format!("kaoyan-focus.before-webdav-{stamp}.sqlite3"))
}

fn get_setting(connection: &Connection, key: &str, fallback: &str) -> Result<String, String> {
    Ok(connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn set_setting(connection: &Connection, key: &str, value: &str, updated_at: &str) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            (key, value, updated_at),
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn persist_webdav_settings(connection: &Connection, settings: &WebDavSettings) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(connection, WEBDAV_URL_KEY, &settings.url, &now)?;
    set_setting(connection, WEBDAV_USERNAME_KEY, &settings.username, &now)?;
    set_setting(connection, WEBDAV_PASSWORD_KEY, &settings.password, &now)?;
    set_setting(connection, WEBDAV_REMOTE_PATH_KEY, &settings.remote_path, &now)?;
    Ok(())
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value: &HeaderValue| value.to_str().ok())
        .map(ToString::to_string)
}
