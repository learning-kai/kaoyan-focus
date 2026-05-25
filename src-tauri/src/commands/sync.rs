use std::{
    collections::{HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, TryLockError},
    time::Duration,
};

use crate::{
    storage::db::open_database,
    sync_package::{
        count_payload_deleted_entities, count_payload_entities, export_shared_sync_payload,
        import_shared_sync_payload, load_or_create_device_id, merge_remote_payload_into_local,
        merge_shared_sync_payloads, shared_active_study_snapshot, SharedActiveStudySnapshot,
        SharedAppEvent, SharedChecklistTask, SharedDailyReview, SharedFocusSession,
        SharedScheduleBlock, SharedScheduleTemplate, SharedStudyMode, SharedSubject,
        SharedSyncPayload, SharedTodayPlanItem, SharedWeeklyReview,
    },
};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    config::{timeout::TimeoutConfig, Builder as S3ConfigBuilder, Region},
    error::{ProvideErrorMetadata, SdkError},
    primitives::ByteStream,
    Client as S3Client,
};
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, NaiveDate, Utc};
use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED},
    Method, StatusCode, Url,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

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
const R2_V3_SCHEMA_VERSION: i64 = 3;
const R2_V3_DEFAULT_PREFIX: &str = "ultrafocus-sync/v3";
const R2_V3_MANIFEST: &str = "manifest.json";
const R2_V3_ACTIVE_LOCK_TTL_MILLIS: i64 = 3 * 60 * 1000;
const R2_V3_SNAPSHOT_OP_THRESHOLD: usize = 500;
const R2_V3_SNAPSHOT_BYTES_THRESHOLD: usize = 2 * 1024 * 1024;
const R2_V3_HLC_LOGICAL_MODULUS: i64 = 100_000;
const R2_OPERATION_TIMEOUT_SECONDS: u64 = 45;
const R2_OPERATION_ATTEMPT_TIMEOUT_SECONDS: u64 = 20;
const R2_CONNECT_TIMEOUT_SECONDS: u64 = 8;
const R2_READ_TIMEOUT_SECONDS: u64 = 20;
const R2_OP_UPLOAD_CONCURRENCY: usize = 16;
const R2_OP_DOWNLOAD_CONCURRENCY: usize = 32;
const R2_LEGACY_BACKUP_PREFIX: &str = "backups/study-sync/";
const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const STUDY_SYNC_STATE_CHANGED_EVENT: &str = "study-sync-state-changed";
static OBJECT_STORAGE_SYNC_LOCK: Mutex<()> = Mutex::new(());

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

