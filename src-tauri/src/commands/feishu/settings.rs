use crate::{
    credential, runtime_health, storage::db::open_database, sync_package::mark_entity_deleted,
};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use reqwest::{
    blocking::{Client, Response},
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    StatusCode, Url,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, TryLockError,
    },
    thread,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const FEISHU_BASE: &str = "https://open.feishu.cn";
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:39781/feishu/callback";
const BRIDGE_CONTAINER_NAME: &str = "考研专注";
const TIME_ZONE: &str = "Asia/Shanghai";
const MARKER_PREFIX: &str = "[kaoyan-focus:";
const FEISHU_OAUTH_SCOPE: &str =
    "offline_access task:task:read task:task:write task:tasklist:read task:tasklist:write calendar:calendar calendar:calendar:readonly";

const FEISHU_SYNC_ENABLED_KEY: &str = "feishu_sync_enabled";
const FEISHU_APP_ID_KEY: &str = "feishu_app_id";
const FEISHU_APP_SECRET_KEY: &str = "feishu_app_secret";
const FEISHU_REDIRECT_URI_KEY: &str = "feishu_redirect_uri";
const FEISHU_ACCESS_TOKEN_KEY: &str = "feishu_user_access_token";
const FEISHU_REFRESH_TOKEN_KEY: &str = "feishu_refresh_token";
const FEISHU_TOKEN_EXPIRES_AT_KEY: &str = "feishu_token_expires_at";
const FEISHU_TASKLIST_GUID_KEY: &str = "feishu_tasklist_guid";
const FEISHU_LEGACY_TASKLIST_GUID_KEY: &str = "feishu_legacy_tasklist_guid";
const FEISHU_CALENDAR_ID_KEY: &str = "feishu_calendar_id";
const FEISHU_OAUTH_STATE_KEY: &str = "feishu_oauth_state";
const FEISHU_OAUTH_URL_KEY: &str = "feishu_oauth_url";
const FEISHU_OAUTH_MESSAGE_KEY: &str = "feishu_oauth_message";
const TASKLIST_KEY_POLITICS: &str = "politics";
const TASKLIST_KEY_ENGLISH: &str = "english";
const TASKLIST_KEY_MATH: &str = "math";
const TASKLIST_KEY_MAJOR: &str = "major";
const TASKLIST_KEY_GENERAL: &str = "general";
const TASKLIST_KEY_TODAY: &str = "today";
const TASKLIST_KEYS: [&str; 6] = [
    TASKLIST_KEY_POLITICS,
    TASKLIST_KEY_ENGLISH,
    TASKLIST_KEY_MATH,
    TASKLIST_KEY_MAJOR,
    TASKLIST_KEY_GENERAL,
    TASKLIST_KEY_TODAY,
];

const ENTITY_CHECKLIST_TASK: &str = "checklist_task";
const ENTITY_TODAY_PLAN_ITEM: &str = "today_plan_item";
const ENTITY_SCHEDULE_BLOCK: &str = "schedule_block";
const REMOTE_FEISHU_TASK: &str = "feishu_task";
const REMOTE_FEISHU_EVENT: &str = "feishu_calendar_event";

