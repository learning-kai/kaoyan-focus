use crate::{storage::db::open_database, sync_package::mark_entity_deleted};
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
    let current_secret = get_setting(&connection, FEISHU_APP_SECRET_KEY, "")?;
    let normalized = normalize_settings(settings);
    let now = Utc::now().to_rfc3339();
    set_setting(
        &connection,
        FEISHU_SYNC_ENABLED_KEY,
        &normalized.enabled.to_string(),
        &now,
    )?;
    set_setting(&connection, FEISHU_APP_ID_KEY, &normalized.app_id, &now)?;
    set_setting(
        &connection,
        FEISHU_APP_SECRET_KEY,
        &normalized.app_secret,
        &now,
    )?;
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
    Ok(normalized)
}

#[tauri::command]
pub fn get_feishu_sync_status(app: AppHandle) -> Result<FeishuSyncStatus, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_feishu_settings(&connection)?;
    let access_token = get_setting(&connection, FEISHU_ACCESS_TOKEN_KEY, "")?;
    let expires_at = non_empty_setting(&connection, FEISHU_TOKEN_EXPIRES_AT_KEY)?;
    let tasklists = feishu_tasklist_statuses(&connection)?;
    let tasklist_count = tasklists.iter().filter(|item| item.ready).count();
    Ok(FeishuSyncStatus {
        enabled: settings.enabled,
        configured: !settings.app_id.is_empty() && !settings.app_secret.is_empty(),
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
    let settings = read_feishu_settings(&connection)?;
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
    let access_token = get_setting(&connection, FEISHU_ACCESS_TOKEN_KEY, "")?;
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

pub fn sync_feishu_bridge_after_local_change(app: AppHandle, trigger: &'static str) {
    if FEISHU_LOCAL_CHANGE_SYNC_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result = (|| -> Result<(), String> {
            let database_path = database_path(&app)?;
            let _sync_guard = FEISHU_SYNC_LOCK
                .lock()
                .map_err(|_| "飞书同步锁状态异常，请重启应用后再试。".to_string())?;
            let result = sync_feishu_bridge_blocking_locked(
                database_path,
                trigger.to_string(),
                Uuid::new_v4().to_string(),
                Utc::now(),
            )?;
            if result.status == "failed" {
                return Err(result.message);
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Feishu local-change sync failed: {error}");
        }
        FEISHU_LOCAL_CHANGE_SYNC_PENDING.store(false, Ordering::SeqCst);
    });
}

fn is_local_change_trigger(trigger: &str) -> bool {
    trigger.ends_with("_change")
}

#[tauri::command]
pub async fn rebuild_feishu_tasklists_from_local(
    app: AppHandle,
) -> Result<FeishuRebuildResult, String> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();
    let database_path = database_path(&app)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        rebuild_feishu_tasklists_from_local_blocking(
            database_path,
            app_data_dir,
            run_id,
            started_at,
        )
    })
    .await
    .map_err(|error| format!("飞书任务清单重建后台任务失败：{error}"))?
}

fn sync_feishu_bridge_blocking(
    database_path: std::path::PathBuf,
    trigger: String,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuSyncResult, String> {
    let _sync_guard = match FEISHU_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Ok(skipped_result("已有飞书同步正在执行，本次已跳过。"));
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err("飞书同步锁状态异常，请重启应用后再试。".to_string());
        }
    };
    sync_feishu_bridge_blocking_locked(database_path, trigger, run_id, started_at)
}

fn sync_feishu_bridge_blocking_locked(
    database_path: std::path::PathBuf,
    trigger: String,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuSyncResult, String> {
    let mut connection = open_database(&database_path)?;

    let result: Result<FeishuSyncResult, String> = (|| {
        let settings = read_feishu_settings(&connection)?;
        if !settings.enabled {
            return Ok(skipped_result("飞书同步已关闭。"));
        }
        if settings.app_id.is_empty() || settings.app_secret.is_empty() {
            return Ok(skipped_result("未配置飞书 App ID 或 App Secret。"));
        }
        let token = ensure_access_token(&connection)?;
        let feishu = FeishuClient::new(token.access_token)?;
        let containers = ensure_feishu_containers(&connection, &feishu)?;
        let mut counters = SyncCounters {
            pushed_count: 0,
            pulled_count: 0,
            deleted_count: 0,
            conflict_count: 0,
            task_count: 0,
            calendar_count: 0,
        };
        sync_tasks(
            &mut connection,
            &feishu,
            &containers.tasklists,
            &mut counters,
        )?;
        sync_calendar_events(
            &mut connection,
            &feishu,
            &containers.calendar_id,
            is_local_change_trigger(&trigger),
            &mut counters,
        )?;
        Ok(FeishuSyncResult {
            status: "synced".to_string(),
            message: format!(
                "飞书同步完成：任务 {} 项，日历 {} 项。",
                counters.task_count, counters.calendar_count
            ),
            pushed_count: counters.pushed_count,
            pulled_count: counters.pulled_count,
            deleted_count: counters.deleted_count,
            conflict_count: counters.conflict_count,
            task_count: counters.task_count,
            calendar_count: counters.calendar_count,
            synced_at: Utc::now().to_rfc3339(),
        })
    })();

    let final_result = match result {
        Ok(value) => value,
        Err(error) => FeishuSyncResult {
            status: "failed".to_string(),
            message: error.clone(),
            pushed_count: 0,
            pulled_count: 0,
            deleted_count: 0,
            conflict_count: 0,
            task_count: 0,
            calendar_count: 0,
            synced_at: Utc::now().to_rfc3339(),
        },
    };
    record_feishu_run(&connection, &run_id, &trigger, started_at, &final_result)?;
    if final_result.status == "failed" {
        return Err(final_result.message);
    }
    Ok(final_result)
}

fn rebuild_feishu_tasklists_from_local_blocking(
    database_path: PathBuf,
    app_data_dir: PathBuf,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuRebuildResult, String> {
    let _sync_guard = match FEISHU_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Err("已有飞书同步正在执行，请稍后再重建。".to_string());
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err("飞书同步锁状态异常，请重启应用后再试。".to_string());
        }
    };
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let backup_path = create_feishu_local_database_backup(&database_path, &app_data_dir)?;
    let mut connection = open_database(&database_path)?;
    let result = rebuild_feishu_tasklists_inner(&mut connection, &app_data_dir);

    let final_result = match result {
        Ok(value) => value,
        Err(error) => {
            let failed = FeishuSyncResult {
                status: "failed".to_string(),
                message: error.clone(),
                pushed_count: 0,
                pulled_count: 0,
                deleted_count: 0,
                conflict_count: 0,
                task_count: 0,
                calendar_count: 0,
                synced_at: Utc::now().to_rfc3339(),
            };
            let _ = record_feishu_run(&connection, &run_id, "rebuild_tasks", started_at, &failed);
            return Err(error);
        }
    };

    let sync_result = FeishuSyncResult {
        status: final_result.status.clone(),
        message: format!(
            "{} 本地备份：{}；飞书备份：{}",
            final_result.message,
            backup_path.to_string_lossy(),
            final_result.remote_backup_path
        ),
        pushed_count: final_result.uploaded_task_count,
        pulled_count: 0,
        deleted_count: final_result.deleted_tasklist_count,
        conflict_count: 0,
        task_count: final_result.uploaded_task_count,
        calendar_count: 0,
        synced_at: final_result.synced_at.clone(),
    };
    record_feishu_run(
        &connection,
        &run_id,
        "rebuild_tasks",
        started_at,
        &sync_result,
    )?;

    Ok(FeishuRebuildResult {
        backup_path: backup_path.to_string_lossy().to_string(),
        ..final_result
    })
}