#[derive(Debug, Clone, Serialize)]
pub struct SyncRunSummary {
    pub id: i64,
    pub sync_id: String,
    pub backend: String,
    pub trigger: String,
    pub direction: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub device_id: Option<String>,
    pub remote_device_id: Option<String>,
    pub remote_exported_at: Option<i64>,
    pub local_exported_at: Option<i64>,
    pub bytes: i64,
    pub imported_count: i64,
    pub exported_count: i64,
    pub deleted_count: i64,
    pub conflict_count: i64,
    pub active_state_changed: bool,
    pub took_over_active_mode: bool,
    pub validation_report: Option<String>,
    pub backup_path: Option<String>,
    pub remote_backup_key: Option<String>,
    pub active_snapshot_sync_id: Option<String>,
    pub remote_active_snapshot_sync_id: Option<String>,
    pub active_snapshot_phase: Option<String>,
    pub remote_active_snapshot_phase: Option<String>,
    pub active_snapshot_updated_at: Option<i64>,
    pub remote_snapshot_updated_at: Option<i64>,
    pub remote_exported_drift_seconds: Option<i64>,
    pub detail: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncBackupEntry {
    pub source: String,
    pub key: String,
    pub label: String,
    pub created_at: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncBackupPreview {
    pub source: String,
    pub key: String,
    pub bytes: u64,
    pub validation_report: String,
    pub entity_count: i64,
    pub deleted_count: i64,
    pub exported_at: Option<i64>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteFileMetadata {
    exists: bool,
    size: Option<u64>,
    last_modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct ObjectStorageBackupObject {
    key: String,
    size: Option<u64>,
    last_modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct R2V3Manifest {
    schema_version: i64,
    current_snapshot_key: Option<String>,
    watermarks: HashMap<String, i64>,
    compacted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct R2V3Snapshot {
    schema_version: i64,
    snapshot_id: String,
    created_hlc: String,
    watermarks: HashMap<String, i64>,
    payload: SharedSyncPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct R2V3Operation {
    schema_version: i64,
    op_id: String,
    device_id: String,
    seq: i64,
    hlc: String,
    base_hlc: Option<String>,
    entity_type: String,
    sync_id: String,
    action: String,
    payload: Option<Value>,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct R2V3ActiveLock {
    schema_version: i64,
    device_id: String,
    sync_id: String,
    state_revision: i64,
    hlc: String,
    claimed_at: i64,
    expires_at: i64,
}

#[derive(Debug, Clone)]
struct R2V3RemoteState {
    manifest: Option<R2V3Manifest>,
    manifest_etag: Option<String>,
    payload: SharedSyncPayload,
    watermarks: HashMap<String, i64>,
    operation_count: usize,
    bytes: usize,
    migrated_legacy: bool,
    applied_operations: Vec<R2V3Operation>,
}

#[derive(Debug, Clone)]
struct LocalEntityVersion {
    hlc: String,
    deleted_at: Option<i64>,
    updated_at: i64,
}

#[derive(Debug, Clone)]
struct SyncRunRecord {
    sync_id: String,
    backend: String,
    trigger: String,
    direction: Option<String>,
    status: String,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    device_id: Option<String>,
    remote_device_id: Option<String>,
    remote_exported_at: Option<i64>,
    local_exported_at: Option<i64>,
    bytes: i64,
    imported_count: i64,
    exported_count: i64,
    deleted_count: i64,
    conflict_count: i64,
    active_state_changed: bool,
    took_over_active_mode: bool,
    validation_report: Option<String>,
    backup_path: Option<String>,
    remote_backup_key: Option<String>,
    active_snapshot_sync_id: Option<String>,
    remote_active_snapshot_sync_id: Option<String>,
    active_snapshot_phase: Option<String>,
    remote_active_snapshot_phase: Option<String>,
    active_snapshot_updated_at: Option<i64>,
    remote_snapshot_updated_at: Option<i64>,
    remote_exported_drift_seconds: Option<i64>,
    detail: Option<String>,
    error_message: Option<String>,
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
    .map_err(|error| format!("Connect to WebDAV failed: {error}"))?;

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
            message: "WebDAV connection succeeded; remote sync file is accessible.".to_string(),
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
            message: "WebDAV connection succeeded; remote sync file does not exist yet."
                .to_string(),
        });
    }

    Err(format!(
        "WebDAV connection failed with status: {}",
        status.as_u16()
    ))
}

#[tauri::command]
pub fn upload_database_to_webdav(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSyncResult, String> {
    let normalized = save_webdav_settings(app.clone(), settings)?;
    let database_path = database_path(&app)?;
    let bytes =
        fs::read(&database_path).map_err(|error| format!("Read local database failed: {error}"))?;

    if bytes.is_empty() {
        return Err("Local database is empty; cannot upload.".to_string());
    }

    let client = webdav_client()?;
    ensure_remote_directories(&client, &normalized)?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::PUT, remote_url.clone(), &normalized)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|error| format!("Upload to WebDAV failed: {error}"))?;

    if response.status().is_success()
        || response.status() == StatusCode::CREATED
        || response.status() == StatusCode::NO_CONTENT
    {
        return Ok(WebDavSyncResult {
            success: true,
            message: "Uploaded to WebDAV successfully.".to_string(),
            remote_url: remote_url.to_string(),
            bytes: fs::metadata(&database_path)
                .map(|meta| meta.len())
                .unwrap_or(0),
            backup_path: None,
        });
    }

    Err(format!(
        "Upload to WebDAV failed with status: {}",
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
        .map_err(|error| format!("Download from WebDAV failed: {error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Err("Remote WebDAV sync file does not exist.".to_string());
    }

    if !response.status().is_success() {
        return Err(format!(
            "Download from WebDAV failed with status: {}",
            response.status().as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("Read WebDAV response failed: {error}"))?;

    if bytes.is_empty() {
        return Err("WebDAV returned an empty file.".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let temp_path = app_data_dir.join("kaoyan-focus.webdav-download.tmp");
    fs::write(&temp_path, &bytes)
        .map_err(|error| format!("Write temporary file failed: {error}"))?;
    validate_sqlite_database(&temp_path)?;

    let backup_path = create_local_sync_backup(&app, &local_database_path, "webdav")?;

    fs::rename(&temp_path, &local_database_path)
        .or_else(|_| {
            fs::copy(&temp_path, &local_database_path)?;
            fs::remove_file(&temp_path)
        })
        .map_err(|error| format!("Replace local database failed: {error}"))?;

    let connection = open_database(&local_database_path)?;
    persist_webdav_settings(&connection, &normalized)?;

    Ok(WebDavSyncResult {
        success: true,
        message: "Downloaded and validated the database from WebDAV.".to_string(),
        remote_url: remote_url.to_string(),
        bytes: bytes.len() as u64,
        backup_path,
    })
}

#[tauri::command]
pub fn auto_sync_webdav_database(app: AppHandle) -> Result<WebDavAutoSyncResult, String> {
    let settings = get_webdav_settings(app.clone())?;
    if !settings.enabled {
        return Ok(skipped_auto_sync(
            "webdav_disabled",
            "WebDAV sync is disabled; automatic sync skipped.",
            None,
        ));
    }

    if settings.url.trim().is_empty() {
        return Ok(skipped_auto_sync(
            "webdav_not_configured",
            "WebDAV is not configured; automatic sync skipped.",
            None,
        ));
    }

    let normalized = normalize_settings(settings)?;
    let local_database_path = database_path(&app)?;
    if has_active_runtime(&local_database_path)? {
        return Ok(skipped_auto_sync(
            "study_mode_active",
            "Study mode is running; WebDAV automatic sync skipped.",
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
            message: "Remote sync file did not exist; uploaded local database.".to_string(),
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
            "Remote did not return Last-Modified; automatic sync skipped.",
            Some(remote_url.to_string()),
        ));
    };

    let tolerance = ChronoDuration::seconds(2);
    if remote_modified > local_modified + tolerance {
        let download_result = download_database_from_webdav(app.clone(), normalized.clone())?;
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "Remote data was newer; downloaded, validated, backed up, and uploaded merged database.".to_string(),
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
            message: "Local data was newer; uploaded to WebDAV.".to_string(),
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
            "Local and remote timestamps are close; automatic sync skipped. Remote size: {}",
            remote_metadata
                .size
                .map(format_bytes)
                .unwrap_or_else(|| "unknown".to_string())
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
            "R2 connection succeeded; remote sync data is accessible.".to_string()
        } else {
            "R2 connection succeeded; remote sync data does not exist yet.".to_string()
        },
    })
}

#[tauri::command]
pub fn list_sync_runs(app: AppHandle, limit: Option<i64>) -> Result<Vec<SyncRunSummary>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let limit = limit.unwrap_or(10).clamp(1, 100);
    let mut statement = connection
        .prepare(
            "
            SELECT id, sync_id, backend, trigger, direction, status, started_at, finished_at,
                   duration_ms, device_id, remote_device_id, remote_exported_at, local_exported_at,
                   bytes, imported_count, exported_count, deleted_count, conflict_count,
                   active_state_changed, took_over_active_mode, validation_report, backup_path,
                   remote_backup_key, active_snapshot_sync_id, remote_active_snapshot_sync_id,
                   active_snapshot_phase, remote_active_snapshot_phase, active_snapshot_updated_at,
                   remote_snapshot_updated_at, remote_exported_drift_seconds, detail, error_message
            FROM sync_runs
            ORDER BY finished_at DESC, id DESC
            LIMIT ?1
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([limit], row_to_sync_run_summary)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_sync_backups(app: AppHandle) -> Result<Vec<SyncBackupEntry>, String> {
    let mut entries = Vec::new();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    if app_data_dir.exists() {
        for entry in fs::read_dir(&app_data_dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("kaoyan-focus.before-") || !name.ends_with(".sqlite3") {
                continue;
            }
            let metadata = entry.metadata().ok();
            entries.push(SyncBackupEntry {
                source: "local".to_string(),
                key: path.to_string_lossy().to_string(),
                label: name.to_string(),
                created_at: metadata
                    .as_ref()
                    .and_then(|value| value.modified().ok())
                    .map(DateTime::<Utc>::from)
                    .map(|value| value.to_rfc3339()),
                bytes: metadata.map(|value| value.len()),
            });
        }
    }

    if let Ok(settings) = get_object_storage_settings(app.clone()) {
        if settings.enabled && object_storage_configured(&settings) {
            let normalized = normalize_object_storage_settings(settings)?;
            if let Ok(remote) = list_object_storage_backup_entries(&normalized) {
                entries.extend(remote);
            }
        }
    }

    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(entries)
}

#[tauri::command]
pub fn preview_sync_backup(
    app: AppHandle,
    source: String,
    key: String,
) -> Result<SyncBackupPreview, String> {
    let bytes = load_backup_bytes(&app, &source, &key)?;
    let validation_report;
    let entity_count;
    let deleted_count;
    let exported_at;
    let device_id;
    if source == "local" {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        let temp_path = app_data_dir.join("kaoyan-focus.backup-preview.tmp");
        fs::write(&temp_path, &bytes).map_err(|error| error.to_string())?;
        validation_report = validate_sqlite_database(&temp_path)
            .map(|_| "SQLite integrity_check ok".to_string())
            .unwrap_or_else(|error| error);
        let _ = fs::remove_file(&temp_path);
        entity_count = 0;
        deleted_count = 0;
        exported_at = None;
        device_id = None;
    } else {
        let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Parse sync backup failed: {error}"))?;
        validation_report = validate_sync_payload(&payload, Some(Utc::now().timestamp_millis()));
        entity_count = count_payload_entities(&payload);
        deleted_count = count_payload_deleted_entities(&payload);
        exported_at = Some(payload.exported_at);
        device_id = Some(payload.device_id);
    }

    Ok(SyncBackupPreview {
        source,
        key,
        bytes: bytes.len() as u64,
        validation_report,
        entity_count,
        deleted_count,
        exported_at,
        device_id,
    })
}

#[tauri::command]
pub fn restore_sync_backup(app: AppHandle, source: String, key: String) -> Result<String, String> {
    let bytes = load_backup_bytes(&app, &source, &key)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;

    if source == "local" {
        let temp_path = app_data_dir.join("kaoyan-focus.restore.tmp");
        fs::write(&temp_path, &bytes).map_err(|error| error.to_string())?;
        validate_sqlite_database(&temp_path)?;
        let _ = create_local_sync_backup(&app, &local_database_path, "restore-current")?;
        fs::rename(&temp_path, &local_database_path)
            .or_else(|_| {
                fs::copy(&temp_path, &local_database_path)?;
                fs::remove_file(&temp_path)
            })
            .map_err(|error| format!("Replace local database failed: {error}"))?;
        return Ok("Restored the database from local backup.".to_string());
    }

    let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parse sync backup failed: {error}"))?;
    let _ = create_local_sync_backup(&app, &local_database_path, "restore-current")?;
    let mut connection = open_database(&local_database_path)?;
    import_shared_sync_payload(&mut connection, &payload)?;
    let _ = crate::commands::focus::sync_study_runtime_state(&app);
    Ok("Imported shared sync data from the R2/S3 backup.".to_string())
}

#[tauri::command]
pub fn upload_database_to_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let normalized = save_object_storage_settings(app.clone(), settings)?;
    let auto = sync_r2_v3_object_storage(app, "manual_upload", false)?;
    Ok(ObjectStorageSyncResult {
        success: auto.status == "synced",
        message: auto.message,
        object_url: auto.object_url.unwrap_or_else(|| {
            format!(
                "{}/{}",
                normalized.endpoint.trim_end_matches('/'),
                r2_v3_prefix(&normalized)
            )
        }),
        bytes: auto.bytes,
        backup_path: auto.backup_path,
    })
}
#[tauri::command]
pub fn download_database_from_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let normalized = save_object_storage_settings(app.clone(), settings)?;
    let auto = sync_r2_v3_object_storage(app, "manual_download", true)?;
    Ok(ObjectStorageSyncResult {
        success: auto.status == "synced",
        message: auto.message,
        object_url: auto.object_url.unwrap_or_else(|| {
            format!(
                "{}/{}",
                normalized.endpoint.trim_end_matches('/'),
                r2_v3_prefix(&normalized)
            )
        }),
        bytes: auto.bytes,
        backup_path: auto.backup_path,
    })
}
#[tauri::command]
pub async fn auto_sync_object_storage_database(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_object_storage_sync_lock(|| auto_sync_object_storage_database_pull_only(app))
    })
    .await
    .map_err(|error| format!("object storage auto sync background task failed: {error}"))?
}

#[tauri::command]
pub async fn sync_object_storage_state_change(
    app: AppHandle,
    trigger: Option<String>,
) -> Result<ObjectStorageAutoSyncResult, String> {
    let trigger = trigger.unwrap_or_else(|| "state_change".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        with_object_storage_sync_lock(|| {
            auto_sync_object_storage_database_blocking_with_trigger(app, &trigger)
        })
    })
    .await
    .map_err(|error| format!("object storage state sync background task failed: {error}"))?
}

fn sync_r2_v3_object_storage(
    app: AppHandle,
    trigger: &str,
    pull_only: bool,
) -> Result<ObjectStorageAutoSyncResult, String> {
    let started_at = Utc::now();
    let sync_id = Uuid::new_v4().to_string();
    eprintln!(
        "R2 v3 sync start id={} trigger={} pull_only={}",
        sync_id, trigger, pull_only
    );
    let settings = get_object_storage_settings(app.clone())?;
    if !settings.enabled {
        let result = skipped_object_storage_auto_sync(
            "object_storage_disabled",
            "object storage sync disabled",
            None,
        );
        record_object_sync_result(
            &app, &sync_id, trigger, started_at, &result, None, None, None, None, None,
        );
        eprintln!(
            "R2 v3 sync finish id={} status={} reason={:?}",
            sync_id, result.status, result.skipped_reason
        );
        return Ok(result);
    }
    if !object_storage_configured(&settings) {
        let result = skipped_object_storage_auto_sync(
            "object_storage_not_configured",
            "object storage not configured",
            None,
        );
        record_object_sync_result(
            &app, &sync_id, trigger, started_at, &result, None, None, None, None, None,
        );
        eprintln!(
            "R2 v3 sync finish id={} status={} reason={:?}",
            sync_id, result.status, result.skipped_reason
        );
        return Ok(result);
    }

    let normalized = normalize_object_storage_settings(settings)?;
    let object_url = format!(
        "{}/{}",
        normalized.endpoint.trim_end_matches('/'),
        r2_v3_prefix(&normalized)
    );
    let local_database_path = database_path(&app)?;

    for attempt in 0..3 {
        eprintln!(
            "R2 v3 sync stage id={} attempt={} open_db",
            sync_id,
            attempt + 1
        );
        let mut connection = open_database(&local_database_path)?;
        let device_id = load_or_create_device_id(&connection)?;
        eprintln!(
            "R2 v3 sync stage id={} attempt={} load_versions",
            sync_id,
            attempt + 1
        );
        let local_entity_versions = load_local_entity_versions(&connection)?;
        let applied_operation_ids = load_applied_operation_ids(&connection)?;
        let exported_at = Utc::now().timestamp_millis();
        eprintln!(
            "R2 v3 sync stage id={} attempt={} export_payload",
            sync_id,
            attempt + 1
        );
        let local_payload =
            export_shared_sync_payload(&connection, device_id.clone(), exported_at)?;
        let local_active_snapshot = shared_active_study_snapshot(&local_payload);
        let local_pending_operations = if pull_only {
            Vec::new()
        } else {
            payload_entity_operations(&local_payload, &device_id, &local_entity_versions)?
        };
        eprintln!(
            "R2 v3 sync stage id={} attempt={} exported pending_ops={}",
            sync_id,
            attempt + 1,
            local_pending_operations.len()
        );

        let remote_state = with_s3_runtime(async {
            eprintln!(
                "R2 v3 sync stage id={} attempt={} create_client",
                sync_id,
                attempt + 1
            );
            let client = object_storage_client(&normalized).await?;
            if let Some(active_snapshot) = local_active_snapshot.as_ref() {
                if !pull_only {
                    eprintln!(
                        "R2 v3 sync stage id={} attempt={} acquire_active_lock",
                        sync_id,
                        attempt + 1
                    );
                    if !try_acquire_r2_v3_active_lock(
                        &client,
                        &normalized,
                        &device_id,
                        active_snapshot,
                    )
                    .await?
                    {
                        eprintln!(
                            "R2 v3 sync active lock conflict ignored before merge sync_id={}",
                            active_snapshot.sync_id
                        );
                    }
                }
            } else {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} release_active_lock",
                    sync_id,
                    attempt + 1
                );
                release_r2_v3_active_lock_if_owned(&client, &normalized, &device_id).await?;
            }
            if !pull_only {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} upload_pending_ops",
                    sync_id,
                    attempt + 1
                );
                upload_payload_operations(&client, &normalized, &local_pending_operations).await?;
            }
            eprintln!(
                "R2 v3 sync stage id={} attempt={} load_remote_state",
                sync_id,
                attempt + 1
            );
            load_r2_v3_remote_state(
                &client,
                &normalized,
                &device_id,
                &applied_operation_ids,
                &local_entity_versions,
            )
            .await
        });
        let remote_state = match remote_state {
            Ok(remote_state) => remote_state,
            Err(message) if message == "r2_active_lock_conflict" => {
                let result = skipped_object_storage_auto_sync(
                    "r2_active_lock_conflict",
                    "Another device already holds the active focus lock.",
                    Some(object_url),
                );
                record_object_sync_result(
                    &app,
                    &sync_id,
                    trigger,
                    started_at,
                    &result,
                    Some(&local_payload),
                    None,
                    local_active_snapshot.as_ref(),
                    None,
                    None,
                );
                return Ok(result);
            }
            Err(message) => return Err(message),
        };
        eprintln!(
            "R2 v3 sync stage id={} attempt={} remote_loaded ops={} bytes={}",
            sync_id,
            attempt + 1,
            remote_state.operation_count,
            remote_state.bytes
        );

        let remote_report = format!(
            "{}; r2_v3 manifest={} ops={} migratedLegacy={}",
            validate_sync_payload(&remote_state.payload, Some(Utc::now().timestamp_millis())),
            remote_state.manifest.is_some(),
            remote_state.operation_count,
            remote_state.migrated_legacy
        );
        let merged_payload = if pull_only {
            merge_remote_payload_into_local(
                local_payload,
                remote_state.payload.clone(),
                device_id.clone(),
                exported_at,
            )
        } else {
            merge_shared_sync_payloads(
                local_payload,
                remote_state.payload.clone(),
                device_id.clone(),
                exported_at,
            )
        };

        let backup_path = create_local_sync_backup(
            &app,
            &local_database_path,
            if pull_only {
                "r2-v3-pull"
            } else {
                "r2-v3-auto"
            },
        )?;
        eprintln!(
            "R2 v3 sync stage id={} attempt={} import_payload",
            sync_id,
            attempt + 1
        );
        import_shared_sync_payload(&mut connection, &merged_payload)?;
        let _ = crate::commands::focus::sync_study_runtime_state(&app);

        eprintln!(
            "R2 v3 sync stage id={} attempt={} refresh_payload",
            sync_id,
            attempt + 1
        );
        let refreshed_payload = export_shared_sync_payload(
            &connection,
            device_id.clone(),
            Utc::now().timestamp_millis(),
        )?;
        let refreshed_active_snapshot = shared_active_study_snapshot(&refreshed_payload);
        let active_state_changed = local_active_snapshot != refreshed_active_snapshot;
        let took_over_active_mode = refreshed_active_snapshot.is_some()
            && local_active_snapshot
                .as_ref()
                .map(|snapshot| snapshot.sync_id.as_str())
                != refreshed_active_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.sync_id.as_str());

        if pull_only {
            persist_applied_operations(&connection, &remote_state.applied_operations)?;
            persist_payload_entity_versions(&connection, &remote_state.payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 pull-only sync completed".to_string(),
                direction: Some("download".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} pull_only=true",
                sync_id, result.status
            );
            return Ok(result);
        }

        let write_success = with_s3_runtime(async {
            eprintln!(
                "R2 v3 sync stage id={} attempt={} write_create_client",
                sync_id,
                attempt + 1
            );
            let client = object_storage_client(&normalized).await?;
            let version_seed = apply_operations_to_version_map(
                apply_operations_to_version_map(
                    payload_entity_version_map(&remote_state.payload, &device_id)?,
                    &remote_state.applied_operations,
                ),
                &local_pending_operations,
            );
            let refreshed_ops =
                payload_entity_operations(&refreshed_payload, &device_id, &version_seed)?;
            eprintln!(
                "R2 v3 sync stage id={} attempt={} upload_refreshed_ops={}",
                sync_id,
                attempt + 1,
                refreshed_ops.len()
            );
            upload_payload_operations(&client, &normalized, &refreshed_ops).await?;
            if local_pending_operations.is_empty() && refreshed_ops.is_empty() {
                return Ok::<(bool, Vec<R2V3Operation>), String>((true, refreshed_ops));
            }
            let mut watermarks = remote_state.watermarks.clone();
            for operation in local_pending_operations.iter().chain(refreshed_ops.iter()) {
                watermarks
                    .entry(operation.device_id.clone())
                    .and_modify(|value| *value = (*value).max(operation.seq))
                    .or_insert(operation.seq);
            }
            let should_compact = remote_state.manifest.is_none()
                || remote_state.migrated_legacy
                || remote_state.operation_count >= R2_V3_SNAPSHOT_OP_THRESHOLD
                || remote_state.bytes >= R2_V3_SNAPSHOT_BYTES_THRESHOLD
                || remote_state
                    .manifest
                    .as_ref()
                    .and_then(|manifest| manifest.current_snapshot_key.as_ref())
                    .is_none();
            let success = if should_compact {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} write_snapshot_manifest",
                    sync_id,
                    attempt + 1
                );
                write_r2_v3_snapshot_and_manifest(
                    &client,
                    &normalized,
                    refreshed_payload.clone(),
                    watermarks,
                    remote_state.manifest_etag.as_deref(),
                    &device_id,
                )
                .await?
            } else {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} write_manifest",
                    sync_id,
                    attempt + 1
                );
                write_r2_v3_manifest(
                    &client,
                    &normalized,
                    remote_state
                        .manifest
                        .as_ref()
                        .and_then(|manifest| manifest.current_snapshot_key.clone()),
                    watermarks,
                    remote_state.manifest_etag.as_deref(),
                )
                .await?
            };
            Ok::<(bool, Vec<R2V3Operation>), String>((success, refreshed_ops))
        })?;

        if write_success.0 {
            let mut applied_operations = remote_state.applied_operations.clone();
            applied_operations.extend(local_pending_operations.clone());
            applied_operations.extend(write_success.1.clone());
            persist_applied_operations(&connection, &applied_operations)?;
            persist_payload_entity_versions(&connection, &refreshed_payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 sync completed with manifest CAS protection".to_string(),
                direction: Some("download_upload".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} pull_only=false",
                sync_id, result.status
            );
            return Ok(result);
        }

        if !local_pending_operations.is_empty() || !write_success.1.is_empty() {
            let mut applied_operations = remote_state.applied_operations.clone();
            applied_operations.extend(local_pending_operations.clone());
            applied_operations.extend(write_success.1.clone());
            persist_applied_operations(&connection, &applied_operations)?;
            persist_payload_entity_versions(&connection, &refreshed_payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 ops uploaded; manifest CAS will be reconciled by the next sync"
                    .to_string(),
                direction: Some("download_upload".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                Some("manifestConflict=true uploadedOps=true".to_string()),
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} manifest_conflict_uploaded_ops=true",
                sync_id, result.status
            );
            return Ok(result);
        }

        if attempt == 2 {
            let result = skipped_object_storage_auto_sync(
                "r2_manifest_conflict",
                "R2 manifest conflict; local data kept and retry will continue next time",
                Some(object_url),
            );
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            return Ok(result);
        }
    }

    Ok(skipped_object_storage_auto_sync(
        "r2_retry_exhausted",
        "R2 v3 sync retry exhausted",
        None,
    ))
}

