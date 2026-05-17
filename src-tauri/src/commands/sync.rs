use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    storage::db::open_database,
    sync_package::{
        export_shared_sync_payload, import_shared_sync_payload, load_or_create_device_id,
        merge_shared_sync_payloads, shared_active_study_snapshot, SharedSyncPayload,
    },
};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    config::{Builder as S3ConfigBuilder, Region},
    primitives::ByteStream,
    Client as S3Client,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED},
    Method, StatusCode, Url,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json;
use tauri::{AppHandle, Emitter, Manager};

const WEBDAV_URL_KEY: &str = "webdav_url";
const WEBDAV_USERNAME_KEY: &str = "webdav_username";
const WEBDAV_PASSWORD_KEY: &str = "webdav_password";
const WEBDAV_REMOTE_PATH_KEY: &str = "webdav_remote_path";
const WEBDAV_ENABLED_KEY: &str = "webdav_sync_enabled";
const OBJECT_STORAGE_ENDPOINT_KEY: &str = "object_storage_endpoint";
const OBJECT_STORAGE_BUCKET_KEY: &str = "object_storage_bucket";
const OBJECT_STORAGE_ACCESS_KEY_ID_KEY: &str = "object_storage_access_key_id";
const OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY: &str = "object_storage_secret_access_key";
const OBJECT_STORAGE_REGION_KEY: &str = "object_storage_region";
const OBJECT_STORAGE_OBJECT_KEY_KEY: &str = "object_storage_object_key";
const OBJECT_STORAGE_ENABLED_KEY: &str = "object_storage_sync_enabled";
const DEFAULT_REMOTE_PATH: &str = "kaoyan-focus/kaoyan-focus.sqlite3";
const DEFAULT_OBJECT_KEY: &str = "study-sync.json";
const DEFAULT_OBJECT_REGION: &str = "auto";
const STUDY_SYNC_STATE_CHANGED_EVENT: &str = "study-sync-state-changed";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavSettings {
    pub enabled: bool,
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

#[derive(Debug, Clone, Serialize)]
pub struct WebDavAutoSyncResult {
    pub status: String,
    pub message: String,
    pub direction: Option<String>,
    pub skipped_reason: Option<String>,
    pub synced_at: String,
    pub remote_url: Option<String>,
    pub bytes: u64,
    pub backup_path: Option<String>,
    pub active_state_changed: bool,
    pub took_over_active_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub object_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectStorageStatus {
    pub configured: bool,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub object_key: String,
    pub object_exists: bool,
    pub object_size: Option<u64>,
    pub last_modified: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectStorageSyncResult {
    pub success: bool,
    pub message: String,
    pub object_url: String,
    pub bytes: u64,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectStorageAutoSyncResult {
    pub status: String,
    pub message: String,
    pub direction: Option<String>,
    pub skipped_reason: Option<String>,
    pub synced_at: String,
    pub object_url: Option<String>,
    pub bytes: u64,
    pub backup_path: Option<String>,
    pub active_state_changed: bool,
    pub took_over_active_mode: bool,
}

#[derive(Debug, Clone)]
struct RemoteFileMetadata {
    exists: bool,
    size: Option<u64>,
    last_modified: Option<DateTime<Utc>>,
}

#[tauri::command]
pub fn get_webdav_settings(app: AppHandle) -> Result<WebDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;

    Ok(WebDavSettings {
        enabled: get_bool_setting(&connection, WEBDAV_ENABLED_KEY, true)?,
        url: get_setting(&connection, WEBDAV_URL_KEY, "")?,
        username: get_setting(&connection, WEBDAV_USERNAME_KEY, "")?,
        password: get_setting(&connection, WEBDAV_PASSWORD_KEY, "")?,
        remote_path: get_setting(&connection, WEBDAV_REMOTE_PATH_KEY, DEFAULT_REMOTE_PATH)?,
    })
}

#[tauri::command]
pub fn save_webdav_settings(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSettings, String> {
    let normalized = normalize_settings(settings)?;
    let connection = open_database(&database_path(&app)?)?;
    persist_webdav_settings(&connection, &normalized)?;

    Ok(normalized)
}

#[tauri::command]
pub fn test_webdav_connection(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavStatus, String> {
    let normalized = save_webdav_settings(app, settings)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(
        &client,
        Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?,
        remote_url.clone(),
        &normalized,
    )
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
            message: "WebDAV 连接成功，远端同步文件尚未创建，可先上传本地数据。".to_string(),
        });
    }

    Err(format!("WebDAV 连接失败，返回状态：{}", status.as_u16()))
}

#[tauri::command]
pub fn upload_database_to_webdav(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSyncResult, String> {
    let normalized = save_webdav_settings(app.clone(), settings)?;
    let database_path = database_path(&app)?;
    let bytes = fs::read(&database_path).map_err(|error| format!("读取本地数据库失败：{error}"))?;

    if bytes.is_empty() {
        return Err("本地数据库为空，无法上传。".to_string());
    }

    let client = webdav_client()?;
    ensure_remote_directories(&client, &normalized)?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::PUT, remote_url.clone(), &normalized)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|error| format!("上传到 WebDAV 失败：{error}"))?;

    if response.status().is_success()
        || response.status() == StatusCode::CREATED
        || response.status() == StatusCode::NO_CONTENT
    {
        return Ok(WebDavSyncResult {
            success: true,
            message: "已成功上传到 WebDAV。".to_string(),
            remote_url: remote_url.to_string(),
            bytes: fs::metadata(&database_path)
                .map(|meta| meta.len())
                .unwrap_or(0),
            backup_path: None,
        });
    }

    Err(format!(
        "上传到 WebDAV 失败，返回状态：{}",
        response.status().as_u16()
    ))
}

#[tauri::command]
pub fn download_database_from_webdav(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSyncResult, String> {
    let normalized = normalize_settings(settings)?;
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::GET, remote_url.clone(), &normalized)
        .send()
        .map_err(|error| format!("从 WebDAV 下载失败：{error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Err("远端 WebDAV 同步文件不存在。".to_string());
    }

    if !response.status().is_success() {
        return Err(format!(
            "从 WebDAV 下载失败，返回状态：{}",
            response.status().as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("读取 WebDAV 响应失败：{error}"))?;

    if bytes.is_empty() {
        return Err("WebDAV 返回的文件为空。".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let temp_path = app_data_dir.join("kaoyan-focus.webdav-download.tmp");
    fs::write(&temp_path, &bytes).map_err(|error| format!("写入临时文件失败：{error}"))?;
    validate_sqlite_database(&temp_path)?;

    let backup_path = backup_database_path(&app_data_dir);
    let backup_created = local_database_path.exists();
    if backup_created {
        fs::copy(&local_database_path, &backup_path)
            .map_err(|error| format!("创建本地备份失败：{error}"))?;
    }

    fs::rename(&temp_path, &local_database_path)
        .or_else(|_| {
            fs::copy(&temp_path, &local_database_path)?;
            fs::remove_file(&temp_path)
        })
        .map_err(|error| format!("替换本地数据库失败：{error}"))?;

    let connection = open_database(&local_database_path)?;
    persist_webdav_settings(&connection, &normalized)?;

    Ok(WebDavSyncResult {
        success: true,
        message: "已从 WebDAV 下载并校验数据库。".to_string(),
        remote_url: remote_url.to_string(),
        bytes: bytes.len() as u64,
        backup_path: backup_created.then(|| backup_path.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub fn auto_sync_webdav_database(app: AppHandle) -> Result<WebDavAutoSyncResult, String> {
    let settings = get_webdav_settings(app.clone())?;
    if !settings.enabled {
        return Ok(skipped_auto_sync(
            "webdav_disabled",
            "WebDAV 同步已关闭，已跳过自动同步。",
            None,
        ));
    }

    if settings.url.trim().is_empty() {
        return Ok(skipped_auto_sync(
            "webdav_not_configured",
            "未配置 WebDAV，已跳过自动同步。",
            None,
        ));
    }

    let normalized = normalize_settings(settings)?;
    let local_database_path = database_path(&app)?;
    if has_active_runtime(&local_database_path)? {
        return Ok(skipped_auto_sync(
            "study_mode_active",
            "学习模式正在运行，已跳过 WebDAV 自动同步。",
            Some(remote_file_url(&normalized)?.to_string()),
        ));
    }

    let local_modified = match local_database_modified_at(&local_database_path) {
        Ok(value) => value,
        Err(message) => {
            return Ok(skipped_auto_sync(
                "local_timestamp_unavailable",
                &message,
                Some(remote_file_url(&normalized)?.to_string()),
            ));
        }
    };

    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let remote_metadata = fetch_remote_file_metadata(&client, &remote_url, &normalized)?;

    if !remote_metadata.exists {
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "远端同步文件不存在，已上传本地数据库。".to_string(),
            direction: Some("upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: None,
            active_state_changed: false,
            took_over_active_mode: false,
        });
    }

    let Some(remote_modified) = remote_metadata.last_modified else {
        return Ok(skipped_auto_sync(
            "remote_timestamp_unavailable",
            "远端未返回 Last-Modified，无法安全判断同步方向，已跳过自动同步。",
            Some(remote_url.to_string()),
        ));
    };

    let tolerance = ChronoDuration::seconds(2);
    if remote_modified > local_modified + tolerance {
        let download_result = download_database_from_webdav(app.clone(), normalized.clone())?;
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "远端数据更新，已下载、校验、备份本地数据库并回传同步。".to_string(),
            direction: Some("download_upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: download_result.backup_path,
            active_state_changed: false,
            took_over_active_mode: false,
        });
    }

    if local_modified > remote_modified + tolerance {
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "本地数据更新，已上传到 WebDAV。".to_string(),
            direction: Some("upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: None,
            active_state_changed: false,
            took_over_active_mode: false,
        });
    }

    Ok(WebDavAutoSyncResult {
        status: "skipped".to_string(),
        message: format!(
            "本地与远端时间接近，未执行自动同步。远端大小：{}。",
            remote_metadata
                .size
                .map(format_bytes)
                .unwrap_or_else(|| "未知".to_string())
        ),
        direction: None,
        skipped_reason: Some("up_to_date".to_string()),
        synced_at: Utc::now().to_rfc3339(),
        remote_url: Some(remote_url.to_string()),
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
    })
}

#[tauri::command]
pub fn get_object_storage_settings(app: AppHandle) -> Result<ObjectStorageSettings, String> {
    let connection = open_database(&database_path(&app)?)?;

    Ok(ObjectStorageSettings {
        enabled: get_bool_setting(&connection, OBJECT_STORAGE_ENABLED_KEY, false)?,
        endpoint: get_setting(&connection, OBJECT_STORAGE_ENDPOINT_KEY, "")?,
        bucket: get_setting(&connection, OBJECT_STORAGE_BUCKET_KEY, "")?,
        access_key_id: get_setting(&connection, OBJECT_STORAGE_ACCESS_KEY_ID_KEY, "")?,
        secret_access_key: get_setting(&connection, OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY, "")?,
        region: get_setting(
            &connection,
            OBJECT_STORAGE_REGION_KEY,
            DEFAULT_OBJECT_REGION,
        )?,
        object_key: normalize_object_storage_key(&get_setting(
            &connection,
            OBJECT_STORAGE_OBJECT_KEY_KEY,
            "",
        )?),
    })
}

#[tauri::command]
pub fn save_object_storage_settings(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSettings, String> {
    let normalized = normalize_object_storage_settings(settings)?;
    let connection = open_database(&database_path(&app)?)?;
    persist_object_storage_settings(&connection, &normalized)?;

    Ok(normalized)
}

#[tauri::command]
pub fn test_object_storage_connection(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageStatus, String> {
    let normalized = save_object_storage_settings(app, settings)?;
    let metadata = with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        fetch_object_storage_metadata(&client, &normalized).await
    })?;

    Ok(ObjectStorageStatus {
        configured: true,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        region: normalized.region,
        object_key: normalized.object_key,
        object_exists: metadata.exists,
        object_size: metadata.size,
        last_modified: metadata.last_modified.map(|value| value.to_rfc3339()),
        message: if metadata.exists {
            "对象存储连接成功，远端同步文件可访问。".to_string()
        } else {
            "对象存储连接成功，远端同步文件尚未创建，可先上传本地数据。".to_string()
        },
    })
}

#[tauri::command]
pub fn upload_database_to_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let normalized = save_object_storage_settings(app.clone(), settings)?;
    let database_path = database_path(&app)?;
    let connection = open_database(&database_path)?;
    let device_id = load_or_create_device_id(&connection)?;
    let payload =
        export_shared_sync_payload(&connection, device_id, Utc::now().timestamp_millis())?;
    let bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    let object_url = object_storage_url(&normalized);
    let bytes_len = bytes.len() as u64;

    with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        client
            .put_object()
            .bucket(&normalized.bucket)
            .key(&normalized.object_key)
            .body(ByteStream::from(bytes))
            .content_type("application/json")
            .send()
            .await
            .map_err(|error| format!("上传到对象存储失败：{error}"))?;
        Ok(())
    })?;

    Ok(ObjectStorageSyncResult {
        success: true,
        message: "已成功上传到对象存储。".to_string(),
        object_url,
        bytes: bytes_len,
        backup_path: None,
    })
}

#[tauri::command]
pub fn download_database_from_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let normalized = normalize_object_storage_settings(settings)?;
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;
    let object_url = object_storage_url(&normalized);

    let bytes = with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        let response = client
            .get_object()
            .bucket(&normalized.bucket)
            .key(&normalized.object_key)
            .send()
            .await
            .map_err(|error| format!("从对象存储下载失败：{error}"))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|error| format!("读取对象存储响应失败：{error}"))?
            .into_bytes();
        Ok(bytes.to_vec())
    })?;

    if bytes.is_empty() {
        return Err("对象存储返回的同步包为空。".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_database_path_with_prefix(&app_data_dir, "object-storage");
    let backup_created = local_database_path.exists();
    if backup_created {
        fs::copy(&local_database_path, &backup_path)
            .map_err(|error| format!("创建本地备份失败：{error}"))?;
    }

    let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
        .map_err(|error| format!("解析对象存储同步包失败：{error}"))?;
    let mut connection = open_database(&local_database_path)?;
    import_shared_sync_payload(&mut connection, &payload)?;
    let _ = crate::commands::focus::sync_study_runtime_state(&app);

    Ok(ObjectStorageSyncResult {
        success: true,
        message: "已从对象存储下载并导入共享数据包。".to_string(),
        object_url,
        bytes: bytes.len() as u64,
        backup_path: backup_created.then(|| backup_path.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub async fn auto_sync_object_storage_database(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || auto_sync_object_storage_database_blocking(app))
        .await
        .map_err(|error| format!("对象存储自动同步后台任务失败：{error}"))?
}

pub(crate) fn auto_sync_object_storage_database_blocking(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    let settings = get_object_storage_settings(app.clone())?;
    if !settings.enabled {
        return Ok(skipped_object_storage_auto_sync(
            "object_storage_disabled",
            "对象存储同步已关闭，已跳过自动同步。",
            None,
        ));
    }

    if !object_storage_configured(&settings) {
        return Ok(skipped_object_storage_auto_sync(
            "object_storage_not_configured",
            "未配置对象存储，已跳过自动同步。",
            None,
        ));
    }

    let normalized = normalize_object_storage_settings(settings)?;
    let object_url = object_storage_url(&normalized);
    let local_database_path = database_path(&app)?;

    let metadata = with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        fetch_object_storage_metadata(&client, &normalized).await
    })?;

    if !metadata.exists {
        let upload_result = upload_database_to_object_storage(app, normalized)?;
        return Ok(ObjectStorageAutoSyncResult {
            status: "synced".to_string(),
            message: "对象存储同步文件不存在，已上传本地共享数据包。".to_string(),
            direction: Some("upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            object_url: Some(upload_result.object_url),
            bytes: upload_result.bytes,
            backup_path: None,
            active_state_changed: false,
            took_over_active_mode: false,
        });
    }

    let remote_bytes = with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        let response = client
            .get_object()
            .bucket(&normalized.bucket)
            .key(&normalized.object_key)
            .send()
            .await
            .map_err(|error| format!("下载对象存储同步包失败：{error}"))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|error| format!("读取对象存储同步包失败：{error}"))?
            .into_bytes();
        Ok(bytes.to_vec())
    })?;

    if remote_bytes.is_empty() {
        return Ok(skipped_object_storage_auto_sync(
            "remote_empty",
            "对象存储同步包为空，已跳过自动同步。",
            Some(object_url),
        ));
    }

    let remote_payload: SharedSyncPayload = serde_json::from_slice(&remote_bytes)
        .map_err(|error| format!("解析对象存储同步包失败：{error}"))?;
    let mut connection = open_database(&local_database_path)?;
    let device_id = load_or_create_device_id(&connection)?;
    let exported_at = Utc::now().timestamp_millis();
    let local_payload = export_shared_sync_payload(&connection, device_id.clone(), exported_at)?;
    let local_active_snapshot = shared_active_study_snapshot(&local_payload);
    let merged_payload = merge_shared_sync_payloads(
        local_payload,
        remote_payload,
        device_id.clone(),
        exported_at,
    );
    import_shared_sync_payload(&mut connection, &merged_payload)?;
    let _ = crate::commands::focus::sync_study_runtime_state(&app);

    let refreshed_payload =
        export_shared_sync_payload(&connection, device_id, Utc::now().timestamp_millis())?;
    let refreshed_active_snapshot = shared_active_study_snapshot(&refreshed_payload);
    let active_state_changed = local_active_snapshot != refreshed_active_snapshot;
    let took_over_active_mode = refreshed_active_snapshot.is_some()
        && local_active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync_id.as_str())
            != refreshed_active_snapshot
                .as_ref()
                .map(|snapshot| snapshot.sync_id.as_str());
    let refreshed_bytes =
        serde_json::to_vec(&refreshed_payload).map_err(|error| error.to_string())?;
    let bytes_len = refreshed_bytes.len() as u64;

    with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        client
            .put_object()
            .bucket(&normalized.bucket)
            .key(&normalized.object_key)
            .body(ByteStream::from(refreshed_bytes))
            .content_type("application/json")
            .send()
            .await
            .map_err(|error| format!("上传对象存储同步包失败：{error}"))?;
        Ok(())
    })?;

    let result = ObjectStorageAutoSyncResult {
        status: "synced".to_string(),
        message: "对象存储共享数据包已合并并回传。".to_string(),
        direction: Some("download_upload".to_string()),
        skipped_reason: None,
        synced_at: Utc::now().to_rfc3339(),
        object_url: Some(object_url),
        bytes: bytes_len,
        backup_path: None,
        active_state_changed,
        took_over_active_mode,
    };
    emit_study_sync_state_changed(&app, &result);
    Ok(result)
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

    if settings.enabled {
        if url.is_empty() {
            return Err("请输入 WebDAV 地址。".to_string());
        }

        Url::parse(&url)
            .map_err(|_| "WebDAV 地址格式不正确，请填写 http 或 https 地址。".to_string())?;

        if remote_path.is_empty() {
            return Err("请输入远端文件路径。".to_string());
        }
    } else if !url.is_empty() {
        Url::parse(&url)
            .map_err(|_| "WebDAV 地址格式不正确，请填写 http 或 https 地址。".to_string())?;
    }

    Ok(WebDavSettings {
        enabled: settings.enabled,
        url,
        username,
        password,
        remote_path: if remote_path.is_empty() {
            DEFAULT_REMOTE_PATH.to_string()
        } else {
            remote_path
        },
    })
}