fn rebuild_feishu_tasklists_inner(
    connection: &mut Connection,
    app_data_dir: &Path,
) -> Result<FeishuRebuildResult, String> {
    let settings = read_feishu_settings(connection)?;
    if settings.app_id.is_empty() || settings.app_secret.is_empty() {
        return Err("未配置飞书 App ID 或 App Secret。".to_string());
    }
    let token = ensure_access_token(connection)?;
    let feishu = FeishuClient::new(token.access_token)?;
    let remote_tasklists =
        feishu.get_paged("/open-apis/task/v2/tasklists?page_size=100&user_id_type=open_id")?;
    let backup_path = export_feishu_tasklists_backup(&feishu, app_data_dir, &remote_tasklists)?;
    let app_tasklists = remote_tasklists
        .iter()
        .filter(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .map(is_app_tasklist_name)
                .unwrap_or(false)
        })
        .filter_map(|item| {
            item.get("guid")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();

    let mut deleted_tasklist_count = 0;
    for tasklist_guid in app_tasklists {
        feishu.delete(&format!(
            "/open-apis/task/v2/tasklists/{}?user_id_type=open_id",
            encode_path_segment(&tasklist_guid)
        ))?;
        deleted_tasklist_count += 1;
    }

    clear_feishu_tasklist_settings(connection)?;
    connection
        .execute(
            "DELETE FROM feishu_sync_links WHERE remote_kind = ?1",
            params![REMOTE_FEISHU_TASK],
        )
        .map_err(|error| error.to_string())?;

    let tasklists = create_fresh_feishu_tasklists(connection, &feishu)?;
    let mut counters = SyncCounters {
        pushed_count: 0,
        pulled_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        task_count: 0,
        calendar_count: 0,
    };
    for task in load_local_tasks(connection)?
        .into_iter()
        .filter(|task| task.deleted_at.is_none())
    {
        if task.title.trim().is_empty() {
            continue;
        }
        let tasklist_guid = tasklists
            .get(task.tasklist_key)
            .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
            .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
        replace_remote_task_in_tasklist(
            connection,
            &feishu,
            tasklist_guid,
            &task,
            None,
            &mut counters,
        )?;
    }

    Ok(FeishuRebuildResult {
        status: "rebuilt".to_string(),
        message: format!(
            "飞书任务清单已按本地数据重建：删除旧清单 {} 个，上传任务 {} 条。",
            deleted_tasklist_count, counters.pushed_count
        ),
        backup_path: String::new(),
        remote_backup_path: backup_path.to_string_lossy().to_string(),
        deleted_tasklist_count,
        uploaded_task_count: counters.pushed_count,
        tasklist_count: tasklists.len() as i64,
        synced_at: Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn list_feishu_sync_runs(
    app: AppHandle,
    limit: Option<i64>,
) -> Result<Vec<FeishuSyncRunSummary>, String> {
    let connection = open_database(&database_path(&app)?)?;
    list_feishu_runs(&connection, limit.unwrap_or(5).clamp(1, 50))
}

struct FeishuClient {
    client: Client,
    token: String,
}

impl FeishuClient {
    fn new(token: String) -> Result<Self, String> {
        Ok(Self {
            client: http_client()?,
            token,
        })
    }

    fn get_paged(&self, path_or_url: &str) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut next_url = Some(feishu_url(path_or_url));
        while let Some(url) = next_url {
            let data = self
                .client
                .get(&url)
                .headers(self.auth_headers()?)
                .send()
                .map_err(|error| format!("飞书分页读取失败：{error}"))
                .and_then(parse_feishu_response)?;
            if let Some(values) = data.get("items").and_then(Value::as_array) {
                items.extend(values.iter().cloned());
            }
            let has_more = data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let token = data
                .get("page_token")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            next_url = if has_more {
                token.map(|value| append_query_param(&url, "page_token", value))
            } else {
                None
            };
        }
        Ok(items)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.client
            .post(feishu_url(path))
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .map_err(|error| format!("飞书 POST 失败：{error}"))
            .and_then(parse_feishu_response)
    }

    fn patch(&self, path: &str, body: Value) -> Result<Value, String> {
        self.client
            .patch(feishu_url(path))
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .map_err(|error| format!("飞书 PATCH 失败：{error}"))
            .and_then(parse_feishu_response)
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        let response = self
            .client
            .delete(feishu_url(path))
            .headers(self.auth_headers()?)
            .send()
            .map_err(|error| format!("飞书 DELETE 失败：{error}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        parse_feishu_response(response).map(|_| ())
    }

    fn auth_headers(&self) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|error| error.to_string())?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }
}

fn ensure_feishu_containers(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<FeishuContainers, String> {
    let tasklists = ensure_feishu_tasklists(connection, feishu)?;
    let calendar_id = match non_empty_setting(connection, FEISHU_CALENDAR_ID_KEY)? {
        Some(value) => value,
        None => {
            let id = find_or_create_calendar(feishu)?;
            set_setting(
                connection,
                FEISHU_CALENDAR_ID_KEY,
                &id,
                &Utc::now().to_rfc3339(),
            )?;
            id
        }
    };
    Ok(FeishuContainers {
        tasklists,
        calendar_id,
    })
}

fn ensure_feishu_tasklists(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<HashMap<String, String>, String> {
    let existing =
        feishu.get_paged("/open-apis/task/v2/tasklists?page_size=100&user_id_type=open_id")?;
    let legacy_tasklist_guid = non_empty_setting(connection, FEISHU_LEGACY_TASKLIST_GUID_KEY)?
        .or(non_empty_setting(connection, FEISHU_TASKLIST_GUID_KEY)?)
        .or_else(|| find_tasklist_guid(&existing, BRIDGE_CONTAINER_NAME));
    let mut tasklists = HashMap::new();
    for key in TASKLIST_KEYS {
        let setting_key = feishu_tasklist_setting_key(key);
        let guid = match non_empty_setting(connection, &setting_key)? {
            Some(value) => value,
            None => {
                let id = find_or_create_tasklist(feishu, &existing, tasklist_title_for_key(key))?;
                let now = Utc::now().to_rfc3339();
                set_setting(connection, &setting_key, &id, &now)?;
                if key == TASKLIST_KEY_GENERAL {
                    if let Some(legacy_id) = legacy_tasklist_guid
                        .as_deref()
                        .filter(|legacy_id| *legacy_id != id)
                    {
                        set_setting(connection, FEISHU_LEGACY_TASKLIST_GUID_KEY, legacy_id, &now)?;
                    }
                    set_setting(connection, FEISHU_TASKLIST_GUID_KEY, &id, &now)?;
                }
                id
            }
        };
        tasklists.insert(key.to_string(), guid);
    }
    Ok(tasklists)
}

fn create_fresh_feishu_tasklists(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<HashMap<String, String>, String> {
    let mut tasklists = HashMap::new();
    let now = Utc::now().to_rfc3339();
    for key in TASKLIST_KEYS {
        let data = feishu.post(
            "/open-apis/task/v2/tasklists?user_id_type=open_id",
            json!({ "name": tasklist_title_for_key(key) }),
        )?;
        let id = data
            .get("tasklist")
            .and_then(|item| item.get("guid"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| "创建飞书任务清单后未返回 guid。".to_string())?;
        set_setting(connection, &feishu_tasklist_setting_key(key), &id, &now)?;
        if key == TASKLIST_KEY_GENERAL {
            set_setting(connection, FEISHU_TASKLIST_GUID_KEY, &id, &now)?;
        }
        tasklists.insert(key.to_string(), id);
    }
    Ok(tasklists)
}

fn find_or_create_tasklist(
    feishu: &FeishuClient,
    existing: &[Value],
    title: &str,
) -> Result<String, String> {
    if let Some(id) = find_tasklist_guid(existing, title) {
        return Ok(id);
    }
    let data = feishu.post(
        "/open-apis/task/v2/tasklists?user_id_type=open_id",
        json!({ "name": title }),
    )?;
    data.get("tasklist")
        .and_then(|item| item.get("guid"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "创建飞书任务清单后未返回 guid。".to_string())
}

fn find_tasklist_guid(existing: &[Value], title: &str) -> Option<String> {
    existing.iter().find_map(|item| {
        if item.get("name").and_then(Value::as_str) == Some(title) {
            item.get("guid")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        } else {
            None
        }
    })
}

fn find_or_create_calendar(feishu: &FeishuClient) -> Result<String, String> {
    for item in feishu.get_paged("/open-apis/calendar/v4/calendars?page_size=100")? {
        if item.get("summary").and_then(Value::as_str) == Some(BRIDGE_CONTAINER_NAME) {
            if let Some(id) = item.get("calendar_id").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
        }
    }
    let data = feishu.post(
        "/open-apis/calendar/v4/calendars",
        json!({
            "summary": BRIDGE_CONTAINER_NAME,
            "description": "kaoyan-focus bridge calendar"
        }),
    )?;
    data.get("calendar")
        .or_else(|| data.get("calendar_info"))
        .and_then(|item| item.get("calendar_id").or_else(|| item.get("id")))
        .and_then(Value::as_str)
        .or_else(|| data.get("calendar_id").and_then(Value::as_str))
        .map(ToString::to_string)
        .ok_or_else(|| "创建飞书日历后未返回 calendar_id。".to_string())
}

fn feishu_tasklist_setting_key(key: &str) -> String {
    format!("feishu_tasklist_guid_{key}")
}

fn tasklist_title_for_key(key: &str) -> &'static str {
    match key {
        TASKLIST_KEY_POLITICS => "考研专注 - 政治",
        TASKLIST_KEY_ENGLISH => "考研专注 - 英语",
        TASKLIST_KEY_MATH => "考研专注 - 数学",
        TASKLIST_KEY_MAJOR => "考研专注 - 专业课",
        TASKLIST_KEY_TODAY => "考研专注 - 今日任务",
        _ => "考研专注 - 通用",
    }
}

fn tasklist_key_for_board_scope(board_scope: &str) -> &'static str {
    match board_scope {
        "checklist:politics" => TASKLIST_KEY_POLITICS,
        "checklist:english" => TASKLIST_KEY_ENGLISH,
        "checklist:math" => TASKLIST_KEY_MATH,
        "checklist:major" => TASKLIST_KEY_MAJOR,
        _ => TASKLIST_KEY_GENERAL,
    }
}

fn sync_tasks(
    connection: &mut Connection,
    feishu: &FeishuClient,
    tasklists: &HashMap<String, String>,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let mut remote_tasks = Vec::new();
    for (key, tasklist_guid) in tasklists {
        fetch_remote_tasks_for_tasklist(feishu, key, tasklist_guid, &mut remote_tasks)?;
    }
    counters.task_count = remote_tasks.len() as i64;
    let remote_by_id = remote_tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect::<HashMap<_, _>>();
    let today_date = today_date_string();
    let stale_today_remote_ids =
        if let Some(today_tasklist_guid) = tasklists.get(TASKLIST_KEY_TODAY) {
            prune_stale_today_task_links(
                connection,
                feishu,
                today_tasklist_guid,
                &today_date,
                counters,
            )?
        } else {
            Vec::new()
        };

    for task in load_local_tasks(connection)? {
        if task.deleted_at.is_none() && task.title.trim().is_empty() {
            continue;
        }
        let tasklist_guid = tasklists
            .get(task.tasklist_key)
            .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
            .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
        if let Some(link) = get_link_by_sync_id(
            connection,
            task.entity_type,
            &task.sync_id,
            REMOTE_FEISHU_TASK,
        )? {
            if let Some(remote) = remote_by_id.get(&link.remote_id) {
                if task.deleted_at.is_some() {
                    feishu.delete(&format!(
                        "/open-apis/task/v2/tasks/{}",
                        encode_path_segment(&link.remote_id)
                    ))?;
                    mark_link_deleted(connection, link.id)?;
                    counters.deleted_count += 1;
                    continue;
                }
                let remote_in_current_tasklists =
                    tasklists.values().any(|guid| guid == &remote.tasklist_guid);
                sync_linked_task(
                    connection,
                    feishu,
                    &remote.tasklist_guid,
                    &task,
                    remote,
                    &link,
                    remote_in_current_tasklists,
                    counters,
                )?;
                let local_after_sync =
                    load_local_task_by_id(connection, task.entity_type, task.id).unwrap_or(task);
                let desired_tasklist_guid = tasklists
                    .get(local_after_sync.tasklist_key)
                    .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
                    .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
                if remote.tasklist_guid != *desired_tasklist_guid {
                    replace_remote_task_in_tasklist(
                        connection,
                        feishu,
                        desired_tasklist_guid,
                        &local_after_sync,
                        Some(remote.id.as_str()),
                        counters,
                    )?;
                }
            } else if task.deleted_at.is_none() {
                if link_points_to_current_tasklist(&link, tasklists) {
                    delete_local_task(connection, task.entity_type, task.id)?;
                    mark_link_deleted(connection, link.id)?;
                    counters.deleted_count += 1;
                } else {
                    replace_remote_task_in_tasklist(
                        connection,
                        feishu,
                        tasklist_guid,
                        &task,
                        Some(link.remote_id.as_str()),
                        counters,
                    )?;
                }
            } else {
                feishu.delete(&format!(
                    "/open-apis/task/v2/tasks/{}",
                    encode_path_segment(&link.remote_id)
                ))?;
                mark_link_deleted(connection, link.id)?;
                counters.deleted_count += 1;
            }
        } else if task.deleted_at.is_none() {
            replace_remote_task_in_tasklist(
                connection,
                feishu,
                tasklist_guid,
                &task,
                None,
                counters,
            )?;
        }
    }

    for remote in remote_tasks {
        if stale_today_remote_ids.iter().any(|id| id == &remote.id) {
            continue;
        }
        if get_link_by_remote_id(connection, REMOTE_FEISHU_TASK, &remote.id)?.is_some() {
            continue;
        }
        if let (Some(entity_type), Some(sync_id)) = (
            remote.marker_entity_type.as_deref(),
            remote.marker_sync_id.as_deref(),
        ) {
            let entity_type = match entity_type {
                ENTITY_CHECKLIST_TASK => Some(ENTITY_CHECKLIST_TASK),
                ENTITY_TODAY_PLAN_ITEM => Some(ENTITY_TODAY_PLAN_ITEM),
                _ => None,
            };
            if let Some(entity_type) = entity_type {
                if let Some((local_id, deleted_at)) =
                    get_sync_meta_local_by_sync_id(connection, entity_type, sync_id)?
                {
                    if entity_type == ENTITY_TODAY_PLAN_ITEM
                        && remote.tasklist_key == TASKLIST_KEY_TODAY
                        && is_stale_today_plan_item(connection, local_id, &today_date)?
                    {
                        delete_remote_task_if_present(feishu, &remote.id)?;
                        counters.deleted_count += 1;
                        continue;
                    }
                    upsert_link(
                        connection,
                        entity_type,
                        Some(local_id),
                        sync_id,
                        REMOTE_FEISHU_TASK,
                        &remote.id,
                        Some(&remote.tasklist_guid),
                        None,
                        None,
                        remote
                            .updated_millis
                            .map(|value| value.to_string())
                            .as_deref(),
                    )?;
                    if deleted_at.is_none() {
                        sync_linked_task(
                            connection,
                            feishu,
                            &remote.tasklist_guid,
                            &load_local_task_by_id(connection, entity_type, local_id)?,
                            &remote,
                            &get_link_by_sync_id(
                                connection,
                                entity_type,
                                sync_id,
                                REMOTE_FEISHU_TASK,
                            )?
                            .ok_or_else(|| "飞书任务链接写入后读取失败。".to_string())?,
                            tasklists.values().any(|guid| guid == &remote.tasklist_guid),
                            counters,
                        )?;
                    }
                    continue;
                }
            }
        }
        create_local_task_from_remote(connection, &remote)?;
        counters.pulled_count += 1;
    }
    Ok(())
}

fn fetch_remote_tasks_for_tasklist(
    feishu: &FeishuClient,
    tasklist_key: &str,
    tasklist_guid: &str,
    remote_tasks: &mut Vec<RemoteTask>,
) -> Result<(), String> {
    let remote_values = feishu.get_paged(&format!(
        "/open-apis/task/v2/tasklists/{}/tasks?page_size=100&user_id_type=open_id",
        encode_path_segment(tasklist_guid)
    ))?;
    remote_tasks.extend(
        remote_values
            .iter()
            .filter_map(|value| parse_remote_task(value, tasklist_key, tasklist_guid)),
    );
    Ok(())
}

fn prune_stale_today_task_links(
    connection: &Connection,
    feishu: &FeishuClient,
    today_tasklist_guid: &str,
    today_date: &str,
    counters: &mut SyncCounters,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT l.id, l.remote_id
            FROM feishu_sync_links l
            INNER JOIN today_plan_items t ON t.id = l.local_id
            WHERE l.entity_type = ?1
              AND l.remote_kind = ?2
              AND l.deleted_at IS NULL
              AND (l.remote_parent_id = ?3 OR l.remote_parent_id IS NULL)
              AND t.today_date <> ?4
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                ENTITY_TODAY_PLAN_ITEM,
                REMOTE_FEISHU_TASK,
                today_tasklist_guid,
                today_date
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let stale_links = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut remote_ids = Vec::new();
    for (link_id, remote_id) in stale_links {
        delete_remote_task_if_present(feishu, &remote_id)?;
        mark_link_deleted(connection, link_id)?;
        counters.deleted_count += 1;
        remote_ids.push(remote_id);
    }
    Ok(remote_ids)
}

fn is_stale_today_plan_item(
    connection: &Connection,
    local_id: i64,
    today_date: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT today_date <> ?1 FROM today_plan_items WHERE id = ?2",
            params![today_date, local_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| error.to_string())
}

fn delete_remote_task_if_present(feishu: &FeishuClient, remote_id: &str) -> Result<(), String> {
    feishu.delete(&format!(
        "/open-apis/task/v2/tasks/{}",
        encode_path_segment(remote_id)
    ))
}

fn link_points_to_current_tasklist(link: &FeishuLink, tasklists: &HashMap<String, String>) -> bool {
    link.remote_parent_id
        .as_deref()
        .map(|parent_id| tasklists.values().any(|guid| guid == parent_id))
        .unwrap_or(false)
}

fn replace_remote_task_in_tasklist(
    connection: &Connection,
    feishu: &FeishuClient,
    tasklist_guid: &str,
    task: &LocalTask,
    previous_remote_id: Option<&str>,
    counters: &mut SyncCounters,
) -> Result<RemoteTask, String> {
    let data = feishu.post(
        "/open-apis/task/v2/tasks?user_id_type=open_id",
        feishu_task_body(task, tasklist_guid),
    )?;
    let remote = data
        .get("task")
        .and_then(|value| parse_remote_task(value, task.tasklist_key, tasklist_guid))
        .or_else(|| parse_remote_task(&data, task.tasklist_key, tasklist_guid))
        .ok_or_else(|| "飞书创建任务后未返回任务信息。".to_string())?;
    upsert_link(
        connection,
        task.entity_type,
        Some(task.id),
        &task.sync_id,
        REMOTE_FEISHU_TASK,
        &remote.id,
        Some(tasklist_guid),
        None,
        None,
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )?;
    if let Some(previous_remote_id) = previous_remote_id.filter(|id| *id != remote.id) {
        feishu.delete(&format!(
            "/open-apis/task/v2/tasks/{}",
            encode_path_segment(previous_remote_id)
        ))?;
    }
    counters.pushed_count += 1;
    counters.task_count += 1;
    Ok(remote)
}

fn sync_linked_task(
    connection: &Connection,
    feishu: &FeishuClient,
    tasklist_guid: &str,
    local: &LocalTask,
    remote: &RemoteTask,
    link: &FeishuLink,
    apply_remote_tasklist: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let local_updated = parse_rfc3339_millis(&local.updated_at)?;
    let remote_updated = remote.updated_millis.unwrap_or(local_updated);
    if local_updated > remote_updated + 1_000 {
        let data = feishu.patch(
            &format!(
                "/open-apis/task/v2/tasks/{}?user_id_type=open_id",
                encode_path_segment(&remote.id)
            ),
            feishu_task_patch_body(local),
        )?;
        let next_remote = data
            .get("task")
            .and_then(|value| parse_remote_task(value, local.tasklist_key, tasklist_guid))
            .or_else(|| parse_remote_task(&data, local.tasklist_key, tasklist_guid))
            .unwrap_or_else(|| remote.clone());
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            REMOTE_FEISHU_TASK,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            next_remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        counters.pushed_count += 1;
    } else if remote_updated > local_updated + 1_000 {
        update_local_task_from_remote(
            connection,
            local.entity_type,
            local.id,
            remote,
            apply_remote_tasklist,
        )?;
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            REMOTE_FEISHU_TASK,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        counters.pulled_count += 1;
    } else {
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            &link.remote_kind,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
    }
    Ok(())
}

fn feishu_tasklist_statuses(connection: &Connection) -> Result<Vec<FeishuTasklistStatus>, String> {
    let mut statuses = Vec::new();
    for key in TASKLIST_KEYS {
        let guid = non_empty_setting(connection, &feishu_tasklist_setting_key(key))?;
        statuses.push(FeishuTasklistStatus {
            key: key.to_string(),
            label: tasklist_label_for_key(key).to_string(),
            ready: guid.is_some(),
            guid,
        });
    }
    Ok(statuses)
}

fn tasklist_label_for_key(key: &str) -> &'static str {
    match key {
        TASKLIST_KEY_POLITICS => "政治",
        TASKLIST_KEY_ENGLISH => "英语",
        TASKLIST_KEY_MATH => "数学",
        TASKLIST_KEY_MAJOR => "专业课",
        TASKLIST_KEY_TODAY => "今日任务",
        _ => "通用",
    }
}

fn sync_calendar_events(
    connection: &mut Connection,
    feishu: &FeishuClient,
    calendar_id: &str,
    prefer_local_changes: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    prune_orphan_calendar_event_links(connection, feishu, calendar_id, counters)?;

    let (range_start, range_end) = calendar_sync_range();
    let remote_values = feishu.get_paged(&format!(
        "/open-apis/calendar/v4/calendars/{}/events?start_time={}&end_time={}&page_size=100",
        encode_path_segment(calendar_id),
        range_start,
        range_end,
    ))?;
    let remote_events = remote_values
        .iter()
        .filter_map(parse_remote_event)
        .collect::<Vec<_>>();
    counters.calendar_count = remote_events.len() as i64;
    let remote_by_id = remote_events
        .iter()
        .map(|event| (event.id.clone(), event.clone()))
        .collect::<HashMap<_, _>>();

    for block in load_local_schedule_blocks(connection)? {
        if let Some(link) = get_link_by_sync_id(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            &block.sync_id,
            REMOTE_FEISHU_EVENT,
        )? {
            if let Some(remote) = remote_by_id.get(&link.remote_id) {
                if block.deleted_at.is_some() {
                    delete_remote_calendar_event_if_present(feishu, calendar_id, &link.remote_id)?;
                    mark_link_deleted(connection, link.id)?;
                    counters.deleted_count += 1;
                    continue;
                }
                sync_linked_event(
                    connection,
                    feishu,
                    calendar_id,
                    &block,
                    remote,
                    &link,
                    prefer_local_changes,
                    counters,
                )?;
            } else if block.deleted_at.is_none() && date_in_sync_range(&block.schedule_date) {
                delete_local_schedule_block(connection, block.id)?;
                mark_link_deleted(connection, link.id)?;
                counters.deleted_count += 1;
            } else if block.deleted_at.is_some() {
                mark_link_deleted(connection, link.id)?;
            }
        } else if block.deleted_at.is_none() && date_in_sync_range(&block.schedule_date) {
            let data = feishu.post(
                &format!(
                    "/open-apis/calendar/v4/calendars/{}/events",
                    encode_path_segment(calendar_id)
                ),
                feishu_event_body(&block),
            )?;
            let remote = data
                .get("event")
                .or_else(|| data.get("calendar_event"))
                .and_then(parse_remote_event)
                .or_else(|| parse_remote_event(&data))
                .ok_or_else(|| "飞书创建日程后未返回日程信息。".to_string())?;
            upsert_link(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                Some(block.id),
                &block.sync_id,
                REMOTE_FEISHU_EVENT,
                &remote.id,
                Some(calendar_id),
                Some(&local_schedule_block_fingerprint(&block)),
                Some(&remote_event_fingerprint(&remote)),
                remote
                    .updated_millis
                    .map(|value| value.to_string())
                    .as_deref(),
            )?;
            counters.pushed_count += 1;
            counters.calendar_count += 1;
        }
    }

    for remote in remote_events {
        if get_link_by_remote_id(connection, REMOTE_FEISHU_EVENT, &remote.id)?.is_some() {
            continue;
        }
        if let Some(sync_id) = remote.marker_sync_id.as_deref() {
            if let Some((local_id, deleted_at)) =
                get_sync_meta_local_by_sync_id(connection, ENTITY_SCHEDULE_BLOCK, sync_id)?
            {
                let local_block = if deleted_at.is_none() {
                    Some(load_local_schedule_block_by_id(connection, local_id)?)
                } else {
                    None
                };
                let local_fingerprint = local_block
                    .as_ref()
                    .map(local_schedule_block_fingerprint)
                    .unwrap_or_else(|| remote_event_fingerprint(&remote));
                upsert_link(
                    connection,
                    ENTITY_SCHEDULE_BLOCK,
                    Some(local_id),
                    sync_id,
                    REMOTE_FEISHU_EVENT,
                    &remote.id,
                    Some(calendar_id),
                    Some(&local_fingerprint),
                    Some(&remote_event_fingerprint(&remote)),
                    remote
                        .updated_millis
                        .map(|value| value.to_string())
                        .as_deref(),
                )?;
                if let Some(local_block) = local_block.as_ref() {
                    sync_linked_event(
                        connection,
                        feishu,
                        calendar_id,
                        local_block,
                        &remote,
                        &get_link_by_sync_id(
                            connection,
                            ENTITY_SCHEDULE_BLOCK,
                            sync_id,
                            REMOTE_FEISHU_EVENT,
                        )?
                        .ok_or_else(|| "飞书日程链接写入后读取失败。".to_string())?,
                        prefer_local_changes,
                        counters,
                    )?;
                }
                continue;
            }
        }
        if is_importable_remote_event(&remote) {
            create_local_schedule_block_from_remote(connection, &remote)?;
            counters.pulled_count += 1;
        }
    }
    Ok(())
}

fn prune_orphan_calendar_event_links(
    connection: &Connection,
    feishu: &FeishuClient,
    calendar_id: &str,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    for link in load_orphan_calendar_event_links(connection)? {
        delete_remote_calendar_event_if_present(feishu, calendar_id, &link.remote_id)?;
        mark_link_deleted(connection, link.id)?;
        counters.deleted_count += 1;
    }
    Ok(())
}

fn load_orphan_calendar_event_links(connection: &Connection) -> Result<Vec<FeishuLink>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT l.id, l.entity_type, l.local_id, l.local_sync_id, l.remote_kind, l.remote_id,
                   l.remote_parent_id, l.remote_etag, l.remote_change_key,
                   l.remote_last_modified
            FROM feishu_sync_links l
            LEFT JOIN schedule_blocks b ON b.id = l.local_id
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = l.local_id
            WHERE l.entity_type = ?1
              AND l.remote_kind = ?2
              AND l.deleted_at IS NULL
              AND (b.id IS NULL OR m.deleted_at IS NOT NULL)
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![ENTITY_SCHEDULE_BLOCK, REMOTE_FEISHU_EVENT],
            row_to_link,
        )
        .map_err(|error| error.to_string())?;
    let mut links = Vec::new();
    for row in rows {
        links.push(row.map_err(|error| error.to_string())?);
    }
    Ok(links)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkedCalendarAction {
    PushLocal,
    PullRemote,
    RefreshLink,
}

fn linked_calendar_action(
    local_updated: i64,
    remote_updated: Option<i64>,
    local_changed_since_sync: bool,
    remote_changed_since_sync: bool,
    prefer_local_changes: bool,
) -> LinkedCalendarAction {
    const SKEW_MILLIS: i64 = 1_000;
    if let Some(remote_updated) = remote_updated {
        if local_updated > remote_updated + SKEW_MILLIS {
            return LinkedCalendarAction::PushLocal;
        }
        if remote_updated > local_updated + SKEW_MILLIS {
            return LinkedCalendarAction::PullRemote;
        }
        return LinkedCalendarAction::RefreshLink;
    }

    if !local_changed_since_sync && !remote_changed_since_sync {
        return LinkedCalendarAction::RefreshLink;
    }
    if local_changed_since_sync && !remote_changed_since_sync {
        LinkedCalendarAction::PushLocal
    } else if remote_changed_since_sync && !local_changed_since_sync {
        LinkedCalendarAction::PullRemote
    } else if prefer_local_changes {
        LinkedCalendarAction::PushLocal
    } else {
        LinkedCalendarAction::PullRemote
    }
}

fn calendar_event_content_differs(local: &LocalScheduleBlock, remote: &RemoteEvent) -> bool {
    local_schedule_block_fingerprint(local) != remote_event_fingerprint(remote)
}

fn local_schedule_block_fingerprint(block: &LocalScheduleBlock) -> String {
    calendar_fingerprint(
        &block.schedule_date,
        block.start_minute,
        block.end_minute,
        &block.title,
        block.note.as_deref(),
    )
}

fn remote_event_fingerprint(remote: &RemoteEvent) -> String {
    calendar_fingerprint(
        &remote.schedule_date,
        remote.start_minute,
        remote.end_minute,
        &remote.title,
        remote.note.as_deref(),
    )
}

fn calendar_fingerprint(
    schedule_date: &str,
    start_minute: i64,
    end_minute: i64,
    title: &str,
    note: Option<&str>,
) -> String {
    [
        schedule_date.trim().to_string(),
        start_minute.to_string(),
        end_minute.to_string(),
        title.trim().to_string(),
        normalize_note(note),
    ]
    .join("\u{1f}")
}

fn normalize_note(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

fn sync_linked_event(
    connection: &Connection,
    feishu: &FeishuClient,
    calendar_id: &str,
    local: &LocalScheduleBlock,
    remote: &RemoteEvent,
    link: &FeishuLink,
    prefer_local_changes: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let local_updated = parse_rfc3339_millis(&local.updated_at)?;
    let remote_updated = remote.updated_millis.or_else(|| {
        link.remote_last_modified
            .as_deref()
            .and_then(parse_link_millis)
    });
    let local_fingerprint = local_schedule_block_fingerprint(local);
    let remote_fingerprint = remote_event_fingerprint(remote);
    let local_changed_since_sync = link
        .remote_etag
        .as_deref()
        .map(|fingerprint| fingerprint != local_fingerprint)
        .unwrap_or_else(|| calendar_event_content_differs(local, remote));
    let remote_changed_since_sync = link
        .remote_change_key
        .as_deref()
        .map(|fingerprint| fingerprint != remote_fingerprint)
        .unwrap_or(false);

    match linked_calendar_action(
        local_updated,
        remote_updated,
        local_changed_since_sync,
        remote_changed_since_sync,
        prefer_local_changes,
    ) {
        LinkedCalendarAction::PushLocal => {
            let data = match feishu.patch(
                &format!(
                    "/open-apis/calendar/v4/calendars/{}/events/{}",
                    encode_path_segment(calendar_id),
                    encode_path_segment(&remote.id)
                ),
                feishu_event_body(local),
            ) {
                Ok(data) => data,
                Err(error) if is_feishu_deleted_event_error(&error) => {
                    mark_link_deleted(connection, link.id)?;
                    create_remote_calendar_event(connection, feishu, calendar_id, local)?;
                    counters.pushed_count += 1;
                    counters.calendar_count += 1;
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            let next_remote = data
                .get("event")
                .or_else(|| data.get("calendar_event"))
                .and_then(parse_remote_event)
                .or_else(|| parse_remote_event(&data))
                .unwrap_or_else(|| remote.clone());
            upsert_link(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                Some(local.id),
                &local.sync_id,
                REMOTE_FEISHU_EVENT,
                &remote.id,
                Some(calendar_id),
                Some(&local_schedule_block_fingerprint(local)),
                Some(&remote_event_fingerprint(&next_remote)),
                next_remote
                    .updated_millis
                    .map(|value| value.to_string())
                    .as_deref(),
            )?;
            counters.pushed_count += 1;
        }
        LinkedCalendarAction::PullRemote => {
            update_local_schedule_block_from_remote(connection, local.id, remote)?;
            upsert_link(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                Some(local.id),
                &local.sync_id,
                REMOTE_FEISHU_EVENT,
                &remote.id,
                Some(calendar_id),
                Some(&remote_event_fingerprint(remote)),
                Some(&remote_event_fingerprint(remote)),
                remote
                    .updated_millis
                    .map(|value| value.to_string())
                    .as_deref(),
            )?;
            counters.pulled_count += 1;
        }
        LinkedCalendarAction::RefreshLink => {
            upsert_link(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                Some(local.id),
                &local.sync_id,
                &link.remote_kind,
                &remote.id,
                link.remote_parent_id.as_deref(),
                Some(&local_schedule_block_fingerprint(local)),
                Some(&remote_event_fingerprint(remote)),
                remote
                    .updated_millis
                    .map(|value| value.to_string())
                    .as_deref(),
            )?;
        }
    }
    Ok(())
}

fn create_remote_calendar_event(
    connection: &Connection,
    feishu: &FeishuClient,
    calendar_id: &str,
    block: &LocalScheduleBlock,
) -> Result<RemoteEvent, String> {
    let data = feishu.post(
        &format!(
            "/open-apis/calendar/v4/calendars/{}/events",
            encode_path_segment(calendar_id)
        ),
        feishu_event_body(block),
    )?;
    let remote = data
        .get("event")
        .or_else(|| data.get("calendar_event"))
        .and_then(parse_remote_event)
        .or_else(|| parse_remote_event(&data))
        .ok_or_else(|| "Feishu did not return created calendar event".to_string())?;
    upsert_link(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        Some(block.id),
        &block.sync_id,
        REMOTE_FEISHU_EVENT,
        &remote.id,
        Some(calendar_id),
        Some(&local_schedule_block_fingerprint(block)),
        Some(&remote_event_fingerprint(&remote)),
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )?;
    Ok(remote)
}

fn delete_remote_calendar_event_if_present(
    feishu: &FeishuClient,
    calendar_id: &str,
    remote_id: &str,
) -> Result<(), String> {
    match feishu.delete(&format!(
        "/open-apis/calendar/v4/calendars/{}/events/{}",
        encode_path_segment(calendar_id),
        encode_path_segment(remote_id)
    )) {
        Ok(()) => Ok(()),
        Err(error) if is_feishu_deleted_event_error(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn feishu_task_body(task: &LocalTask, tasklist_guid: &str) -> Value {
    let mut task_body = json!({
        "summary": task.title,
        "description": body_with_marker(task.note.as_deref(), task.entity_type, &task.sync_id),
        "extra": marker_json(task.entity_type, &task.sync_id),
        "completed_at": if task.completed {
            parse_rfc3339_millis(&task.updated_at).unwrap_or_else(|_| Utc::now().timestamp_millis()).to_string()
        } else {
            "0".to_string()
        },
        "tasklists": [{ "tasklist_guid": tasklist_guid }]
    });
    if let Some(due_date) = task.due_date.as_deref().filter(|value| !value.is_empty()) {
        task_body["due"] = json!({
            "timestamp": due_date_to_millis(due_date).to_string(),
            "is_all_day": true
        });
    }
    task_body
}

fn feishu_task_patch_body(task: &LocalTask) -> Value {
    let mut task_body = json!({
        "summary": task.title,
        "description": body_with_marker(task.note.as_deref(), task.entity_type, &task.sync_id),
        "extra": marker_json(task.entity_type, &task.sync_id),
        "completed_at": if task.completed {
            parse_rfc3339_millis(&task.updated_at).unwrap_or_else(|_| Utc::now().timestamp_millis()).to_string()
        } else {
            "0".to_string()
        }
    });
    let mut update_fields = vec!["summary", "description", "extra", "completed_at"];
    if let Some(due_date) = task.due_date.as_deref().filter(|value| !value.is_empty()) {
        task_body["due"] = json!({
            "timestamp": due_date_to_millis(due_date).to_string(),
            "is_all_day": true
        });
        update_fields.push("due");
    }
    json!({
        "task": task_body,
        "update_fields": update_fields
    })
}

fn feishu_event_body(block: &LocalScheduleBlock) -> Value {
    json!({
        "summary": block.title,
        "description": body_with_marker(block.note.as_deref(), ENTITY_SCHEDULE_BLOCK, &block.sync_id),
        "start_time": {
            "timestamp": minute_to_timestamp(&block.schedule_date, block.start_minute).to_string(),
            "timezone": TIME_ZONE
        },
        "end_time": {
            "timestamp": minute_to_timestamp(&block.schedule_date, block.end_minute).to_string(),
            "timezone": TIME_ZONE
        },
        "visibility": "default",
        "free_busy_status": if block.status == "completed" { "free" } else { "busy" }
    })
}

fn parse_remote_task(value: &Value, tasklist_key: &str, tasklist_guid: &str) -> Option<RemoteTask> {
    let id = value.get("guid")?.as_str()?.to_string();
    let title = value
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or("")
        .to_string();
    let raw_note = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let marker = extract_marker(raw_note).or_else(|| {
        value
            .get("extra")
            .and_then(Value::as_str)
            .and_then(extract_marker)
    });
    let note = Some(strip_marker(raw_note)).filter(|item| !item.trim().is_empty());
    let due_date = value
        .get("due")
        .and_then(|due| due.get("timestamp"))
        .and_then(value_to_i64)
        .map(millis_to_local_date_string);
    let completed = value
        .get("completed_at")
        .and_then(value_to_i64)
        .unwrap_or(0)
        > 0
        || value.get("status").and_then(Value::as_str) == Some("completed");
    let updated_millis = value.get("updated_at").and_then(value_to_i64);
    Some(RemoteTask {
        id,
        tasklist_key: tasklist_key.to_string(),
        tasklist_guid: tasklist_guid.to_string(),
        title,
        note,
        due_date,
        completed,
        updated_millis,
        marker_entity_type: marker.as_ref().map(|item| item.0.clone()),
        marker_sync_id: marker.map(|item| item.1),
    })
}

fn parse_remote_event(value: &Value) -> Option<RemoteEvent> {
    let id = value
        .get("event_id")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    let title = value
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or("")
        .to_string();
    let raw_note = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let marker = extract_marker(raw_note);
    let note = Some(strip_marker(raw_note)).filter(|item| !item.trim().is_empty());
    let start_timestamp = value
        .get("start_time")
        .and_then(|item| item.get("timestamp"))
        .and_then(value_to_i64)?;
    let end_timestamp = value
        .get("end_time")
        .and_then(|item| item.get("timestamp"))
        .and_then(value_to_i64)?;
    let (schedule_date, start_minute) = timestamp_to_local_date_minute(start_timestamp)?;
    let (end_date, mut end_minute) = timestamp_to_local_date_minute(end_timestamp)?;
    if end_date != schedule_date {
        end_minute = 1440;
    }
    if end_minute <= start_minute {
        end_minute = (start_minute + 60).min(1440);
    }
    let updated_millis = value
        .get("updated_time")
        .or_else(|| value.get("updated_at"))
        .and_then(value_to_i64)
        .map(normalize_timestamp_millis);
    Some(RemoteEvent {
        id,
        title,
        note,
        schedule_date,
        start_minute,
        end_minute,
        updated_millis,
        marker_sync_id: marker.map(|item| item.1),
    })
}

fn load_local_tasks(connection: &Connection) -> Result<Vec<LocalTask>, String> {
    let mut tasks = Vec::new();
    {
        let mut statement = connection
            .prepare(
                "
                SELECT t.id, t.title, t.note, t.due_date, t.completed, t.created_at, t.updated_at,
                       t.board_scope,
                       m.sync_id, m.deleted_at
                FROM checklist_tasks t
                LEFT JOIN sync_meta m ON m.entity_type = 'checklist_task' AND m.local_id = t.id
                ORDER BY t.id ASC
                ",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                Ok(LocalTask {
                    entity_type: ENTITY_CHECKLIST_TASK,
                    id,
                    tasklist_key: tasklist_key_for_board_scope(row.get::<_, String>(7)?.as_str()),
                    title: row.get(1)?,
                    note: row.get(2)?,
                    due_date: row.get(3)?,
                    completed: row.get::<_, i64>(4)? != 0,
                    updated_at: row.get(6)?,
                    sync_id: row
                        .get::<_, Option<String>>(8)?
                        .unwrap_or_else(|| format!("{ENTITY_CHECKLIST_TASK}-{id}")),
                    deleted_at: row.get(9)?,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let task = row.map_err(|error| error.to_string())?;
            ensure_sync_meta(
                connection,
                task.entity_type,
                task.id,
                &task.sync_id,
                &task.updated_at,
                None,
            )?;
            tasks.push(task);
        }
    }
    {
        let today_date = today_date_string();
        let mut statement = connection
            .prepare(
                "
                SELECT t.id, t.title, t.note, t.due_date, t.completed, t.created_at, t.updated_at,
                       m.sync_id, m.deleted_at
                FROM today_plan_items t
                LEFT JOIN sync_meta m ON m.entity_type = 'today_plan_item' AND m.local_id = t.id
                WHERE t.today_date = ?1
                ORDER BY t.id ASC
                ",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![today_date], |row| {
                let id: i64 = row.get(0)?;
                Ok(LocalTask {
                    entity_type: ENTITY_TODAY_PLAN_ITEM,
                    id,
                    tasklist_key: TASKLIST_KEY_TODAY,
                    title: row.get(1)?,
                    note: row.get(2)?,
                    due_date: row.get(3)?,
                    completed: row.get::<_, i64>(4)? != 0,
                    updated_at: row.get(6)?,
                    sync_id: row
                        .get::<_, Option<String>>(7)?
                        .unwrap_or_else(|| format!("{ENTITY_TODAY_PLAN_ITEM}-{id}")),
                    deleted_at: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let task = row.map_err(|error| error.to_string())?;
            ensure_sync_meta(
                connection,
                task.entity_type,
                task.id,
                &task.sync_id,
                &task.updated_at,
                None,
            )?;
            tasks.push(task);
        }
    }
    tasks.extend(load_tombstone_tasks(connection, ENTITY_CHECKLIST_TASK)?);
    tasks.extend(load_tombstone_tasks(connection, ENTITY_TODAY_PLAN_ITEM)?);
    Ok(tasks)
}

fn load_tombstone_tasks(
    connection: &Connection,
    entity_type: &'static str,
) -> Result<Vec<LocalTask>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT local_id, sync_id, deleted_at, updated_at
            FROM sync_meta
            WHERE entity_type = ?1 AND deleted_at IS NOT NULL
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![entity_type], |row| {
            let local_id: i64 = row.get(0)?;
            let deleted_at: i64 = row.get(2)?;
            Ok(LocalTask {
                entity_type,
                id: local_id,
                sync_id: row.get(1)?,
                tasklist_key: if entity_type == ENTITY_TODAY_PLAN_ITEM {
                    TASKLIST_KEY_TODAY
                } else {
                    TASKLIST_KEY_GENERAL
                },
                title: String::new(),
                note: None,
                due_date: None,
                completed: false,
                updated_at: millis_to_rfc3339(row.get(3)?),
                deleted_at: Some(deleted_at),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_local_task_by_id(
    connection: &Connection,
    entity_type: &'static str,
    local_id: i64,
) -> Result<LocalTask, String> {
    if entity_type == ENTITY_CHECKLIST_TASK {
        return connection
            .query_row(
                "
                SELECT t.id, t.title, t.note, t.due_date, t.completed, t.updated_at,
                       t.board_scope, m.sync_id, m.deleted_at
                FROM checklist_tasks t
                LEFT JOIN sync_meta m ON m.entity_type = ?1 AND m.local_id = t.id
                WHERE t.id = ?2
                ",
                params![entity_type, local_id],
                |row| {
                    let id: i64 = row.get(0)?;
                    Ok(LocalTask {
                        entity_type,
                        id,
                        tasklist_key: tasklist_key_for_board_scope(
                            row.get::<_, String>(6)?.as_str(),
                        ),
                        title: row.get(1)?,
                        note: row.get(2)?,
                        due_date: row.get(3)?,
                        completed: row.get::<_, i64>(4)? != 0,
                        updated_at: row.get(5)?,
                        sync_id: row
                            .get::<_, Option<String>>(7)?
                            .unwrap_or_else(|| format!("{entity_type}-{id}")),
                        deleted_at: row.get(8)?,
                    })
                },
            )
            .map_err(|error| error.to_string());
    }

    connection
        .query_row(
            "
                SELECT t.id, t.title, t.note, t.due_date, t.completed, t.updated_at,
                       m.sync_id, m.deleted_at
                FROM today_plan_items t
                LEFT JOIN sync_meta m ON m.entity_type = ?1 AND m.local_id = t.id
                WHERE t.id = ?2
                ",
            params![entity_type, local_id],
            |row| {
                let id: i64 = row.get(0)?;
                Ok(LocalTask {
                    entity_type,
                    id,
                    tasklist_key: TASKLIST_KEY_TODAY,
                    title: row.get(1)?,
                    note: row.get(2)?,
                    due_date: row.get(3)?,
                    completed: row.get::<_, i64>(4)? != 0,
                    updated_at: row.get(5)?,
                    sync_id: row
                        .get::<_, Option<String>>(6)?
                        .unwrap_or_else(|| format!("{entity_type}-{id}")),
                    deleted_at: row.get(7)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

fn load_local_schedule_blocks(connection: &Connection) -> Result<Vec<LocalScheduleBlock>, String> {
    let mut blocks = Vec::new();
    let mut statement = connection
        .prepare(
            "
            SELECT b.id, b.schedule_date, b.title, b.note, b.start_minute, b.end_minute,
                   b.status, b.created_at, b.updated_at, m.sync_id, m.deleted_at
            FROM schedule_blocks b
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
            ORDER BY b.schedule_date ASC, b.start_minute ASC, b.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            Ok(LocalScheduleBlock {
                id,
                schedule_date: row.get(1)?,
                title: row.get(2)?,
                note: row.get(3)?,
                start_minute: row.get(4)?,
                end_minute: row.get(5)?,
                status: row.get(6)?,
                updated_at: row.get(8)?,
                sync_id: row
                    .get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| format!("{ENTITY_SCHEDULE_BLOCK}-{id}")),
                deleted_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let block = row.map_err(|error| error.to_string())?;
        ensure_sync_meta(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            block.id,
            &block.sync_id,
            &block.updated_at,
            None,
        )?;
        blocks.push(block);
    }
    blocks.extend(load_tombstone_blocks(connection)?);
    Ok(blocks)
}

fn load_local_schedule_block_by_id(
    connection: &Connection,
    local_id: i64,
) -> Result<LocalScheduleBlock, String> {
    connection
        .query_row(
            "
            SELECT b.id, b.schedule_date, b.title, b.note, b.start_minute, b.end_minute,
                   b.status, b.updated_at, m.sync_id, m.deleted_at
            FROM schedule_blocks b
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
            WHERE b.id = ?1
            ",
            params![local_id],
            |row| {
                let id: i64 = row.get(0)?;
                Ok(LocalScheduleBlock {
                    id,
                    schedule_date: row.get(1)?,
                    title: row.get(2)?,
                    note: row.get(3)?,
                    start_minute: row.get(4)?,
                    end_minute: row.get(5)?,
                    status: row.get(6)?,
                    updated_at: row.get(7)?,
                    sync_id: row
                        .get::<_, Option<String>>(8)?
                        .unwrap_or_else(|| format!("{ENTITY_SCHEDULE_BLOCK}-{id}")),
                    deleted_at: row.get(9)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

fn load_tombstone_blocks(connection: &Connection) -> Result<Vec<LocalScheduleBlock>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT local_id, sync_id, deleted_at, updated_at
            FROM sync_meta
            WHERE entity_type = 'schedule_block' AND deleted_at IS NOT NULL
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let local_id: i64 = row.get(0)?;
            let deleted_at: i64 = row.get(2)?;
            Ok(LocalScheduleBlock {
                id: local_id,
                sync_id: row.get(1)?,
                schedule_date: Local::now().date_naive().format("%Y-%m-%d").to_string(),
                title: String::new(),
                note: None,
                start_minute: 0,
                end_minute: 60,
                status: "planned".to_string(),
                updated_at: millis_to_rfc3339(row.get(3)?),
                deleted_at: Some(deleted_at),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn update_local_task_from_remote(
    connection: &Connection,
    entity_type: &str,
    id: i64,
    remote: &RemoteTask,
    update_group: bool,
) -> Result<(), String> {
    let updated_at = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let completed = if remote.completed { 1 } else { 0 };
    match entity_type {
        ENTITY_TODAY_PLAN_ITEM => connection.execute(
            "
            UPDATE today_plan_items
            SET title = ?1, note = ?2, due_date = ?3, completed = ?4, updated_at = ?5
            WHERE id = ?6
            ",
            params![remote.title, remote.note, remote.due_date, completed, updated_at, id],
        ),
        _ if update_group => {
            let board_scope = board_scope_for_tasklist_key(&remote.tasklist_key);
            ensure_checklist_bucket(connection, board_scope)?;
            connection.execute(
                "
                UPDATE checklist_tasks
                SET board_scope = ?1,
                    column_id = (SELECT id FROM checklist_columns WHERE board_scope = ?1 ORDER BY sort_order ASC, id ASC LIMIT 1),
                    title = ?2,
                    note = ?3,
                    due_date = ?4,
                    completed = ?5,
                    updated_at = ?6
                WHERE id = ?7
                ",
                params![
                    board_scope,
                    remote.title,
                    remote.note,
                    remote.due_date,
                    completed,
                    updated_at,
                    id
                ],
            )
        }
        _ => connection.execute(
            "
            UPDATE checklist_tasks
            SET title = ?1, note = ?2, due_date = ?3, completed = ?4, updated_at = ?5
            WHERE id = ?6
            ",
            params![remote.title, remote.note, remote.due_date, completed, updated_at, id],
        ),
    }
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn create_local_task_from_remote(
    connection: &Connection,
    remote: &RemoteTask,
) -> Result<(), String> {
    if remote.tasklist_key == TASKLIST_KEY_TODAY {
        return create_local_today_plan_item_from_remote(connection, remote);
    }
    create_local_checklist_task_from_remote(connection, remote)
}

fn create_local_today_plan_item_from_remote(
    connection: &Connection,
    remote: &RemoteTask,
) -> Result<(), String> {
    let today_date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let now = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let sort_order: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM today_plan_items WHERE today_date = ?1",
            params![today_date],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "
            INSERT INTO today_plan_items (
              today_date, source_task_id, subject_id, title, note, due_date, sort_order,
              completed, synced_source_completion, created_at, updated_at
            ) VALUES (?1, NULL, NULL, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)
            ",
            params![
                today_date,
                remote.title,
                remote.note,
                remote.due_date,
                sort_order,
                if remote.completed { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    let sync_id = format!("{ENTITY_TODAY_PLAN_ITEM}-{local_id}");
    ensure_sync_meta(
        connection,
        ENTITY_TODAY_PLAN_ITEM,
        local_id,
        &sync_id,
        &now,
        None,
    )?;
    upsert_link(
        connection,
        ENTITY_TODAY_PLAN_ITEM,
        Some(local_id),
        &sync_id,
        REMOTE_FEISHU_TASK,
        &remote.id,
        Some(&remote.tasklist_guid),
        None,
        None,
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )
}

fn create_local_checklist_task_from_remote(
    connection: &Connection,
    remote: &RemoteTask,
) -> Result<(), String> {
    let board_scope = board_scope_for_tasklist_key(&remote.tasklist_key);
    ensure_checklist_bucket(connection, board_scope)?;
    let now = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let sort_order: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM checklist_tasks WHERE board_scope = ?1",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "
            INSERT INTO checklist_tasks (
              board_scope, subject_id, column_id, title, note, due_date, sort_order,
              completed, created_at, updated_at
            ) VALUES (?1, NULL, (SELECT id FROM checklist_columns WHERE board_scope = ?1 ORDER BY sort_order ASC, id ASC LIMIT 1), ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ",
            params![
                board_scope,
                remote.title,
                remote.note,
                remote.due_date,
                sort_order,
                if remote.completed { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    let sync_id = format!("{ENTITY_CHECKLIST_TASK}-{local_id}");
    ensure_sync_meta(
        connection,
        ENTITY_CHECKLIST_TASK,
        local_id,
        &sync_id,
        &now,
        None,
    )?;
    upsert_link(
        connection,
        ENTITY_CHECKLIST_TASK,
        Some(local_id),
        &sync_id,
        REMOTE_FEISHU_TASK,
        &remote.id,
        Some(&remote.tasklist_guid),
        None,
        None,
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )
}

fn delete_local_task(connection: &Connection, entity_type: &str, id: i64) -> Result<(), String> {
    mark_entity_deleted(connection, entity_type, id, Utc::now().timestamp_millis())?;
    if entity_type == ENTITY_TODAY_PLAN_ITEM {
        connection
            .execute("DELETE FROM today_plan_items WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "DELETE FROM today_plan_items WHERE source_task_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute("DELETE FROM checklist_tasks WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn update_local_schedule_block_from_remote(
    connection: &Connection,
    id: i64,
    remote: &RemoteEvent,
) -> Result<(), String> {
    let updated_at = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    connection
        .execute(
            "
            UPDATE schedule_blocks
            SET schedule_date = ?1,
                title = CASE WHEN ?2 = '' THEN title ELSE ?2 END,
                note = ?3,
                start_minute = ?4,
                end_minute = ?5, updated_at = ?6
            WHERE id = ?7
            ",
            params![
                remote.schedule_date,
                remote.title,
                remote.note,
                remote.start_minute,
                remote.end_minute,
                updated_at,
                id
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn is_importable_remote_event(remote: &RemoteEvent) -> bool {
    !remote.title.trim().is_empty()
}

fn create_local_schedule_block_from_remote(
    connection: &Connection,
    remote: &RemoteEvent,
) -> Result<(), String> {
    let now = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    connection
        .execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              start_minute, end_minute, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 'general', NULL, NULL, ?4, ?5, 'planned', ?6, ?6)
            ",
            params![
                remote.schedule_date,
                remote.title,
                remote.note,
                remote.start_minute,
                remote.end_minute,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    let sync_id = format!("{ENTITY_SCHEDULE_BLOCK}-{local_id}");
    ensure_sync_meta(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        local_id,
        &sync_id,
        &now,
        None,
    )?;
    upsert_link(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        Some(local_id),
        &sync_id,
        REMOTE_FEISHU_EVENT,
        &remote.id,
        None,
        Some(&remote_event_fingerprint(remote)),
        Some(&remote_event_fingerprint(remote)),
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )
}

fn delete_local_schedule_block(connection: &Connection, id: i64) -> Result<(), String> {
    mark_entity_deleted(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        id,
        Utc::now().timestamp_millis(),
    )?;
    connection
        .execute("DELETE FROM schedule_blocks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_link_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: &str,
    remote_kind: &str,
) -> Result<Option<FeishuLink>, String> {
    connection
        .query_row(
            "
            SELECT id, entity_type, local_id, local_sync_id, remote_kind, remote_id,
                   remote_parent_id, remote_etag, remote_change_key, remote_last_modified
            FROM feishu_sync_links
            WHERE entity_type = ?1 AND local_sync_id = ?2 AND remote_kind = ?3 AND deleted_at IS NULL
            ",
            params![entity_type, sync_id, remote_kind],
            row_to_link,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_link_by_remote_id(
    connection: &Connection,
    remote_kind: &str,
    remote_id: &str,
) -> Result<Option<FeishuLink>, String> {
    connection
        .query_row(
            "
            SELECT id, entity_type, local_id, local_sync_id, remote_kind, remote_id,
                   remote_parent_id, remote_etag, remote_change_key, remote_last_modified
            FROM feishu_sync_links
            WHERE remote_kind = ?1 AND remote_id = ?2 AND deleted_at IS NULL
            ",
            params![remote_kind, remote_id],
            row_to_link,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_sync_meta_local_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: &str,
) -> Result<Option<(i64, Option<i64>)>, String> {
    connection
        .query_row(
            "
            SELECT local_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND sync_id = ?2
            ",
            params![entity_type, sync_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn upsert_link(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
    local_sync_id: &str,
    remote_kind: &str,
    remote_id: &str,
    remote_parent_id: Option<&str>,
    remote_etag: Option<&str>,
    remote_change_key: Option<&str>,
    remote_last_modified: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            DELETE FROM feishu_sync_links
            WHERE remote_kind = ?1
              AND remote_id = ?2
              AND NOT (entity_type = ?3 AND local_sync_id = ?4 AND remote_kind = ?1)
            ",
            params![remote_kind, remote_id, entity_type, local_sync_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "
            INSERT INTO feishu_sync_links (
              entity_type, local_id, local_sync_id, remote_kind, remote_id, remote_parent_id,
              remote_etag, remote_change_key, remote_last_modified, last_synced_at, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
            ON CONFLICT(entity_type, local_sync_id, remote_kind) DO UPDATE SET
              local_id = excluded.local_id,
              remote_id = excluded.remote_id,
              remote_parent_id = excluded.remote_parent_id,
              remote_etag = excluded.remote_etag,
              remote_change_key = excluded.remote_change_key,
              remote_last_modified = COALESCE(excluded.remote_last_modified, feishu_sync_links.remote_last_modified),
              last_synced_at = excluded.last_synced_at,
              deleted_at = NULL
            ",
            params![
                entity_type,
                local_id,
                local_sync_id,
                remote_kind,
                remote_id,
                remote_parent_id,
                remote_etag,
                remote_change_key,
                remote_last_modified,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn mark_link_deleted(connection: &Connection, link_id: i64) -> Result<(), String> {
    connection
        .execute(
            "UPDATE feishu_sync_links SET deleted_at = ?1, last_synced_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), link_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn clear_feishu_tasklist_settings(connection: &Connection) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    for key in [
        FEISHU_TASKLIST_GUID_KEY,
        FEISHU_LEGACY_TASKLIST_GUID_KEY,
        &feishu_tasklist_setting_key(TASKLIST_KEY_POLITICS),
        &feishu_tasklist_setting_key(TASKLIST_KEY_ENGLISH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MATH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MAJOR),
        &feishu_tasklist_setting_key(TASKLIST_KEY_GENERAL),
        &feishu_tasklist_setting_key(TASKLIST_KEY_TODAY),
    ] {
        set_setting(connection, key, "", &now)?;
    }
    Ok(())
}

fn row_to_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeishuLink> {
    Ok(FeishuLink {
        id: row.get(0)?,
        remote_kind: row.get(4)?,
        remote_id: row.get(5)?,
        remote_parent_id: row.get(6)?,
        remote_etag: row.get(7)?,
        remote_change_key: row.get(8)?,
        remote_last_modified: row.get(9)?,
    })
}

fn ensure_sync_meta(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    sync_id: &str,
    updated_at: &str,
    deleted_at: Option<i64>,
) -> Result<(), String> {
    let updated_at_millis = parse_rfc3339_millis(updated_at)?;
    connection
        .execute(
            "
            INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(entity_type, local_id) DO UPDATE SET
              sync_id = excluded.sync_id,
              deleted_at = excluded.deleted_at,
              updated_at = excluded.updated_at
            ",
            params![
                entity_type,
                local_id,
                sync_id,
                deleted_at,
                updated_at_millis
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn board_scope_for_tasklist_key(key: &str) -> &'static str {
    match key {
        TASKLIST_KEY_POLITICS => "checklist:politics",
        TASKLIST_KEY_ENGLISH => "checklist:english",
        TASKLIST_KEY_MATH => "checklist:math",
        TASKLIST_KEY_MAJOR => "checklist:major",
        _ => "checklist:general",
    }
}

fn ensure_checklist_bucket(connection: &Connection, board_scope: &str) -> Result<(), String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM checklist_columns WHERE board_scope = ?1",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if count > 0 {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            INSERT INTO checklist_columns (board_scope, name, sort_order, created_at, updated_at)
            VALUES (?1, '默认清单', 0, ?2, ?2)
            ",
            params![board_scope, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn handle_oauth_code(app: &AppHandle, code: &str) -> Result<String, String> {
    let connection = open_database(&database_path(app)?)?;
    let settings = read_feishu_settings(&connection)?;
    let data = exchange_user_token(&settings, code)?;
    persist_token_response(&connection, &data)?;
    Ok("飞书登录成功，已保存本机 Token。".to_string())
}

fn ensure_access_token(connection: &Connection) -> Result<TokenSet, String> {
    let access_token = get_setting(connection, FEISHU_ACCESS_TOKEN_KEY, "")?;
    let refresh_token = get_setting(connection, FEISHU_REFRESH_TOKEN_KEY, "")?;
    let expires_at_raw = get_setting(connection, FEISHU_TOKEN_EXPIRES_AT_KEY, "")?;
    let expires_at = parse_rfc3339(&expires_at_raw).unwrap_or_else(Utc::now);
    if !access_token.is_empty() && expires_at > Utc::now() + Duration::seconds(60) {
        return Ok(TokenSet { access_token });
    }
    if refresh_token.is_empty() {
        return Err("飞书尚未登录，请先完成浏览器授权。".to_string());
    }
    let settings = read_feishu_settings(connection)?;
    let data = refresh_user_token(&settings, &refresh_token)?;
    persist_token_response(connection, &data)?;
    Ok(TokenSet {
        access_token: get_setting(connection, FEISHU_ACCESS_TOKEN_KEY, "")?,
    })
}

fn exchange_user_token(settings: &FeishuSyncSettings, code: &str) -> Result<Value, String> {
    let client = http_client()?;
    let v2 = client
        .post(format!("{FEISHU_BASE}/open-apis/authen/v2/oauth/token"))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "grant_type": "authorization_code",
            "client_id": settings.app_id,
            "client_secret": settings.app_secret,
            "code": code,
            "redirect_uri": settings.redirect_uri,
        }))
        .send()
        .map_err(|error| format!("交换飞书 user_access_token 失败：{error}"))
        .and_then(parse_feishu_response);
    match v2 {
        Ok(value) => Ok(value),
        Err(v2_error) => {
            let app_access_token = get_app_access_token(settings)?;
            client
                .post(format!("{FEISHU_BASE}/open-apis/authen/v1/access_token"))
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {app_access_token}"))
                .json(&json!({
                    "grant_type": "authorization_code",
                    "code": code
                }))
                .send()
                .map_err(|error| format!("交换飞书 user_access_token 失败：{error}"))
                .and_then(parse_feishu_response)
                .map_err(|v1_error| {
                    format!("飞书 OAuth v2 交换失败：{v2_error}；v1 兼容交换也失败：{v1_error}")
                })
        }
    }
}

fn refresh_user_token(settings: &FeishuSyncSettings, refresh_token: &str) -> Result<Value, String> {
    let client = http_client()?;
    let v2 = client
        .post(format!("{FEISHU_BASE}/open-apis/authen/v2/oauth/token"))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "grant_type": "refresh_token",
            "client_id": settings.app_id,
            "client_secret": settings.app_secret,
            "refresh_token": refresh_token
        }))
        .send()
        .map_err(|error| format!("刷新飞书 Token 失败：{error}"))
        .and_then(parse_feishu_response);
    match v2 {
        Ok(value) => Ok(value),
        Err(v2_error) => {
            let app_access_token = get_app_access_token(settings)?;
            client
                .post(format!(
                    "{FEISHU_BASE}/open-apis/authen/v1/refresh_access_token"
                ))
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {app_access_token}"))
                .json(&json!({
                    "grant_type": "refresh_token",
                    "refresh_token": refresh_token
                }))
                .send()
                .map_err(|error| format!("刷新飞书 Token 失败：{error}"))
                .and_then(parse_feishu_response)
                .map_err(|v1_error| {
                    format!("飞书 OAuth v2 刷新失败：{v2_error}；v1 兼容刷新也失败：{v1_error}")
                })
        }
    }
}

fn get_app_access_token(settings: &FeishuSyncSettings) -> Result<String, String> {
    let response = http_client()?
        .post(format!(
            "{FEISHU_BASE}/open-apis/auth/v3/app_access_token/internal"
        ))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "app_id": settings.app_id,
            "app_secret": settings.app_secret,
        }))
        .send()
        .map_err(|error| format!("获取飞书 app_access_token 失败：{error}"))?;
    let data = parse_feishu_response(response)?;
    data.get("app_access_token")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "飞书未返回 app_access_token。".to_string())
}

fn persist_token_response(connection: &Connection, value: &Value) -> Result<(), String> {
    let access_token = value
        .get("access_token")
        .or_else(|| value.get("user_access_token"))
        .and_then(Value::as_str)
        .ok_or_else(|| "飞书未返回 user_access_token。".to_string())?;
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let expires_in = value
        .get("expires_in")
        .or_else(|| value.get("expires_in_sec"))
        .and_then(value_to_i64)
        .unwrap_or(7200);
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    let now = Utc::now().to_rfc3339();
    set_setting(connection, FEISHU_ACCESS_TOKEN_KEY, access_token, &now)?;
    if !refresh_token.is_empty() {
        set_setting(connection, FEISHU_REFRESH_TOKEN_KEY, refresh_token, &now)?;
    }
    set_setting(
        connection,
        FEISHU_TOKEN_EXPIRES_AT_KEY,
        &expires_at.to_rfc3339(),
        &now,
    )?;
    Ok(())
}

fn read_feishu_settings(connection: &Connection) -> Result<FeishuSyncSettings, String> {
    Ok(normalize_settings(FeishuSyncSettings {
        enabled: get_bool_setting(connection, FEISHU_SYNC_ENABLED_KEY, false)?,
        app_id: get_setting(connection, FEISHU_APP_ID_KEY, "")?,
        app_secret: get_setting(connection, FEISHU_APP_SECRET_KEY, "")?,
        redirect_uri: get_setting(connection, FEISHU_REDIRECT_URI_KEY, DEFAULT_REDIRECT_URI)?,
    }))
}

fn normalize_settings(settings: FeishuSyncSettings) -> FeishuSyncSettings {
    FeishuSyncSettings {
        enabled: settings.enabled,
        app_id: settings.app_id.trim().to_string(),
        app_secret: settings.app_secret.trim().to_string(),
        redirect_uri: settings
            .redirect_uri
            .trim()
            .to_string()
            .if_empty(DEFAULT_REDIRECT_URI),
    }
}

fn clear_feishu_tokens(connection: &Connection) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    for key in [
        FEISHU_ACCESS_TOKEN_KEY,
        FEISHU_REFRESH_TOKEN_KEY,
        FEISHU_TOKEN_EXPIRES_AT_KEY,
        FEISHU_TASKLIST_GUID_KEY,
        FEISHU_LEGACY_TASKLIST_GUID_KEY,
        FEISHU_CALENDAR_ID_KEY,
        &feishu_tasklist_setting_key(TASKLIST_KEY_POLITICS),
        &feishu_tasklist_setting_key(TASKLIST_KEY_ENGLISH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MATH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MAJOR),
        &feishu_tasklist_setting_key(TASKLIST_KEY_GENERAL),
        &feishu_tasklist_setting_key(TASKLIST_KEY_TODAY),
        FEISHU_OAUTH_STATE_KEY,
        FEISHU_OAUTH_URL_KEY,
        FEISHU_OAUTH_MESSAGE_KEY,
    ] {
        set_setting(connection, key, "", &now)?;
    }
    Ok(())
}

fn is_feishu_access_token_usable(access_token: &str, expires_at: Option<&str>) -> bool {
    if access_token.is_empty() {
        return false;
    }

    expires_at
        .and_then(parse_rfc3339)
        .map(|value| value > Utc::now() + Duration::seconds(60))
        .unwrap_or(true)
}

fn record_feishu_run(
    connection: &Connection,
    run_id: &str,
    trigger: &str,
    started_at: DateTime<Utc>,
    result: &FeishuSyncResult,
) -> Result<(), String> {
    let finished_at = Utc::now();
    connection
        .execute(
            "
            INSERT INTO feishu_sync_runs (
              run_id, trigger, status, started_at, finished_at, duration_ms,
              pushed_count, pulled_count, deleted_count, conflict_count, task_count,
              calendar_count, message, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ",
            params![
                run_id,
                trigger,
                result.status,
                started_at.to_rfc3339(),
                finished_at.to_rfc3339(),
                (finished_at - started_at).num_milliseconds(),
                result.pushed_count,
                result.pulled_count,
                result.deleted_count,
                result.conflict_count,
                result.task_count,
                result.calendar_count,
                result.message,
                if result.status == "failed" {
                    Some(result.message.as_str())
                } else {
                    None
                },
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn list_feishu_runs(
    connection: &Connection,
    limit: i64,
) -> Result<Vec<FeishuSyncRunSummary>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, run_id, trigger, status, started_at, finished_at, duration_ms,
                   pushed_count, pulled_count, deleted_count, conflict_count, task_count,
                   calendar_count, message, error_message
            FROM feishu_sync_runs
            ORDER BY finished_at DESC, id DESC
            LIMIT ?1
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![limit], row_to_feishu_run)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn last_feishu_sync_run(connection: &Connection) -> Result<Option<FeishuSyncRunSummary>, String> {
    connection
        .query_row(
            "
            SELECT id, run_id, trigger, status, started_at, finished_at, duration_ms,
                   pushed_count, pulled_count, deleted_count, conflict_count, task_count,
                   calendar_count, message, error_message
            FROM feishu_sync_runs
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            ",
            [],
            row_to_feishu_run,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn row_to_feishu_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeishuSyncRunSummary> {
    Ok(FeishuSyncRunSummary {
        id: row.get(0)?,
        run_id: row.get(1)?,
        trigger: row.get(2)?,
        status: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        duration_ms: row.get(6)?,
        pushed_count: row.get(7)?,
        pulled_count: row.get(8)?,
        deleted_count: row.get(9)?,
        conflict_count: row.get(10)?,
        task_count: row.get(11)?,
        calendar_count: row.get(12)?,
        message: row.get(13)?,
        error_message: row.get(14)?,
    })
}

fn skipped_result(message: &str) -> FeishuSyncResult {
    FeishuSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        pushed_count: 0,
        pulled_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        task_count: 0,
        calendar_count: 0,
        synced_at: Utc::now().to_rfc3339(),
    }
}

fn create_feishu_local_database_backup(
    database_path: &Path,
    app_data_dir: &Path,
) -> Result<PathBuf, String> {
    let backup_path = app_data_dir.join(format!(
        "kaoyan-focus.before-feishu-rebuild-{}.sqlite3",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(database_path, &backup_path)
        .map_err(|error| format!("备份本地数据库失败：{error}"))?;
    Ok(backup_path)
}

fn export_feishu_tasklists_backup(
    feishu: &FeishuClient,
    app_data_dir: &Path,
    tasklists: &[Value],
) -> Result<PathBuf, String> {
    let backup_dir = app_data_dir.join("feishu-backups");
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_dir.join(format!(
        "feishu-tasklists-before-rebuild-{}.json",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let mut exported = Vec::new();
    for tasklist in tasklists.iter().filter(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .map(is_app_tasklist_name)
            .unwrap_or(false)
    }) {
        let guid = tasklist
            .get("guid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let tasks = if guid.is_empty() {
            Vec::new()
        } else {
            feishu.get_paged(&format!(
                "/open-apis/task/v2/tasklists/{}/tasks?page_size=100&user_id_type=open_id",
                encode_path_segment(guid)
            ))?
        };
        exported.push(json!({
            "tasklist": tasklist,
            "tasks": tasks,
        }));
    }
    let payload = json!({
        "exported_at": Utc::now().to_rfc3339(),
        "source": "kaoyan-focus-feishu-rebuild",
        "tasklists": exported,
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|error| format!("序列化飞书备份失败：{error}"))?;
    fs::write(&backup_path, bytes).map_err(|error| format!("写入飞书备份失败：{error}"))?;
    Ok(backup_path)
}

fn is_app_tasklist_name(name: &str) -> bool {
    name == BRIDGE_CONTAINER_NAME || name.starts_with("考研专注 - ")
}

fn receive_oauth_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|error| format!("接收飞书登录回调失败：{error}"))?;
    let mut buffer = [0_u8; 8192];
    let size = stream
        .read(&mut buffer)
        .map_err(|error| format!("读取飞书登录回调失败：{error}"))?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let first_line = request.lines().next().unwrap_or_default();
    let target = first_line.split_whitespace().nth(1).unwrap_or_default();
    let query = target
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let params = parse_query_params(query);
    let result = if let Some(error) = params.get("error") {
        Err(format!("飞书授权失败：{error}"))
    } else if params.get("state").map(String::as_str) != Some(expected_state) {
        Err("飞书授权 state 不匹配，请重新登录。".to_string())
    } else {
        params
            .get("code")
            .filter(|value| !value.is_empty())
            .cloned()
            .ok_or_else(|| "飞书回调未包含授权 code。".to_string())
    };
    let body = match &result {
        Ok(_) => "Feishu login received. You can return to Kaoyan Focus.",
        Err(_) => "Feishu login failed. Please return to Kaoyan Focus and retry.",
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.as_bytes().len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    result
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn callback_bind_addr(redirect_uri: &str) -> Result<String, String> {
    let url = Url::parse(redirect_uri).map_err(|error| format!("飞书回调地址不正确：{error}"))?;
    let host = url.host_str().unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" {
        return Err("飞书本地回调地址必须使用 127.0.0.1 或 localhost。".to_string());
    }
    let port = url.port().unwrap_or(80);
    Ok(format!("127.0.0.1:{port}"))
}

fn get_setting(connection: &Connection, key: &str, fallback: &str) -> Result<String, String> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
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
            params![key, value, updated_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn non_empty_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    let value = get_setting(connection, key, "")?.trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[test]
    fn recognizes_deleted_calendar_event_error() {
        assert!(is_feishu_deleted_event_error(
            "Feishu returned status 403 / code 193003: event is deleted"
        ));
        assert!(!is_feishu_deleted_event_error(
            "Feishu returned status 403 / code 193004: permission denied"
        ));
    }

    #[test]
    fn skips_untitled_remote_calendar_imports() {
        let event = parse_remote_event(&json!({
            "event_id": "remote-blank-title",
            "description": "",
            "start_time": { "timestamp": "1779235200" },
            "end_time": { "timestamp": "1779238800" },
            "updated_time": "1779235200000"
        }))
        .expect("event parses");

        assert_eq!(event.title, "");
        assert!(!is_importable_remote_event(&event));
    }

    #[test]
    fn finds_orphan_calendar_event_links() {
        let connection = Connection::open_in_memory().expect("in-memory db");
        connection
            .execute_batch(
                "
                CREATE TABLE schedule_blocks (
                  id INTEGER PRIMARY KEY,
                  title TEXT NOT NULL
                );
                CREATE TABLE sync_meta (
                  entity_type TEXT NOT NULL,
                  local_id INTEGER NOT NULL,
                  sync_id TEXT NOT NULL,
                  deleted_at INTEGER,
                  updated_at INTEGER,
                  PRIMARY KEY (entity_type, local_id)
                );
                CREATE TABLE feishu_sync_links (
                  id INTEGER PRIMARY KEY,
                  entity_type TEXT NOT NULL,
                  local_id INTEGER,
                  local_sync_id TEXT NOT NULL,
                  remote_kind TEXT NOT NULL,
                  remote_id TEXT NOT NULL,
                  remote_parent_id TEXT,
                  remote_etag TEXT,
                  remote_change_key TEXT,
                  remote_last_modified TEXT,
                  last_synced_at TEXT,
                  deleted_at TEXT
                );
                ",
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO schedule_blocks (id, title) VALUES (1, 'live'), (2, 'deleted')",
                [],
            )
            .expect("blocks");
        connection
            .execute(
                "INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
                 VALUES ('schedule_block', 1, 'schedule_block-1', NULL, 1),
                        ('schedule_block', 2, 'schedule_block-2', 2, 2)",
                [],
            )
            .expect("sync meta");
        connection
            .execute(
                "INSERT INTO feishu_sync_links
                 (id, entity_type, local_id, local_sync_id, remote_kind, remote_id, deleted_at)
                 VALUES
                 (1, 'schedule_block', 1, 'schedule_block-1', 'feishu_calendar_event', 'live', NULL),
                 (2, 'schedule_block', 2, 'schedule_block-2', 'feishu_calendar_event', 'tombstone', NULL),
                 (3, 'schedule_block', 3, 'schedule_block-3', 'feishu_calendar_event', 'missing-local', NULL),
                 (4, 'schedule_block', 4, 'schedule_block-4', 'feishu_calendar_event', 'already-deleted', 'now'),
                 (5, 'checklist_task', 5, 'checklist_task-5', 'feishu_calendar_event', 'other-entity', NULL)",
                [],
            )
            .expect("links");

        let links = load_orphan_calendar_event_links(&connection).expect("loads orphan links");
        let remote_ids = links
            .into_iter()
            .map(|link| link.remote_id)
            .collect::<Vec<_>>();

        assert_eq!(remote_ids, vec!["tombstone", "missing-local"]);
    }

    #[test]
    fn local_calendar_change_pushes_when_remote_timestamp_is_missing() {
        assert_eq!(
            linked_calendar_action(2_000, None, true, false, true),
            LinkedCalendarAction::PushLocal
        );
        assert_eq!(
            linked_calendar_action(6_500, None, true, false, false),
            LinkedCalendarAction::PushLocal
        );
        assert_eq!(
            linked_calendar_action(2_000, None, false, true, false),
            LinkedCalendarAction::PullRemote
        );
        assert_eq!(
            linked_calendar_action(2_000, None, false, false, true),
            LinkedCalendarAction::RefreshLink
        );
    }

    #[test]
    fn calendar_fingerprint_tracks_block_time_changes() {
        let local = LocalScheduleBlock {
            id: 1,
            sync_id: "schedule_block-1".to_string(),
            schedule_date: "2026-05-20".to_string(),
            title: "Math".to_string(),
            note: Some("chapter 3".to_string()),
            start_minute: 600,
            end_minute: 660,
            status: "planned".to_string(),
            updated_at: "2026-05-20T00:00:00Z".to_string(),
            deleted_at: None,
        };
        let remote = RemoteEvent {
            id: "event-1".to_string(),
            title: "Math".to_string(),
            note: Some("chapter 3".to_string()),
            schedule_date: "2026-05-20".to_string(),
            start_minute: 630,
            end_minute: 690,
            updated_millis: None,
            marker_sync_id: Some("schedule_block-1".to_string()),
        };

        assert_ne!(
            local_schedule_block_fingerprint(&local),
            remote_event_fingerprint(&remote)
        );
    }

    #[test]
    #[ignore = "uses the local app database and live Feishu account"]
    fn live_sync_feishu_bridge_once() {
        let trigger =
            std::env::var("FEISHU_LIVE_TEST_TRIGGER").unwrap_or_else(|_| "live_test".to_string());
        let database_path = PathBuf::from(std::env::var("APPDATA").expect("APPDATA is required"))
            .join("com.kaoyan.focus")
            .join("kaoyan-focus.sqlite3");
        let result = sync_feishu_bridge_blocking(
            database_path,
            trigger,
            Uuid::new_v4().to_string(),
            Utc::now(),
        )
        .expect("live Feishu sync should complete");
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serializes sync result")
        );
        assert_ne!(result.status, "failed", "{}", result.message);
    }
}

fn feishu_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("https://") {
        path_or_url.to_string()
    } else {
        format!("{FEISHU_BASE}{path_or_url}")
    }
}

fn parse_feishu_response(response: Response) -> Result<Value, String> {
    let status = response.status();
    let value: Value = response.json().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(feishu_value_error_message(status, &value));
    }
    if value.get("access_token").is_some() || value.get("user_access_token").is_some() {
        return Ok(value);
    }
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code == 0 {
        return Ok(value.get("data").cloned().unwrap_or(value));
    }
    Err(feishu_value_error_message(status, &value))
}

fn feishu_value_error_message(status: StatusCode, value: &Value) -> String {
    let message = value
        .get("msg")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .or_else(|| value.get("error_description").and_then(Value::as_str))
        .or_else(|| value.get("error").and_then(Value::as_str))
        .unwrap_or("未知错误");
    let code = value
        .get("code")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    format!(
        "飞书返回状态 {} / code {}：{}",
        status.as_u16(),
        code,
        message
    )
}

fn is_feishu_deleted_event_error(error: &str) -> bool {
    error.contains("193003")
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={}", encode_form_component(value))
}

fn body_with_marker(note: Option<&str>, entity_type: &str, sync_id: &str) -> String {
    let mut content = note.unwrap_or("").trim().to_string();
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(&format!("{MARKER_PREFIX}{entity_type}:{sync_id}]"));
    content
}

fn marker_json(entity_type: &str, sync_id: &str) -> String {
    json!({
        "source": "kaoyan-focus",
        "entity_type": entity_type,
        "sync_id": sync_id
    })
    .to_string()
}

fn extract_marker(raw: &str) -> Option<(String, String)> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if value.get("source").and_then(Value::as_str) == Some("kaoyan-focus") {
            let entity_type = value.get("entity_type").and_then(Value::as_str)?;
            let sync_id = value.get("sync_id").and_then(Value::as_str)?;
            return Some((entity_type.to_string(), sync_id.to_string()));
        }
    }
    raw.lines().find_map(|line| {
        let trimmed = line.trim();
        let body = trimmed.strip_prefix(MARKER_PREFIX)?.strip_suffix(']')?;
        let (entity_type, sync_id) = body.split_once(':')?;
        Some((entity_type.to_string(), sync_id.to_string()))
    })
}

fn strip_marker(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.trim_start().starts_with(MARKER_PREFIX))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn calendar_sync_range() -> (i64, i64) {
    let today = Local::now().date_naive();
    let start = today - Duration::days(30);
    let end = today + Duration::days(180);
    (
        local_date_minute_to_timestamp(&start.format("%Y-%m-%d").to_string(), 0),
        local_date_minute_to_timestamp(&end.format("%Y-%m-%d").to_string(), 1439),
    )
}

fn date_in_sync_range(date: &str) -> bool {
    let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return false;
    };
    let today = Local::now().date_naive();
    date >= today - Duration::days(30) && date <= today + Duration::days(180)
}

fn today_date_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn due_date_to_millis(date: &str) -> i64 {
    local_date_minute_to_timestamp(date, 0) * 1000
}

fn minute_to_timestamp(date: &str, minute: i64) -> i64 {
    local_date_minute_to_timestamp(date, minute.clamp(0, 1440))
}

fn local_date_minute_to_timestamp(date: &str, minute: i64) -> i64 {
    let date =
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive());
    let clamped = minute.clamp(0, 1440);
    let hour = (clamped / 60).min(23) as u32;
    let minute = if clamped >= 1440 {
        59
    } else {
        (clamped % 60) as u32
    };
    let naive = date
        .and_hms_opt(hour, minute, if clamped >= 1440 { 59 } else { 0 })
        .unwrap_or_else(|| Local::now().naive_local());
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .unwrap_or_else(Local::now)
        .timestamp()
}

fn timestamp_to_local_date_minute(timestamp: i64) -> Option<(String, i64)> {
    let timestamp = normalize_timestamp_seconds(timestamp);
    let value = Local.timestamp_opt(timestamp, 0).single()?;
    Some((
        value.date_naive().format("%Y-%m-%d").to_string(),
        i64::from(value.hour()) * 60 + i64::from(value.minute()),
    ))
}

fn millis_to_local_date_string(value: i64) -> String {
    let seconds = normalize_timestamp_seconds(value);
    Local
        .timestamp_opt(seconds, 0)
        .single()
        .unwrap_or_else(Local::now)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn normalize_timestamp_seconds(value: i64) -> i64 {
    if value > 9_999_999_999 {
        value / 1000
    } else {
        value
    }
}

fn normalize_timestamp_millis(value: i64) -> i64 {
    if value < 9_999_999_999 {
        value * 1000
    } else {
        value
    }
}

fn parse_link_millis(value: &str) -> Option<i64> {
    value
        .parse::<i64>()
        .ok()
        .map(normalize_timestamp_millis)
        .or_else(|| parse_rfc3339_millis(value).ok())
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn parse_rfc3339_millis(value: &str) -> Result<i64, String> {
    parse_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .ok_or_else(|| format!("时间格式不正确：{value}"))
}

fn millis_to_rfc3339(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|item| item.parse::<i64>().ok()))
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn encode_form_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