fn with_object_storage_sync_lock<F>(work: F) -> Result<ObjectStorageAutoSyncResult, String>
where
    F: FnOnce() -> Result<ObjectStorageAutoSyncResult, String>,
{
    let _guard = match OBJECT_STORAGE_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Ok(skipped_object_storage_auto_sync(
                "object_storage_sync_in_flight",
                "object storage sync already in flight, skipped",
                None,
            ));
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err(
                "object storage sync lock is poisoned, please restart the app and retry"
                    .to_string(),
            );
        }
    };
    work()
}
fn auto_sync_object_storage_database_pull_only(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    sync_r2_v3_object_storage(app, "periodic_pull", true)
}
pub(crate) fn sync_object_storage_after_external_change(
    app: AppHandle,
    trigger: &str,
) -> Result<ObjectStorageAutoSyncResult, String> {
    with_object_storage_sync_lock(|| {
        auto_sync_object_storage_database_blocking_with_trigger(app, trigger)
    })
}

pub(crate) fn poll_object_storage_for_remote_changes(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    with_object_storage_sync_lock(|| {
        if has_pending_object_storage_local_changes(app.clone()).unwrap_or(false) {
            auto_sync_object_storage_database_blocking_with_trigger(app, "periodic_local_change")
        } else {
            auto_sync_object_storage_database_pull_only(app)
        }
    })
}

fn auto_sync_object_storage_database_blocking_with_trigger(
    app: AppHandle,
    trigger: &str,
) -> Result<ObjectStorageAutoSyncResult, String> {
    sync_r2_v3_object_storage(app, trigger, false)
}