static FEISHU_SYNC_LOCK: Mutex<()> = Mutex::new(());
static FEISHU_LOCAL_CHANGE_SYNC_PENDING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuSyncSettings {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    #[serde(default)]
    pub app_secret_configured: bool,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuSyncStatus {
    pub enabled: bool,
    pub configured: bool,
    pub authenticated: bool,
    pub expires_at: Option<String>,
    pub tasklist_guid: Option<String>,
    pub tasklist_count: usize,
    pub tasklists: Vec<FeishuTasklistStatus>,
    pub calendar_id: Option<String>,
    pub redirect_uri: String,
    pub pending_authorization_url: Option<String>,
    pub pending_message: Option<String>,
    pub required_scopes: String,
    pub last_run: Option<FeishuSyncRunSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuTasklistStatus {
    pub key: String,
    pub label: String,
    pub guid: Option<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuOAuthLogin {
    pub status: String,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuLoginPollResult {
    pub status: String,
    pub message: String,
    pub authenticated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuSyncResult {
    pub status: String,
    pub message: String,
    pub pushed_count: i64,
    pub pulled_count: i64,
    pub deleted_count: i64,
    pub conflict_count: i64,
    pub task_count: i64,
    pub calendar_count: i64,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuRebuildResult {
    pub status: String,
    pub message: String,
    pub backup_path: String,
    pub remote_backup_path: String,
    pub deleted_tasklist_count: i64,
    pub uploaded_task_count: i64,
    pub tasklist_count: i64,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeishuSyncRunSummary {
    pub id: i64,
    pub run_id: String,
    pub trigger: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub pushed_count: i64,
    pub pulled_count: i64,
    pub deleted_count: i64,
    pub conflict_count: i64,
    pub task_count: i64,
    pub calendar_count: i64,
    pub message: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
struct TokenSet {
    access_token: String,
}

#[derive(Debug, Clone)]
struct FeishuLink {
    id: i64,
    remote_kind: String,
    remote_id: String,
    remote_parent_id: Option<String>,
    remote_etag: Option<String>,
    remote_change_key: Option<String>,
    remote_last_modified: Option<String>,
}

#[derive(Debug, Clone)]
struct LocalTask {
    entity_type: &'static str,
    id: i64,
    sync_id: String,
    tasklist_key: &'static str,
    title: String,
    note: Option<String>,
    due_date: Option<String>,
    completed: bool,
    updated_at: String,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct LocalScheduleBlock {
    id: i64,
    sync_id: String,
    schedule_date: String,
    title: String,
    note: Option<String>,
    start_minute: i64,
    end_minute: i64,
    status: String,
    updated_at: String,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct RemoteTask {
    id: String,
    tasklist_key: String,
    tasklist_guid: String,
    title: String,
    note: Option<String>,
    due_date: Option<String>,
    completed: bool,
    updated_millis: Option<i64>,
    marker_entity_type: Option<String>,
    marker_sync_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteEvent {
    id: String,
    title: String,
    note: Option<String>,
    schedule_date: String,
    start_minute: i64,
    end_minute: i64,
    updated_millis: Option<i64>,
    marker_sync_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SyncCounters {
    pushed_count: i64,
    pulled_count: i64,
    deleted_count: i64,
    conflict_count: i64,
    task_count: i64,
    calendar_count: i64,
}

struct FeishuContainers {
    tasklists: HashMap<String, String>,
    calendar_id: String,
}

#[tauri::command]
pub fn get_feishu_sync_settings(app: AppHandle) -> Result<FeishuSyncSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    read_feishu_settings(&connection)
}

#[tauri::command]
pub fn save_feishu_sync_settings(
    app: AppHandle,
    settings: FeishuSyncSettings,
) -> Result<FeishuSyncSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let current_app_id = get_setting(&connection, FEISHU_APP_ID_KEY, "")?;
    let current_secret = credential::get_secret(&connection, FEISHU_APP_SECRET_KEY)?;
    let secret_changed = !settings.app_secret.is_empty();
    let normalized = normalize_settings(resolve_feishu_secret(&connection, settings)?);
    let now = Utc::now().to_rfc3339();
    set_setting(
        &connection,
        FEISHU_SYNC_ENABLED_KEY,
        &normalized.enabled.to_string(),
        &now,
    )?;
    set_setting(&connection, FEISHU_APP_ID_KEY, &normalized.app_id, &now)?;
    if secret_changed {
        credential::set_secret(
            &connection,
            FEISHU_APP_SECRET_KEY,
            &normalized.app_secret,
            &now,
        )?;
    } else {
        credential::set_secret_if_changed(&connection, FEISHU_APP_SECRET_KEY, "", &now)?;
    }
    set_setting(
        &connection,
        FEISHU_REDIRECT_URI_KEY,
        &normalized.redirect_uri,
        &now,
    )?;
    if (!current_app_id.is_empty() && current_app_id != normalized.app_id)
        || (!current_secret.is_empty() && current_secret != normalized.app_secret)
    {
        clear_feishu_tokens(&connection)?;
    }
    Ok(redact_feishu_settings(normalized))
}

#[tauri::command]
pub fn get_feishu_sync_status(app: AppHandle) -> Result<FeishuSyncStatus, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_feishu_settings(&connection)?;
    let access_token = credential::get_secret(&connection, FEISHU_ACCESS_TOKEN_KEY)?;
    let expires_at = non_empty_setting(&connection, FEISHU_TOKEN_EXPIRES_AT_KEY)?;
    let tasklists = feishu_tasklist_statuses(&connection)?;
    let tasklist_count = tasklists.iter().filter(|item| item.ready).count();
    Ok(FeishuSyncStatus {
        enabled: settings.enabled,
        configured: !settings.app_id.is_empty() && settings.app_secret_configured,
        authenticated: is_feishu_access_token_usable(&access_token, expires_at.as_deref()),
        expires_at,
        tasklist_guid: non_empty_setting(
            &connection,
            &feishu_tasklist_setting_key(TASKLIST_KEY_GENERAL),
        )?
        .or(non_empty_setting(&connection, FEISHU_TASKLIST_GUID_KEY)?),
        tasklist_count,
        tasklists,
        calendar_id: non_empty_setting(&connection, FEISHU_CALENDAR_ID_KEY)?,
        redirect_uri: settings.redirect_uri,
        pending_authorization_url: non_empty_setting(&connection, FEISHU_OAUTH_URL_KEY)?,
        pending_message: non_empty_setting(&connection, FEISHU_OAUTH_MESSAGE_KEY)?,
        required_scopes: FEISHU_OAUTH_SCOPE.to_string(),
        last_run: last_feishu_sync_run(&connection)?,
    })
}

#[tauri::command]
pub fn start_feishu_oauth_login(app: AppHandle) -> Result<FeishuOAuthLogin, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_feishu_settings_for_api(&connection)?;
    if settings.app_id.is_empty() || settings.app_secret.is_empty() {
        return Err("请先填写飞书 App ID 和 App Secret。".to_string());
    }

    let bind_addr = callback_bind_addr(&settings.redirect_uri)?;
    let listener = TcpListener::bind(&bind_addr)
        .map_err(|error| format!("启动本地飞书登录回调失败：{error}"))?;
    let state = Uuid::new_v4().to_string();
    let authorization_url = format!(
        "{FEISHU_BASE}/open-apis/authen/v1/index?app_id={}&redirect_uri={}&state={}&scope={}",
        encode_form_component(&settings.app_id),
        encode_form_component(&settings.redirect_uri),
        encode_form_component(&state),
        encode_form_component(FEISHU_OAUTH_SCOPE),
    );
    let now = Utc::now().to_rfc3339();
    set_setting(&connection, FEISHU_OAUTH_STATE_KEY, &state, &now)?;
    set_setting(&connection, FEISHU_OAUTH_URL_KEY, &authorization_url, &now)?;
    set_setting(
        &connection,
        FEISHU_OAUTH_MESSAGE_KEY,
        "等待浏览器完成飞书授权。",
        &now,
    )?;

    let app_for_callback = app.clone();
    let expected_state = state.clone();
    thread::spawn(move || {
        let message = match receive_oauth_callback(listener, &expected_state) {
            Ok(code) => handle_oauth_code(&app_for_callback, &code),
            Err(error) => Err(error),
        };
        if let Ok(connection) =
            database_path(&app_for_callback).and_then(|path| open_database(&path))
        {
            let now = Utc::now().to_rfc3339();
            let _ = set_setting(
                &connection,
                FEISHU_OAUTH_MESSAGE_KEY,
                match &message {
                    Ok(message) => message,
                    Err(error) => error,
                },
                &now,
            );
            if message.is_ok() {
                let _ = set_setting(&connection, FEISHU_OAUTH_STATE_KEY, "", &now);
                let _ = set_setting(&connection, FEISHU_OAUTH_URL_KEY, "", &now);
                if let Ok(path) = database_path(&app_for_callback) {
                    let _ = sync_feishu_bridge_blocking(
                        path,
                        "oauth_success".to_string(),
                        Uuid::new_v4().to_string(),
                        Utc::now(),
                    );
                }
            }
        }
    });

    Ok(FeishuOAuthLogin {
        status: "pending".to_string(),
        authorization_url,
        redirect_uri: settings.redirect_uri,
        message: "已打开飞书授权入口，请在浏览器完成登录。".to_string(),
    })
}

#[tauri::command]
pub fn poll_feishu_oauth_login(app: AppHandle) -> Result<FeishuLoginPollResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let access_token = credential::get_secret(&connection, FEISHU_ACCESS_TOKEN_KEY)?;
    let expires_at = get_setting(&connection, FEISHU_TOKEN_EXPIRES_AT_KEY, "")?;
    let authenticated = is_feishu_access_token_usable(&access_token, Some(&expires_at));
    let message = get_setting(
        &connection,
        FEISHU_OAUTH_MESSAGE_KEY,
        if authenticated {
            "飞书登录成功。"
        } else {
            "还在等待浏览器授权。"
        },
    )?;
    Ok(FeishuLoginPollResult {
        status: if authenticated {
            "authenticated"
        } else {
            "pending"
        }
        .to_string(),
        message,
        authenticated,
    })
}

#[tauri::command]
pub fn logout_feishu(app: AppHandle) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    clear_feishu_tokens(&connection)
}

#[tauri::command]
pub async fn sync_feishu_bridge(
    app: AppHandle,
    trigger: Option<String>,
) -> Result<FeishuSyncResult, String> {
    let started_at = Utc::now();
    let trigger = trigger.unwrap_or_else(|| "manual".to_string());
    let run_id = Uuid::new_v4().to_string();
    let database_path = database_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        sync_feishu_bridge_blocking(database_path, trigger, run_id, started_at)
    })
    .await
    .map_err(|error| format!("飞书同步后台任务失败：{error}"))?
}