fn normalize_object_storage_settings(
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSettings, String> {
    let endpoint = settings.endpoint.trim().trim_end_matches('/').to_string();
    let bucket = settings.bucket.trim().to_string();
    let access_key_id = settings.access_key_id.trim().to_string();
    let secret_access_key = settings.secret_access_key;
    let region = settings.region.trim().to_string();
    let object_key = normalize_object_storage_key(&settings.object_key);

    if settings.enabled {
        if endpoint.is_empty() {
            return Err("请输入对象存储 Endpoint。".to_string());
        }

        Url::parse(&endpoint)
            .map_err(|_| "对象存储 Endpoint 格式不正确，请填写 http 或 https 地址。".to_string())?;

        if bucket.is_empty() {
            return Err("请输入对象存储 Bucket。".to_string());
        }

        if access_key_id.is_empty() {
            return Err("请输入 Access Key ID。".to_string());
        }

        if secret_access_key.trim().is_empty() {
            return Err("请输入 Secret Access Key。".to_string());
        }

        if object_key.contains("..") {
            return Err("对象 Key 格式不正确，请填写类似 study-sync.json 的路径。".to_string());
        }
    } else if !endpoint.is_empty() {
        Url::parse(&endpoint)
            .map_err(|_| "对象存储 Endpoint 格式不正确，请填写 http 或 https 地址。".to_string())?;
    }

    Ok(ObjectStorageSettings {
        enabled: settings.enabled,
        endpoint,
        bucket,
        access_key_id,
        secret_access_key,
        region: if region.is_empty() {
            DEFAULT_OBJECT_REGION.to_string()
        } else {
            region
        },
        object_key,
    })
}

fn normalize_object_storage_key(raw_key: &str) -> String {
    let object_key = raw_key.trim().trim_start_matches('/').replace('\\', "/");

    if object_key.is_empty() || object_key == DEFAULT_REMOTE_PATH {
        DEFAULT_OBJECT_KEY.to_string()
    } else {
        object_key
    }
}

fn object_storage_configured(settings: &ObjectStorageSettings) -> bool {
    !settings.endpoint.trim().is_empty()
        && !settings.bucket.trim().is_empty()
        && !settings.access_key_id.trim().is_empty()
        && !settings.secret_access_key.trim().is_empty()
        && !settings.object_key.trim().is_empty()
}

fn with_s3_runtime<T>(
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(future)
}

async fn object_storage_client(settings: &ObjectStorageSettings) -> Result<S3Client, String> {
    let credentials = Credentials::new(
        settings.access_key_id.clone(),
        settings.secret_access_key.clone(),
        None,
        None,
        "kaoyan-focus-object-storage",
    );
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .region(Region::new(settings.region.clone()))
        .load()
        .await;
    let config = S3ConfigBuilder::from(&shared_config)
        .endpoint_url(settings.endpoint.clone())
        .force_path_style(true)
        .build();

    Ok(S3Client::from_conf(config))
}

async fn fetch_object_storage_metadata(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<RemoteFileMetadata, String> {
    match client
        .head_object()
        .bucket(&settings.bucket)
        .key(&settings.object_key)
        .send()
        .await
    {
        Ok(output) => Ok(RemoteFileMetadata {
            exists: true,
            size: output
                .content_length()
                .and_then(|value| u64::try_from(value).ok()),
            last_modified: output.last_modified().and_then(|value| {
                DateTime::<Utc>::from_timestamp(value.secs(), value.subsec_nanos())
            }),
        }),
        Err(error) => {
            let message = error.to_string();
            if message.contains("NotFound")
                || message.contains("404")
                || message.contains("NoSuchKey")
            {
                Ok(RemoteFileMetadata {
                    exists: false,
                    size: None,
                    last_modified: None,
                })
            } else {
                Err(format!("读取对象存储远端状态失败：{error}"))
            }
        }
    }
}

fn object_storage_url(settings: &ObjectStorageSettings) -> String {
    format!(
        "{}/{}/{}",
        settings.endpoint.trim_end_matches('/'),
        settings.bucket,
        settings.object_key.trim_start_matches('/')
    )
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
            .map_err(|_| "WebDAV 鍦板潃涓嶆敮鎸佽矾寰勬嫾鎺ャ€?.".to_string())?;
        for segment in settings
            .remote_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            segments.push(segment);
        }
    }
    Ok(url)
}