fn has_pending_object_storage_local_changes(app: AppHandle) -> Result<bool, String> {
    let settings = get_object_storage_settings(app.clone())?;
    if !settings.enabled || !object_storage_configured(&settings) {
        return Ok(false);
    }
    let local_database_path = database_path(&app)?;
    let connection = open_database(&local_database_path)?;
    let device_id = load_or_create_device_id(&connection)?;
    let entity_versions = load_local_entity_versions(&connection)?;
    let payload = export_shared_sync_payload(
        &connection,
        device_id.clone(),
        Utc::now().timestamp_millis(),
    )?;
    Ok(!payload_entity_operations(&payload, &device_id, &entity_versions)?.is_empty())
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
            return Err("Please enter the WebDAV URL.".to_string());
        }

        Url::parse(&url)
            .map_err(|_| "WebDAV URL is invalid; use an http or https URL.".to_string())?;

        if remote_path.is_empty() {
            return Err("Please enter the remote file path.".to_string());
        }
    } else if !url.is_empty() {
        Url::parse(&url)
            .map_err(|_| "WebDAV URL is invalid; use an http or https URL.".to_string())?;
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
            return Err("Please enter the R2 endpoint.".to_string());
        }

        Url::parse(&endpoint)
            .map_err(|_| "R2 endpoint is invalid; use an http or https URL.".to_string())?;

        if bucket.is_empty() {
            return Err("Please enter the R2 bucket.".to_string());
        }

        if access_key_id.is_empty() {
            return Err("Please enter Access Key ID.".to_string());
        }

        if secret_access_key.trim().is_empty() {
            return Err("Please enter Secret Access Key.".to_string());
        }

        if object_key.contains("..") {
            return Err(
                "Object key is invalid; use a path like study-sync.json or a folder prefix."
                    .to_string(),
            );
        }
    } else if !endpoint.is_empty() {
        Url::parse(&endpoint)
            .map_err(|_| "R2 endpoint is invalid; use an http or https URL.".to_string())?;
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

fn r2_v3_prefix(settings: &ObjectStorageSettings) -> String {
    let key = settings
        .object_key
        .trim()
        .trim_matches('/')
        .replace('\\', "/");
    if key.is_empty() || key == DEFAULT_OBJECT_KEY || key.ends_with(".json") {
        R2_V3_DEFAULT_PREFIX.to_string()
    } else {
        key
    }
}

fn r2_v3_key(settings: &ObjectStorageSettings, child: &str) -> String {
    format!(
        "{}/{}",
        r2_v3_prefix(settings),
        child.trim_start_matches('/')
    )
}

fn r2_v3_manifest_key(settings: &ObjectStorageSettings) -> String {
    r2_v3_key(settings, R2_V3_MANIFEST)
}

fn r2_v3_active_lock_key(settings: &ObjectStorageSettings) -> String {
    r2_v3_key(settings, "runtime/active-lock.json")
}

fn empty_shared_payload(device_id: &str, exported_at: i64) -> SharedSyncPayload {
    SharedSyncPayload {
        schema_version: R2_V3_SCHEMA_VERSION,
        device_id: device_id.to_string(),
        exported_at,
        source_device_id: Some(device_id.to_string()),
        active_device_id: None,
        subjects: Vec::new(),
        study_modes: Vec::new(),
        focus_sessions: Vec::new(),
        app_events: Vec::new(),
        checklist_tasks: Vec::new(),
        today_plan_items: Vec::new(),
        schedule_blocks: Vec::new(),
        schedule_templates: Vec::new(),
        daily_reviews: Vec::new(),
        weekly_reviews: Vec::new(),
    }
}

fn sanitize_op_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_logical_suffix(entity_type: &str, sync_id: &str, action: &str) -> i64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!("{entity_type}:{sync_id}:{action}").as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash % (R2_V3_HLC_LOGICAL_MODULUS as u64)) as i64
}

fn operation_seq(updated_at: i64, entity_type: &str, sync_id: &str, action: &str) -> i64 {
    updated_at
        .max(0)
        .saturating_mul(R2_V3_HLC_LOGICAL_MODULUS)
        .saturating_add(stable_logical_suffix(entity_type, sync_id, action))
}

fn hlc_from_updated_at(
    updated_at: i64,
    entity_type: &str,
    sync_id: &str,
    action: &str,
    device_id: &str,
) -> String {
    format!(
        "{:020}-{:05}-{}",
        updated_at.max(0),
        stable_logical_suffix(entity_type, sync_id, action),
        sanitize_op_part(device_id)
    )
}

fn operation_sort_key(operation: &R2V3Operation) -> (i64, String) {
    (operation.seq, operation.op_id.clone())
}

fn op_key_device_seq(key: &str) -> Option<(String, i64)> {
    let mut parts = key.rsplitn(3, '/');
    let file_name = parts.next()?;
    let device_id = parts.next()?.to_string();
    let seq = file_name.split_once('-')?.0.parse::<i64>().ok()?;
    Some((device_id, seq))
}

fn op_id_from_key(key: &str) -> Option<String> {
    key.rsplit('/')
        .next()
        .and_then(|file_name| file_name.split_once('-').map(|(_, op_id)| op_id))
        .map(|op_id| op_id.trim_end_matches(".json").to_string())
}

fn payload_entity_operations(
    payload: &SharedSyncPayload,
    device_id: &str,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Result<Vec<R2V3Operation>, String> {
    let value = serde_json::to_value(payload).map_err(|error| error.to_string())?;
    let fields = [
        ("subjects", "subject"),
        ("studyModes", "study_mode"),
        ("focusSessions", "focus_session"),
        ("appEvents", "app_event"),
        ("checklistTasks", "checklist_task"),
        ("todayPlanItems", "today_plan_item"),
        ("scheduleBlocks", "schedule_block"),
        ("scheduleTemplates", "schedule_template"),
        ("dailyReviews", "daily_review"),
        ("weeklyReviews", "weekly_review"),
    ];

    let mut operations = Vec::new();
    for (field, entity_type) in fields {
        let Some(items) = value.get(field).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let sync_id = item
                .get("syncId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if sync_id.is_empty() {
                continue;
            }
            let updated_at = item
                .get("updatedAt")
                .and_then(Value::as_i64)
                .unwrap_or(payload.exported_at);
            let deleted_at = item.get("deletedAt").and_then(Value::as_i64);
            let action = if deleted_at.is_some() {
                "delete"
            } else {
                "upsert"
            };
            let version_key = (entity_type.to_string(), sync_id.to_string());
            if let Some(version) = entity_versions.get(&version_key) {
                if version.updated_at == updated_at && version.deleted_at == deleted_at {
                    continue;
                }
            }
            let hlc = hlc_from_updated_at(updated_at, entity_type, sync_id, action, device_id);
            let op_id = sanitize_op_part(&format!("{entity_type}-{sync_id}-{hlc}-{action}"));
            operations.push(R2V3Operation {
                schema_version: R2_V3_SCHEMA_VERSION,
                op_id,
                device_id: device_id.to_string(),
                seq: operation_seq(updated_at, entity_type, sync_id, action),
                hlc,
                base_hlc: entity_versions
                    .get(&version_key)
                    .map(|version| version.hlc.clone()),
                entity_type: entity_type.to_string(),
                sync_id: sync_id.to_string(),
                action: action.to_string(),
                payload: Some(item.clone()),
                deleted_at,
            });
        }
    }

    operations.sort_by_key(operation_sort_key);
    operations
        .dedup_by(|left, right| left.op_id == right.op_id && left.device_id == right.device_id);
    Ok(operations)
}

