use std::{
    collections::{HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, TryLockError},
    time::Duration,
};

use crate::{
    credential,
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
    header::{HeaderMap, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED},
    Method, StatusCode, Url,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const ACTIVE_CONTROL_ACTIONS: [&str; 6] = [
    "pause",
    "resume",
    "confirm_break",
    "finish",
    "emergency_exit",
    "switch_subject",
];
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
    #[serde(default)]
    pub password_configured: bool,
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
    pub primary_owner_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default)]
    pub secret_access_key_configured: bool,
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
    pub primary_owner_changed: bool,
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
    #[serde(default)]
    primary_owner_device_id: Option<String>,
    #[serde(default)]
    primary_owner_updated_at: Option<i64>,
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