fn ensure_remote_directories(client: &Client, settings: &WebDavSettings) -> Result<(), String> {
    let mut current = Url::parse(&format!("{}/", settings.url.trim_end_matches('/')))
        .map_err(|error| error.to_string())?;
    let parts: Vec<&str> = settings
        .remote_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if parts.len() <= 1 {
        return Ok(());
    }

    for part in &parts[..parts.len() - 1] {
        {
            let mut segments = current
                .path_segments_mut()
                .map_err(|_| "WebDAV 鍦板潃涓嶆敮鎸佺洰褰曞垱寤恒€?.".to_string())?;
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

        return Err(format!(
            "创建远端目录失败，WebDAV 返回状态：{}",
            response.status().as_u16()
        ));
    }

    Ok(())
}

fn validate_sqlite_database(path: &Path) -> Result<(), String> {
    let connection =
        Connection::open(path).map_err(|error| format!("文件不是有效的 SQLite 数据库：{error}"))?;
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("校验数据库失败：{error}"))
        .and_then(|result| {
            if result == "ok" {
                Ok(())
            } else {
                Err(format!("数据库完整性检查失败：{result}"))
            }
        })
}

fn ensure_no_active_runtime(path: &Path) -> Result<(), String> {
    if has_active_runtime(path)? {
        return Err("当前有进行中的学习模式，请先完成本次学习后再恢复数据。".to_string());
    }

    Ok(())
}