fn payload_from_operations(
    operations: &[R2V3Operation],
    device_id: &str,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let mut payload = empty_shared_payload(device_id, exported_at);
    let mut operations = operations.to_vec();
    operations.sort_by_key(operation_sort_key);

    for operation in operations {
        let Some(value) = operation.payload else {
            continue;
        };
        match operation.entity_type.as_str() {
            "subject" => payload.subjects.push(
                serde_json::from_value::<SharedSubject>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "study_mode" => payload.study_modes.push(
                serde_json::from_value::<SharedStudyMode>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "focus_session" => payload.focus_sessions.push(
                serde_json::from_value::<SharedFocusSession>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "app_event" => payload.app_events.push(
                serde_json::from_value::<SharedAppEvent>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "checklist_task" => payload.checklist_tasks.push(
                serde_json::from_value::<SharedChecklistTask>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "today_plan_item" => payload.today_plan_items.push(
                serde_json::from_value::<SharedTodayPlanItem>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "schedule_block" => payload.schedule_blocks.push(
                serde_json::from_value::<SharedScheduleBlock>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "schedule_template" => payload.schedule_templates.push(
                serde_json::from_value::<SharedScheduleTemplate>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "daily_review" => payload.daily_reviews.push(
                serde_json::from_value::<SharedDailyReview>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "weekly_review" => payload.weekly_reviews.push(
                serde_json::from_value::<SharedWeeklyReview>(value)
                    .map_err(|error| error.to_string())?,
            ),
            _ => {}
        }
    }

    Ok(payload)
}

fn apply_operations_to_payload(
    base: SharedSyncPayload,
    operations: &[R2V3Operation],
    device_id: &str,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let ops_payload = payload_from_operations(operations, device_id, exported_at)?;
    Ok(merge_shared_sync_payloads(
        base,
        ops_payload,
        device_id.to_string(),
        exported_at,
    ))
}

fn load_local_entity_versions(
    connection: &Connection,
) -> Result<HashMap<(String, String), LocalEntityVersion>, String> {
    let mut statement = connection
        .prepare(
            "SELECT entity_type, sync_id, hlc, deleted_at, updated_at
             FROM sync_entity_versions",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                LocalEntityVersion {
                    hlc: row.get(2)?,
                    deleted_at: row.get(3)?,
                    updated_at: row.get(4)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut versions = HashMap::new();
    for row in rows {
        let (key, value) = row.map_err(|error| error.to_string())?;
        versions.insert(key, value);
    }
    Ok(versions)
}

fn load_applied_operation_ids(connection: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare("SELECT op_id FROM sync_applied_ops")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut op_ids = HashSet::new();
    for row in rows {
        op_ids.insert(row.map_err(|error| error.to_string())?);
    }
    Ok(op_ids)
}

fn should_apply_incoming_operation(
    current: Option<&LocalEntityVersion>,
    operation: &R2V3Operation,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    if operation.action == "delete" {
        return operation.hlc >= current.hlc;
    }
    if current.deleted_at.is_some() {
        if let Some(base_hlc) = operation.base_hlc.as_deref() {
            if current.hlc.as_str() > base_hlc {
                return false;
            }
        } else if current.hlc >= operation.hlc {
            return false;
        }
    }
    operation.hlc > current.hlc
}

fn operation_version(operation: &R2V3Operation) -> LocalEntityVersion {
    let updated_at = operation
        .payload
        .as_ref()
        .and_then(|payload| payload.get("updatedAt"))
        .and_then(Value::as_i64)
        .or_else(|| {
            operation
                .hlc
                .split('-')
                .next()
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or_default();
    LocalEntityVersion {
        hlc: operation.hlc.clone(),
        deleted_at: operation.deleted_at.or_else(|| {
            operation
                .payload
                .as_ref()
                .and_then(|payload| payload.get("deletedAt"))
                .and_then(Value::as_i64)
        }),
        updated_at,
    }
}

fn filter_incoming_operations(
    operations: Vec<R2V3Operation>,
    applied_operation_ids: &HashSet<String>,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Vec<R2V3Operation> {
    let mut known_versions = entity_versions.clone();
    let mut accepted = Vec::new();
    let mut sorted = operations;
    sorted.sort_by_key(operation_sort_key);

    for operation in sorted {
        if applied_operation_ids.contains(&operation.op_id) {
            continue;
        }
        let key = (operation.entity_type.clone(), operation.sync_id.clone());
        if !should_apply_incoming_operation(known_versions.get(&key), &operation) {
            continue;
        }
        known_versions.insert(key, operation_version(&operation));
        accepted.push(operation);
    }

    accepted
}

fn persist_applied_operations(
    connection: &Connection,
    operations: &[R2V3Operation],
) -> Result<(), String> {
    if operations.is_empty() {
        return Ok(());
    }
    let now = Utc::now().timestamp_millis();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut insert_applied = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_applied_ops (op_id, device_id, seq, hlc, applied_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        let mut upsert_version = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_entity_versions (entity_type, sync_id, hlc, deleted_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        for operation in operations {
            insert_applied
                .execute((
                    &operation.op_id,
                    &operation.device_id,
                    operation.seq,
                    &operation.hlc,
                    now,
                ))
                .map_err(|error| error.to_string())?;
            let version = operation_version(operation);
            upsert_version
                .execute((
                    &operation.entity_type,
                    &operation.sync_id,
                    &version.hlc,
                    version.deleted_at,
                    version.updated_at,
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn persist_payload_entity_versions(
    connection: &Connection,
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<(), String> {
    let current_versions = load_local_entity_versions(connection)?;
    let candidates = payload_entity_version_records(payload, device_id)?;
    let versions: Vec<_> = candidates
        .into_iter()
        .filter(|(entity_type, sync_id, version)| {
            let Some(current) = current_versions.get(&(entity_type.clone(), sync_id.clone()))
            else {
                return true;
            };
            version.hlc > current.hlc
                || (version.hlc == current.hlc
                    && version.updated_at >= current.updated_at
                    && version.deleted_at != current.deleted_at)
        })
        .collect();
    if versions.is_empty() {
        return Ok(());
    }

    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut upsert_version = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_entity_versions (entity_type, sync_id, hlc, deleted_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        for (entity_type, sync_id, version) in versions {
            upsert_version
                .execute((
                    entity_type,
                    sync_id,
                    version.hlc,
                    version.deleted_at,
                    version.updated_at,
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn payload_entity_version_records(
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<Vec<(String, String, LocalEntityVersion)>, String> {
    let value = serde_json::to_value(payload).map_err(|error| error.to_string())?;
    let fields = [
        ("subjects", "subject"),
        ("studyModes", "study_mode"),
        ("focusSessions", "focus_session"),
        ("appEvents", "app_event"),
        ("checklistTasks", "checklist_task"),
        ("todayPlanItems", "today_plan_item"),
        ("scheduleBlocks", "schedule_block"),
        ("scheduleTemplates", "schedule_template"),
        ("dailyReviews", "daily_review"),
        ("weeklyReviews", "weekly_review"),
    ];
    let version_device_id = payload
        .source_device_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(device_id);
    let mut versions: HashMap<(String, String), LocalEntityVersion> = HashMap::new();

    for (field, entity_type) in fields {
        let Some(items) = value.get(field).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let sync_id = item
                .get("syncId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if sync_id.is_empty() {
                continue;
            }
            let updated_at = item
                .get("updatedAt")
                .and_then(Value::as_i64)
                .unwrap_or(payload.exported_at);
            let deleted_at = item.get("deletedAt").and_then(Value::as_i64);
            let action = if deleted_at.is_some() {
                "delete"
            } else {
                "upsert"
            };
            let version = LocalEntityVersion {
                hlc: hlc_from_updated_at(
                    updated_at,
                    entity_type,
                    sync_id,
                    action,
                    version_device_id,
                ),
                deleted_at,
                updated_at,
            };
            let key = (entity_type.to_string(), sync_id.to_string());
            let should_replace = versions
                .get(&key)
                .map(|current| version.hlc > current.hlc)
                .unwrap_or(true);
            if should_replace {
                versions.insert(key, version);
            }
        }
    }

    Ok(versions
        .into_iter()
        .map(|((entity_type, sync_id), version)| (entity_type, sync_id, version))
        .collect())
}

fn payload_entity_version_map(
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<HashMap<(String, String), LocalEntityVersion>, String> {
    Ok(payload_entity_version_records(payload, device_id)?
        .into_iter()
        .map(|(entity_type, sync_id, version)| ((entity_type, sync_id), version))
        .collect())
}

fn apply_operations_to_version_map(
    mut entity_versions: HashMap<(String, String), LocalEntityVersion>,
    operations: &[R2V3Operation],
) -> HashMap<(String, String), LocalEntityVersion> {
    for operation in operations {
        entity_versions.insert(
            (operation.entity_type.clone(), operation.sync_id.clone()),
            operation_version(operation),
        );
    }
    entity_versions
}

fn with_s3_runtime<T>(
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(R2_OPERATION_TIMEOUT_SECONDS * 3),
            future,
        )
        .await
        .map_err(|_| "R2 operation timed out; sync will retry later".to_string())?
    })
}

async fn object_storage_client(settings: &ObjectStorageSettings) -> Result<S3Client, String> {
    let credentials = Credentials::new(
        settings.access_key_id.clone(),
        settings.secret_access_key.clone(),
        None,
        None,
        "kaoyan-focus-object-storage",
    );
    let timeout_config = TimeoutConfig::builder()
        .connect_timeout(Duration::from_secs(R2_CONNECT_TIMEOUT_SECONDS))
        .read_timeout(Duration::from_secs(R2_READ_TIMEOUT_SECONDS))
        .operation_attempt_timeout(Duration::from_secs(R2_OPERATION_ATTEMPT_TIMEOUT_SECONDS))
        .operation_timeout(Duration::from_secs(R2_OPERATION_TIMEOUT_SECONDS))
        .build();
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .region(Region::new(settings.region.clone()))
        .timeout_config(timeout_config)
        .load()
        .await;
    let config = S3ConfigBuilder::from(&shared_config)
        .endpoint_url(settings.endpoint.clone())
        .force_path_style(true)
        .build();

    Ok(S3Client::from_conf(config))
}

fn is_missing_object_error(message: &str) -> bool {
    message.contains("NotFound") || message.contains("404") || message.contains("NoSuchKey")
}

fn is_precondition_error(message: &str) -> bool {
    message.contains("PreconditionFailed")
        || message.contains("412")
        || message.contains("ConditionalRequestConflict")
        || message.contains("409")
}

fn is_r2_conditional_put_conflict<E, R>(error: &SdkError<E, R>) -> bool
where
    E: ProvideErrorMetadata + std::fmt::Debug,
    R: std::fmt::Debug,
{
    if matches!(
        error.code(),
        Some("PreconditionFailed" | "ConditionalRequestConflict")
    ) {
        return true;
    }

    if let Some(response) = error.raw_response() {
        let text = format!("{response:?}");
        if is_precondition_error(&text) {
            return true;
        }
    }

    is_precondition_error(&format!("{error:?}")) || is_precondition_error(&error.to_string())
}

async fn get_object_bytes_with_etag(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
) -> Result<Option<(Vec<u8>, Option<String>)>, String> {
    match client
        .get_object()
        .bucket(&settings.bucket)
        .key(key)
        .send()
        .await
    {
        Ok(response) => {
            let etag = response.e_tag().map(ToString::to_string);
            let bytes = response
                .body
                .collect()
                .await
                .map_err(|error| format!("Read R2 object body failed: {error}"))?
                .into_bytes();
            Ok(Some((bytes.to_vec(), etag)))
        }
        Err(error) => {
            if error
                .as_service_error()
                .map(|service_error| service_error.is_no_such_key())
                .unwrap_or(false)
            {
                Ok(None)
            } else {
                let message = error.to_string();
                if is_missing_object_error(&message) {
                    Ok(None)
                } else {
                    Err(format!("Download R2 object {key} failed: {error:?}"))
                }
            }
        }
    }
}

async fn delete_object_if_exists(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
) -> Result<(), String> {
    match client
        .delete_object()
        .bucket(&settings.bucket)
        .key(key)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            if is_missing_object_error(&message) {
                Ok(())
            } else {
                Err(format!("Delete R2 object failed: {error}"))
            }
        }
    }
}

async fn put_object_if_none_match(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
    bytes: Vec<u8>,
) -> Result<bool, String> {
    let result = client
        .put_object()
        .bucket(&settings.bucket)
        .key(key)
        .body(ByteStream::from(bytes))
        .content_type("application/json")
        .if_none_match("*")
        .send()
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(error) => {
            if is_r2_conditional_put_conflict(&error) {
                Ok(false)
            } else {
                Err(format!("Upload R2 object failed: {error:?}"))
            }
        }
    }
}

async fn put_manifest_conditionally(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
    bytes: Vec<u8>,
    etag: Option<&str>,
) -> Result<bool, String> {
    let mut request = client
        .put_object()
        .bucket(&settings.bucket)
        .key(key)
        .body(ByteStream::from(bytes))
        .content_type("application/json");

    request = if let Some(etag) = etag {
        request.if_match(etag)
    } else {
        request.if_none_match("*")
    };

    match request.send().await {
        Ok(_) => Ok(true),
        Err(error) => {
            if is_r2_conditional_put_conflict(&error) {
                Ok(false)
            } else {
                Err(format!("Conditional R2 manifest update failed: {error:?}"))
            }
        }
    }
}

async fn try_acquire_r2_v3_active_lock(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
    active_snapshot: &SharedActiveStudySnapshot,
) -> Result<bool, String> {
    let key = r2_v3_active_lock_key(settings);
    let existing = get_object_bytes_with_etag(client, settings, &key).await?;
    let now = Utc::now().timestamp_millis();
    if let Some((bytes, _etag)) = existing.as_ref() {
        if !bytes.is_empty() {
            let lock: R2V3ActiveLock = serde_json::from_slice(bytes)
                .map_err(|error| format!("Parse R2 active lock failed: {error}"))?;
            if lock.expires_at > now
                && !lock.device_id.trim().is_empty()
                && lock.device_id != device_id
            {
                return Ok(false);
            }
        }
    }

    let lock = R2V3ActiveLock {
        schema_version: R2_V3_SCHEMA_VERSION,
        device_id: device_id.to_string(),
        sync_id: active_snapshot.sync_id.clone(),
        state_revision: active_snapshot.state_revision.unwrap_or_default(),
        hlc: hlc_from_updated_at(
            now,
            "active_lock",
            &active_snapshot.sync_id,
            "claim",
            device_id,
        ),
        claimed_at: now,
        expires_at: now + R2_V3_ACTIVE_LOCK_TTL_MILLIS,
    };
    let bytes = serde_json::to_vec(&lock).map_err(|error| error.to_string())?;
    match existing.and_then(|(_, etag)| etag) {
        Some(etag) => put_manifest_conditionally(client, settings, &key, bytes, Some(&etag)).await,
        None => put_object_if_none_match(client, settings, &key, bytes).await,
    }
}

async fn release_r2_v3_active_lock_if_owned(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
) -> Result<(), String> {
    let key = r2_v3_active_lock_key(settings);
    let Some((bytes, etag)) = get_object_bytes_with_etag(client, settings, &key).await? else {
        return Ok(());
    };
    if bytes.is_empty() {
        return Ok(());
    }
    let Ok(existing_lock) = serde_json::from_slice::<R2V3ActiveLock>(&bytes) else {
        return Ok(());
    };
    if existing_lock.device_id != device_id {
        return Ok(());
    }
    let now = Utc::now().timestamp_millis();
    let released_lock = R2V3ActiveLock {
        hlc: hlc_from_updated_at(
            now,
            "active_lock",
            &existing_lock.sync_id,
            "release",
            device_id,
        ),
        claimed_at: now,
        expires_at: now - 1,
        ..existing_lock
    };
    let bytes = serde_json::to_vec(&released_lock).map_err(|error| error.to_string())?;
    if let Some(etag) = etag.as_deref() {
        let _ = put_manifest_conditionally(client, settings, &key, bytes, Some(etag)).await?;
    }
    Ok(())
}

async fn upload_payload_operations(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    operations: &[R2V3Operation],
) -> Result<usize, String> {
    let mut uploaded = 0usize;
    for chunk in operations.chunks(R2_OP_UPLOAD_CONCURRENCY) {
        let mut tasks = Vec::with_capacity(chunk.len());
        for operation in chunk {
            let key = r2_v3_key(
                settings,
                &format!(
                    "ops/{}/{:020}-{}.json",
                    sanitize_op_part(&operation.device_id),
                    operation.seq.max(0),
                    operation.op_id
                ),
            );
            let bytes = serde_json::to_vec(&operation).map_err(|error| error.to_string())?;
            let client = client.clone();
            let settings = settings.clone();
            tasks.push(tokio::spawn(async move {
                put_object_if_none_match(&client, &settings, &key, bytes).await
            }));
        }
        for task in tasks {
            if task
                .await
                .map_err(|error| format!("Upload R2 op task failed: {error}"))??
            {
                uploaded += 1;
            }
        }
    }
    Ok(uploaded)
}

async fn list_r2_v3_operations(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    watermarks: &HashMap<String, i64>,
    applied_operation_ids: &HashSet<String>,
) -> Result<(Vec<R2V3Operation>, HashMap<String, i64>, usize), String> {
    let prefix = r2_v3_key(settings, "ops/");
    let mut continuation: Option<String> = None;
    let mut operations = Vec::new();
    let mut next_watermarks = watermarks.clone();
    let mut bytes_read = 0usize;
    let mut candidate_keys = Vec::new();

    loop {
        let mut request = client
            .list_objects_v2()
            .bucket(&settings.bucket)
            .prefix(&prefix);
        if let Some(token) = continuation.as_deref() {
            request = request.continuation_token(token);
        }
        let output = request
            .send()
            .await
            .map_err(|error| format!("List R2 ops failed: {error}"))?;

        for object in output.contents() {
            let Some(key) = object.key() else {
                continue;
            };
            if !key.ends_with(".json") {
                continue;
            }
            if let Some((device_id, seq)) = op_key_device_seq(key) {
                let watermark = watermarks.get(&device_id).copied().unwrap_or_default();
                if seq <= watermark {
                    continue;
                }
            }
            if op_id_from_key(key)
                .as_ref()
                .is_some_and(|op_id| applied_operation_ids.contains(op_id))
            {
                continue;
            }
            candidate_keys.push(key.to_string());
        }

        continuation = output.next_continuation_token().map(ToString::to_string);
        if continuation.is_none() {
            break;
        }
    }

    for chunk in candidate_keys.chunks(R2_OP_DOWNLOAD_CONCURRENCY) {
        let mut tasks = Vec::with_capacity(chunk.len());
        for key in chunk {
            let client = client.clone();
            let settings = settings.clone();
            let key = key.clone();
            tasks.push(tokio::spawn(async move {
                let bytes = get_object_bytes_with_etag(&client, &settings, &key).await?;
                Ok::<_, String>((key, bytes))
            }));
        }
        for task in tasks {
            let (key, bytes) = task
                .await
                .map_err(|error| format!("Download R2 op task failed: {error}"))??;
            let Some((bytes, _etag)) = bytes else {
                continue;
            };
            bytes_read += bytes.len();
            let operation: R2V3Operation = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Parse R2 op {key} failed: {error}"))?;
            let watermark = watermarks
                .get(&operation.device_id)
                .copied()
                .unwrap_or_default();
            if operation.seq <= watermark {
                continue;
            }
            next_watermarks
                .entry(operation.device_id.clone())
                .and_modify(|value| *value = (*value).max(operation.seq))
                .or_insert(operation.seq);
            operations.push(operation);
        }
    }

    operations.sort_by_key(operation_sort_key);
    Ok((operations, next_watermarks, bytes_read))
}

async fn list_object_storage_backup_objects_for_prefix(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    prefix: &str,
) -> Result<Vec<ObjectStorageBackupObject>, String> {
    let mut continuation: Option<String> = None;
    let mut objects = Vec::new();

    loop {
        let mut request = client
            .list_objects_v2()
            .bucket(&settings.bucket)
            .prefix(prefix);
        if let Some(token) = continuation.as_deref() {
            request = request.continuation_token(token);
        }
        let output = request
            .send()
            .await
            .map_err(|error| format!("List R2/S3 backups failed: {error}"))?;

        for object in output.contents() {
            let Some(key) = object.key() else {
                continue;
            };
            objects.push(ObjectStorageBackupObject {
                key: key.to_string(),
                size: object.size().and_then(|value| u64::try_from(value).ok()),
                last_modified: object.last_modified().and_then(|value| {
                    DateTime::<Utc>::from_timestamp(value.secs(), value.subsec_nanos())
                }),
            });
        }

        continuation = output.next_continuation_token().map(ToString::to_string);
        if continuation.is_none() {
            break;
        }
    }

    Ok(objects)
}

async fn list_all_object_storage_backup_objects(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<Vec<ObjectStorageBackupObject>, String> {
    let mut objects = list_object_storage_backup_objects_for_prefix(
        client,
        settings,
        &r2_v3_key(settings, "backups/"),
    )
    .await?;
    objects.extend(
        list_object_storage_backup_objects_for_prefix(client, settings, R2_LEGACY_BACKUP_PREFIX)
            .await?,
    );
    Ok(objects)
}

async fn load_legacy_v2_payload(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
) -> Result<(SharedSyncPayload, usize, bool), String> {
    let Some((bytes, _etag)) =
        get_object_bytes_with_etag(client, settings, &settings.object_key).await?
    else {
        return Ok((
            empty_shared_payload(device_id, Utc::now().timestamp_millis()),
            0,
            false,
        ));
    };
    if bytes.is_empty() {
        return Ok((
            empty_shared_payload(device_id, Utc::now().timestamp_millis()),
            0,
            false,
        ));
    }
    let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parse legacy study-sync.json failed: {error}"))?;
    Ok((payload, bytes.len(), true))
}

async fn load_r2_v3_remote_state(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
    applied_operation_ids: &HashSet<String>,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Result<R2V3RemoteState, String> {
    let manifest_key = r2_v3_manifest_key(settings);
    let Some((manifest_bytes, manifest_etag)) =
        get_object_bytes_with_etag(client, settings, &manifest_key).await?
    else {
        let (payload, bytes, migrated_legacy) =
            load_legacy_v2_payload(client, settings, device_id).await?;
        return Ok(R2V3RemoteState {
            manifest: None,
            manifest_etag: None,
            payload,
            watermarks: HashMap::new(),
            operation_count: 0,
            bytes,
            migrated_legacy,
            applied_operations: Vec::new(),
        });
    };

    let manifest: R2V3Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Parse R2 manifest failed: {error}"))?;
    let exported_at = Utc::now().timestamp_millis();
    let snapshot = if let Some(snapshot_key) = manifest.current_snapshot_key.as_deref() {
        match get_object_bytes_with_etag(client, settings, snapshot_key).await? {
            Some((snapshot_bytes, _etag)) => Some(
                serde_json::from_slice::<R2V3Snapshot>(&snapshot_bytes)
                    .map_err(|error| format!("Parse R2 snapshot failed: {error}"))?,
            ),
            None => None,
        }
    } else {
        None
    };
    let mut payload = snapshot
        .as_ref()
        .map(|snapshot| snapshot.payload.clone())
        .unwrap_or_else(|| empty_shared_payload(device_id, exported_at));
    let snapshot_watermarks = snapshot
        .as_ref()
        .map(|snapshot| snapshot.watermarks.clone())
        .unwrap_or_default();

    let (operations, watermarks, op_bytes) = list_r2_v3_operations(
        client,
        settings,
        &snapshot_watermarks,
        applied_operation_ids,
    )
    .await?;
    let applied_operations =
        filter_incoming_operations(operations, applied_operation_ids, entity_versions);
    if !applied_operations.is_empty() {
        payload =
            apply_operations_to_payload(payload, &applied_operations, device_id, exported_at)?;
    }

    Ok(R2V3RemoteState {
        manifest: Some(manifest),
        manifest_etag,
        payload,
        watermarks,
        operation_count: applied_operations.len(),
        bytes: manifest_bytes.len() + op_bytes,
        migrated_legacy: false,
        applied_operations,
    })
}

async fn write_r2_v3_snapshot_and_manifest(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    mut payload: SharedSyncPayload,
    watermarks: HashMap<String, i64>,
    manifest_etag: Option<&str>,
    device_id: &str,
) -> Result<bool, String> {
    let now = Utc::now();
    payload.schema_version = R2_V3_SCHEMA_VERSION;
    let snapshot_id = format!("{}-{}", now.format("%Y%m%dT%H%M%S%.3fZ"), Uuid::new_v4());
    let snapshot_key = r2_v3_key(settings, &format!("snapshots/{snapshot_id}.json"));
    let backup_key = r2_v3_key(
        settings,
        &format!(
            "backups/{}-{}.json",
            now.format("%Y%m%dT%H%M%S%.3fZ"),
            sanitize_op_part(device_id)
        ),
    );
    let snapshot = R2V3Snapshot {
        schema_version: R2_V3_SCHEMA_VERSION,
        snapshot_id: snapshot_id.clone(),
        created_hlc: hlc_from_updated_at(
            now.timestamp_millis(),
            "snapshot",
            &snapshot_id,
            "compact",
            device_id,
        ),
        watermarks: watermarks.clone(),
        payload,
    };
    let snapshot_bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    let _ = put_object_if_none_match(client, settings, &backup_key, snapshot_bytes.clone()).await?;
    prune_object_storage_backups_with_client_best_effort(client, settings).await;
    put_object_if_none_match(client, settings, &snapshot_key, snapshot_bytes).await?;

    let manifest = R2V3Manifest {
        schema_version: R2_V3_SCHEMA_VERSION,
        current_snapshot_key: Some(snapshot_key),
        watermarks,
        compacted_at: now.to_rfc3339(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    put_manifest_conditionally(
        client,
        settings,
        &r2_v3_manifest_key(settings),
        manifest_bytes,
        manifest_etag,
    )
    .await
}

async fn write_r2_v3_manifest(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    current_snapshot_key: Option<String>,
    watermarks: HashMap<String, i64>,
    manifest_etag: Option<&str>,
) -> Result<bool, String> {
    let manifest = R2V3Manifest {
        schema_version: R2_V3_SCHEMA_VERSION,
        current_snapshot_key,
        watermarks,
        compacted_at: Utc::now().to_rfc3339(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    put_manifest_conditionally(
        client,
        settings,
        &r2_v3_manifest_key(settings),
        manifest_bytes,
        manifest_etag,
    )
    .await
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
                Err(format!("闂備浇宕垫慨鏉懨洪埡鍜佹晪鐟滄垿濡甸幇鏉跨倞闁宠鍎虫禍楣冩煟閵忊槅鍟忛柡鈧导瀛樼參闁哄诞鍐句紑闂侀€炲苯澧紒瀣浮閺佸姊虹粙娆惧剰缂佸娼ч…鍥╂嫚瀹割喚鍙嗛梺鍛婄矆濡炴帞妲愰幘缁樷拺缂備焦锕╅悞楣冩煕閳哄倻澧甸柛鈹惧亾濡炪倖鍨煎▔鏇⑺囬敃鍌涚厪闁搞儯鍔岄悘鈺呮煙瀹勬壆绉哄┑锛勫厴椤㈡稑顭ㄩ崨顖氱倞{error}"))
            }
        }
    }
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
            .map_err(|_| "WebDAV URL does not support path segments.".to_string())?;
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
                .map_err(|_| "WebDAV URL does not support directory creation.".to_string())?;
            segments.push(part);
        }

        let response = webdav_request(
            client,
            Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
            current.clone(),
            settings,
        )
        .send()
        .map_err(|error| format!("Create remote WebDAV directory failed: {error}"))?;

        if response.status().is_success()
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
            || response.status() == StatusCode::CONFLICT
        {
            continue;
        }

        return Err(format!(
            "Create remote WebDAV directory failed with status: {}",
            response.status().as_u16()
        ));
    }

    Ok(())
}

fn validate_sqlite_database(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("File is not a valid SQLite database: {error}"))?;
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("SQLite integrity_check failed: {error}"))
        .and_then(|result| {
            if result == "ok" {
                Ok(())
            } else {
                Err(format!("SQLite integrity_check failed: {result}"))
            }
        })
}

fn ensure_no_active_runtime(path: &Path) -> Result<(), String> {
    if has_active_runtime(path)? {
        return Err(
            "A study mode is currently running; finish it before restoring data.".to_string(),
        );
    }

    Ok(())
}

fn has_active_runtime(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let connection = open_database(path)?;
    let active_runtime_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM study_modes sm
            WHERE sm.status = 'active'
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(active_runtime_count > 0)
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn backup_database_path_with_prefix(app_data_dir: &Path, prefix: &str) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    app_data_dir.join(format!("kaoyan-focus.before-{prefix}-{stamp}.sqlite3"))
}

fn create_local_sync_backup(
    app: &AppHandle,
    local_database_path: &Path,
    prefix: &str,
) -> Result<Option<String>, String> {
    if !local_database_path.exists() {
        return Ok(None);
    }
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_database_path_with_prefix(&app_data_dir, prefix);
    fs::copy(local_database_path, &backup_path)
        .map_err(|error| format!("Create local sync backup failed: {error}"))?;
    prune_local_sync_backups_best_effort(&app_data_dir);
    Ok(Some(backup_path.to_string_lossy().to_string()))
}

fn list_object_storage_backup_entries(
    settings: &ObjectStorageSettings,
) -> Result<Vec<SyncBackupEntry>, String> {
    with_s3_runtime(async {
        let client = object_storage_client(settings).await?;
        let entries = list_all_object_storage_backup_objects(&client, settings)
            .await?
            .into_iter()
            .map(|object| SyncBackupEntry {
                source: "r2".to_string(),
                label: object
                    .key
                    .rsplit('/')
                    .next()
                    .unwrap_or(&object.key)
                    .to_string(),
                key: object.key,
                created_at: object.last_modified.map(|date| date.to_rfc3339()),
                bytes: object.size,
            })
            .collect();
        Ok(entries)
    })
}

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS).expect("valid UTC+8 offset")
}

fn current_backup_keep_dates(now: DateTime<Utc>) -> (NaiveDate, HashSet<NaiveDate>) {
    let today = now.with_timezone(&beijing_offset()).date_naive();
    let yesterday = (now - ChronoDuration::days(1))
        .with_timezone(&beijing_offset())
        .date_naive();
    let mut keep_dates = HashSet::new();
    keep_dates.insert(today);
    keep_dates.insert(yesterday);
    (today, keep_dates)
}

fn should_delete_backup(
    modified_at: Option<DateTime<Utc>>,
    today: NaiveDate,
    keep_dates: &HashSet<NaiveDate>,
) -> bool {
    let Some(modified_at) = modified_at else {
        return false;
    };
    let backup_date = modified_at.with_timezone(&beijing_offset()).date_naive();
    if backup_date > today {
        return false;
    }
    !keep_dates.contains(&backup_date)
}

fn prune_local_sync_backups(app_data_dir: &Path) -> Result<(), String> {
    if !app_data_dir.exists() {
        return Ok(());
    }

    let (today, keep_dates) = current_backup_keep_dates(Utc::now());
    for entry in fs::read_dir(app_data_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("kaoyan-focus.before-") || !name.ends_with(".sqlite3") {
            continue;
        }
        let modified_at = match entry.metadata() {
            Ok(metadata) => metadata.modified().ok().map(DateTime::<Utc>::from),
            Err(error) => {
                eprintln!(
                    "Keep local sync backup without metadata {}: {error}",
                    path.display()
                );
                None
            }
        };
        if !should_delete_backup(modified_at, today, &keep_dates) {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Delete local sync backup {} failed: {error}",
                    path.display()
                ));
            }
        }
    }

    Ok(())
}

fn prune_local_sync_backups_best_effort(app_data_dir: &Path) {
    if let Err(error) = prune_local_sync_backups(app_data_dir) {
        eprintln!("Local sync backup prune failed: {error}");
    }
}

async fn prune_object_storage_backups_with_client(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<(), String> {
    let (today, keep_dates) = current_backup_keep_dates(Utc::now());
    for object in list_all_object_storage_backup_objects(client, settings).await? {
        if object.last_modified.is_none() {
            eprintln!("Keep R2 backup without last_modified: {}", object.key);
            continue;
        }
        if !should_delete_backup(object.last_modified, today, &keep_dates) {
            continue;
        }
        delete_object_if_exists(client, settings, &object.key).await?;
    }
    Ok(())
}

async fn prune_object_storage_backups_with_client_best_effort(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) {
    if let Err(error) = prune_object_storage_backups_with_client(client, settings).await {
        eprintln!("R2 sync backup prune failed: {error}");
    }
}

fn prune_object_storage_backups(settings: &ObjectStorageSettings) -> Result<(), String> {
    with_s3_runtime(async {
        let client = object_storage_client(settings).await?;
        prune_object_storage_backups_with_client(&client, settings).await
    })
}

pub(crate) fn prune_sync_backups_best_effort(app: &AppHandle) {
    match app.path().app_data_dir() {
        Ok(app_data_dir) => prune_local_sync_backups_best_effort(&app_data_dir),
        Err(error) => eprintln!("Resolve app data dir for backup prune failed: {error}"),
    }

    match get_object_storage_settings(app.clone()) {
        Ok(settings) if settings.enabled && object_storage_configured(&settings) => {
            match normalize_object_storage_settings(settings) {
                Ok(normalized) => {
                    if let Err(error) = prune_object_storage_backups(&normalized) {
                        eprintln!("Scheduled R2 sync backup prune failed: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Normalize object storage settings for prune failed: {error}")
                }
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("Load object storage settings for prune failed: {error}"),
    }
}

fn load_backup_bytes(app: &AppHandle, source: &str, key: &str) -> Result<Vec<u8>, String> {
    if source == "local" {
        let path = PathBuf::from(key);
        return fs::read(&path).map_err(|error| format!("Read local backup failed: {error}"));
    }
    let settings = normalize_object_storage_settings(get_object_storage_settings(app.clone())?)?;
    with_s3_runtime(async {
        let client = object_storage_client(&settings).await?;
        let response = client
            .get_object()
            .bucket(&settings.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| format!("Download R2/S3 backup failed: {error}"))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|error| format!("Read R2/S3 backup failed: {error}"))?
            .into_bytes();
        Ok(bytes.to_vec())
    })
}

fn validate_sync_payload(payload: &SharedSyncPayload, local_now: Option<i64>) -> String {
    let mut warnings = Vec::new();
    if payload.schema_version <= 0 {
        warnings.push("schemaVersion is invalid".to_string());
    }
    if payload.device_id.trim().is_empty() {
        warnings.push("deviceId is empty".to_string());
    }
    let active_count = payload
        .study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(|status| status == "running" || status == "active")
                .unwrap_or(false)
        })
        .count();
    if active_count > 1 {
        warnings.push(format!("{active_count} active study modes were found"));
    }
    if let Some(now) = local_now {
        let drift_ms = now.saturating_sub(payload.exported_at).abs();
        if drift_ms > 120_000 {
            warnings.push(format!(
                "remote export clock drift is about {} seconds",
                drift_ms / 1000
            ));
        }
    }
    let entity_count = count_payload_entities(payload);
    let deleted_count = count_payload_deleted_entities(payload);
    if warnings.is_empty() {
        format!("validation passed: {entity_count} entities, {deleted_count} tombstones")
    } else {
        format!(
            "validation completed: {entity_count} entities, {deleted_count} tombstones; warnings: {}",
            warnings.join("; ")
        )
    }
}
fn record_object_sync_result(
    app: &AppHandle,
    sync_id: &str,
    trigger: &str,
    started_at: DateTime<Utc>,
    result: &ObjectStorageAutoSyncResult,
    payload: Option<&SharedSyncPayload>,
    remote_payload: Option<&SharedSyncPayload>,
    previous_active_snapshot: Option<&SharedActiveStudySnapshot>,
    validation_report: Option<String>,
    remote_backup_key: Option<String>,
) {
    let finished_at = Utc::now();
    let Ok(path) = database_path(app) else {
        return;
    };
    let Ok(connection) = open_database(&path) else {
        return;
    };
    let active_snapshot = payload.and_then(shared_active_study_snapshot);
    let remote_active_snapshot = remote_payload.and_then(shared_active_study_snapshot);
    let remote_exported_drift_seconds = remote_payload.map(|remote| {
        (Utc::now()
            .timestamp_millis()
            .saturating_sub(remote.exported_at))
        .abs()
            / 1000
    });
    let detail = build_sync_run_detail(
        trigger,
        result,
        previous_active_snapshot,
        active_snapshot.as_ref(),
        remote_active_snapshot.as_ref(),
        remote_exported_drift_seconds,
    );
    let record = SyncRunRecord {
        sync_id: sync_id.to_string(),
        backend: "object_storage".to_string(),
        trigger: trigger.to_string(),
        direction: result.direction.clone(),
        status: result.status.clone(),
        started_at,
        finished_at,
        device_id: payload.map(|value| value.device_id.clone()),
        remote_device_id: remote_payload.map(|value| value.device_id.clone()),
        remote_exported_at: remote_payload.map(|value| value.exported_at),
        local_exported_at: payload.map(|value| value.exported_at),
        bytes: result.bytes as i64,
        imported_count: payload.map(count_payload_entities).unwrap_or(0),
        exported_count: payload.map(count_payload_entities).unwrap_or(0),
        deleted_count: payload.map(count_payload_deleted_entities).unwrap_or(0),
        conflict_count: 0,
        active_state_changed: result.active_state_changed,
        took_over_active_mode: result.took_over_active_mode,
        validation_report,
        backup_path: result.backup_path.clone(),
        remote_backup_key,
        active_snapshot_sync_id: active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync_id.clone()),
        remote_active_snapshot_sync_id: remote_active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync_id.clone()),
        active_snapshot_phase: active_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.phase.clone()),
        remote_active_snapshot_phase: remote_active_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.phase.clone()),
        active_snapshot_updated_at: active_snapshot.as_ref().map(|snapshot| snapshot.updated_at),
        remote_snapshot_updated_at: remote_active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.updated_at),
        remote_exported_drift_seconds,
        detail: Some(detail),
        error_message: result.skipped_reason.clone(),
    };
    let _ = insert_sync_run(&connection, &record);
}

fn build_sync_run_detail(
    trigger: &str,
    result: &ObjectStorageAutoSyncResult,
    previous_active: Option<&SharedActiveStudySnapshot>,
    active: Option<&SharedActiveStudySnapshot>,
    remote_active: Option<&SharedActiveStudySnapshot>,
    remote_drift_seconds: Option<i64>,
) -> String {
    let decision = match (previous_active, active, remote_active) {
        (Some(previous), Some(current), Some(remote))
            if current.sync_id == remote.sync_id
                && previous.sync_id != current.sync_id
                && remote.updated_at > previous.updated_at =>
        {
            "takeover"
        }
        (Some(previous), Some(current), Some(remote))
            if previous.sync_id == current.sync_id && remote.updated_at < previous.updated_at =>
        {
            "rejected_stale_remote"
        }
        (Some(previous), Some(current), Some(remote))
            if previous.sync_id == current.sync_id
                && remote.sync_id != current.sync_id
                && remote.updated_at == previous.updated_at =>
        {
            "tie_kept_local"
        }
        (None, Some(current), Some(remote))
            if current.sync_id == remote.sync_id && result.took_over_active_mode =>
        {
            "accepted_remote"
        }
        _ if result.took_over_active_mode => "takeover",
        _ => "kept_local",
    };
    let pull_mode = if trigger == "periodic_pull" {
        "pull_only"
    } else {
        "full"
    };
    let drift = remote_drift_seconds
        .map(|seconds| seconds.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "{pull_mode} decision={decision}; localBefore={}; active={}; remote={}; remoteDriftSeconds={drift}",
        format_active_snapshot(previous_active),
        format_active_snapshot(active),
        format_active_snapshot(remote_active)
    )
}

fn format_active_snapshot(snapshot: Option<&SharedActiveStudySnapshot>) -> String {
    snapshot
        .map(|value| {
            format!(
                "{}:{}:{}",
                value.sync_id,
                value.phase.as_deref().unwrap_or("unknown"),
                value.updated_at
            )
        })
        .unwrap_or_else(|| "none".to_string())
}

fn insert_sync_run(connection: &Connection, record: &SyncRunRecord) -> Result<i64, String> {
    connection
        .execute(
            "
            INSERT INTO sync_runs (
              sync_id, backend, trigger, direction, status, started_at, finished_at, duration_ms,
              device_id, remote_device_id, remote_exported_at, local_exported_at, bytes,
              imported_count, exported_count, deleted_count, conflict_count,
              active_state_changed, took_over_active_mode, validation_report, backup_path,
              remote_backup_key, active_snapshot_sync_id, remote_active_snapshot_sync_id,
              active_snapshot_phase, remote_active_snapshot_phase, active_snapshot_updated_at,
              remote_snapshot_updated_at, remote_exported_drift_seconds, detail, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)
            ",
            rusqlite::params![
                record.sync_id,
                record.backend,
                record.trigger,
                record.direction,
                record.status,
                record.started_at.to_rfc3339(),
                record.finished_at.to_rfc3339(),
                (record.finished_at - record.started_at).num_milliseconds(),
                record.device_id,
                record.remote_device_id,
                record.remote_exported_at,
                record.local_exported_at,
                record.bytes,
                record.imported_count,
                record.exported_count,
                record.deleted_count,
                record.conflict_count,
                if record.active_state_changed { 1 } else { 0 },
                if record.took_over_active_mode { 1 } else { 0 },
                record.validation_report,
                record.backup_path,
                record.remote_backup_key,
                record.active_snapshot_sync_id,
                record.remote_active_snapshot_sync_id,
                record.active_snapshot_phase,
                record.remote_active_snapshot_phase,
                record.active_snapshot_updated_at,
                record.remote_snapshot_updated_at,
                record.remote_exported_drift_seconds,
                record.detail,
                record.error_message,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(connection.last_insert_rowid())
}

fn row_to_sync_run_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncRunSummary> {
    Ok(SyncRunSummary {
        id: row.get(0)?,
        sync_id: row.get(1)?,
        backend: row.get(2)?,
        trigger: row.get(3)?,
        direction: row.get(4)?,
        status: row.get(5)?,
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        duration_ms: row.get(8)?,
        device_id: row.get(9)?,
        remote_device_id: row.get(10)?,
        remote_exported_at: row.get(11)?,
        local_exported_at: row.get(12)?,
        bytes: row.get(13)?,
        imported_count: row.get(14)?,
        exported_count: row.get(15)?,
        deleted_count: row.get(16)?,
        conflict_count: row.get(17)?,
        active_state_changed: row.get::<_, i64>(18)? != 0,
        took_over_active_mode: row.get::<_, i64>(19)? != 0,
        validation_report: row.get(20)?,
        backup_path: row.get(21)?,
        remote_backup_key: row.get(22)?,
        active_snapshot_sync_id: row.get(23)?,
        remote_active_snapshot_sync_id: row.get(24)?,
        active_snapshot_phase: row.get(25)?,
        remote_active_snapshot_phase: row.get(26)?,
        active_snapshot_updated_at: row.get(27)?,
        remote_snapshot_updated_at: row.get(28)?,
        remote_exported_drift_seconds: row.get(29)?,
        detail: row.get(30)?,
        error_message: row.get(31)?,
    })
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
        .map_err(|error| format!("Read WebDAV metadata failed: {error}"))?;

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
            "Read WebDAV metadata failed with status: {}",
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
        .map_err(|error| format!("Read local database metadata failed: {error}"))?
        .modified()
        .map_err(|error| format!("Read local database modified time failed: {error}"))?;
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