fn has_active_runtime(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
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

    Ok(active_study_modes > 0 || running_sessions > 0)
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn backup_database_path(app_data_dir: &Path) -> PathBuf {
    backup_database_path_with_prefix(app_data_dir, "webdav")
}

fn backup_database_path_with_prefix(app_data_dir: &Path, prefix: &str) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    app_data_dir.join(format!("kaoyan-focus.before-{prefix}-{stamp}.sqlite3"))
}

fn get_setting(connection: &Connection, key: &str, fallback: &str) -> Result<String, String> {
    Ok(connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn get_bool_setting(connection: &Connection, key: &str, fallback: bool) -> Result<bool, String> {
    let raw = get_setting(connection, key, if fallback { "true" } else { "false" })?;
    Ok(matches!(raw.as_str(), "true" | "1" | "yes" | "on"))
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
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

fn persist_webdav_settings(
    connection: &Connection,
    settings: &WebDavSettings,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(
        connection,
        WEBDAV_ENABLED_KEY,
        &settings.enabled.to_string(),
        &now,
    )?;
    set_setting(connection, WEBDAV_URL_KEY, &settings.url, &now)?;
    set_setting(connection, WEBDAV_USERNAME_KEY, &settings.username, &now)?;
    set_setting(connection, WEBDAV_PASSWORD_KEY, &settings.password, &now)?;
    set_setting(
        connection,
        WEBDAV_REMOTE_PATH_KEY,
        &settings.remote_path,
        &now,
    )?;
    Ok(())
}

fn persist_object_storage_settings(
    connection: &Connection,
    settings: &ObjectStorageSettings,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(
        connection,
        OBJECT_STORAGE_ENABLED_KEY,
        &settings.enabled.to_string(),
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_ENDPOINT_KEY,
        &settings.endpoint,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_BUCKET_KEY,
        &settings.bucket,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_ACCESS_KEY_ID_KEY,
        &settings.access_key_id,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY,
        &settings.secret_access_key,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_REGION_KEY,
        &settings.region,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_OBJECT_KEY_KEY,
        &settings.object_key,
        &now,
    )?;
    Ok(())
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn fetch_remote_file_metadata(
    client: &Client,
    remote_url: &Url,
    settings: &WebDavSettings,
) -> Result<RemoteFileMetadata, String> {
    let response = webdav_request(client, Method::HEAD, remote_url.clone(), settings)
        .send()
        .map_err(|error| format!("读取 WebDAV 远端状态失败：{error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(RemoteFileMetadata {
            exists: false,
            size: None,
            last_modified: None,
        });
    }

    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        return Ok(RemoteFileMetadata {
            exists: true,
            size: None,
            last_modified: None,
        });
    }

    if !response.status().is_success() {
        return Err(format!(
            "读取 WebDAV 远端状态失败，返回状态：{}",
            response.status().as_u16()
        ));
    }

    let headers = response.headers().clone();
    Ok(RemoteFileMetadata {
        exists: true,
        size: content_length(&headers),
        last_modified: parse_last_modified(&headers),
    })
}

fn parse_last_modified(headers: &HeaderMap) -> Option<DateTime<Utc>> {
    headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn local_database_modified_at(path: &Path) -> Result<DateTime<Utc>, String> {
    let modified = fs::metadata(path)
        .map_err(|error| format!("读取本地数据库时间失败：{error}"))?
        .modified()
        .map_err(|error| format!("读取本地数据库修改时间失败：{error}"))?;
    Ok(modified.into())
}

fn skipped_auto_sync(
    reason: &str,
    message: &str,
    remote_url: Option<String>,
) -> WebDavAutoSyncResult {
    WebDavAutoSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        direction: None,
        skipped_reason: Some(reason.to_string()),
        synced_at: Utc::now().to_rfc3339(),
        remote_url,
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
    }
}

fn skipped_object_storage_auto_sync(
    reason: &str,
    message: &str,
    object_url: Option<String>,
) -> ObjectStorageAutoSyncResult {
    ObjectStorageAutoSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        direction: None,
        skipped_reason: Some(reason.to_string()),
        synced_at: Utc::now().to_rfc3339(),
        object_url,
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
    }
}

fn emit_study_sync_state_changed(app: &AppHandle, result: &ObjectStorageAutoSyncResult) {
    if !result.active_state_changed {
        return;
    }

    let _ = app.emit(STUDY_SYNC_STATE_CHANGED_EVENT, result);
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }

    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value: &HeaderValue| value.to_str().ok())
        .map(ToString::to_string)
}
