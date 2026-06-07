use crate::{
    focus::{session::FocusSession, subject::Subject},
    storage::db::open_database,
    sync_package::{load_or_create_device_id, mark_entity_deleted},
    AppState,
};
use chrono::{DateTime, Datelike, Duration, FixedOffset, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::thread;
use tauri::{AppHandle, Manager, State};

const MIN_RECORDED_FOCUS_SECONDS: i64 = 60;
const CONTROL_PAUSE: &str = "pause";
const CONTROL_RESUME: &str = "resume";
const CONTROL_CONFIRM_BREAK: &str = "confirm_break";
const CONTROL_FINISH: &str = "finish";
const CONTROL_EMERGENCY_EXIT: &str = "emergency_exit";
const CONTROL_SWITCH_SUBJECT: &str = "switch_subject";
const PRIMARY_OWNER_DEVICE_ID_KEY: &str = "primary_owner_device_id";

fn trigger_shared_sync(app: &AppHandle, trigger: &'static str) {
    let sync_app = app.clone();
    thread::spawn(move || {
        let _ = crate::commands::sync::sync_object_storage_after_external_change(sync_app, trigger);
    });
    crate::commands::feishu::sync_feishu_bridge_after_local_change(app.clone(), trigger);
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusStatsSummary {
    pub today_seconds: i64,
    pub week_seconds: i64,
    pub month_seconds: i64,
    pub interruption_count: i64,
    pub subjects: Vec<SubjectStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectStats {
    pub subject: Subject,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSessionRecovery {
    pub recovery_status: String,
    pub session: FocusSession,
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StudyModeLinks {
    pub schedule_block_id: Option<i64>,
    pub today_plan_item_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudyModeState {
    pub id: Option<i64>,
    pub state_revision: Option<i64>,
    pub phase: String,
    pub status: String,
    pub mode: String,
    pub subject_id: Option<i64>,
    pub planned_seconds: i64,
    pub focus_seconds: i64,
    pub break_seconds: i64,
    pub long_break_seconds: i64,
    pub long_break_interval: i64,
    pub effective_break_seconds: i64,
    pub break_kind: String,
    pub cycle_index: i64,
    pub started_at: Option<String>,
    pub phase_started_at: Option<String>,
    pub paused_at: Option<String>,
    pub ended_at: Option<String>,
    pub current_session: Option<FocusSession>,
    pub study_elapsed_seconds: i64,
    pub study_remaining_seconds: i64,
    pub phase_elapsed_seconds: i64,
    pub phase_remaining_seconds: i64,
    pub focus_enforcement_active: bool,
    pub whitelist_enabled: bool,
    pub is_paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StudyRuntimeSyncMarker {
    id: Option<i64>,
    state_revision: i64,
    phase: String,
    status: String,
    subject_id: Option<i64>,
    cycle_index: i64,
    paused_at: Option<String>,
    current_session_id: Option<i64>,
    break_kind: String,
}

#[derive(Debug, Clone)]
struct StudyModeRecord {
    id: i64,
    state_revision: i64,
    mode: String,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    whitelist_enabled: bool,
    phase: String,
    cycle_index: i64,
    started_at: String,
    phase_started_at: String,
    paused_at: Option<String>,
    accumulated_study_seconds: i64,
    _total_paused_seconds: i64,
    _phase_paused_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
    schedule_block_id: Option<i64>,
    _today_plan_item_id: Option<i64>,
    status: String,
}

#[tauri::command]
pub fn start_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    planned_seconds: i64,
    mode: String,
    subject_id: Option<i64>,
) -> Result<FocusSession, String> {
    if planned_seconds <= 0 {
        return Err("专注时长必须大于 0 秒".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let connection = open_database(&database_path(&app)?)?;
    let session = insert_focus_session(&connection, planned_seconds, &mode, subject_id, &now)?;
    set_runtime_state(state.inner(), false, Some(session.id))?;
    Ok(session)
}

#[tauri::command]
pub fn start_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    mode: String,
    subject_id: Option<i64>,
    whitelist_enabled: Option<bool>,
) -> Result<StudyModeState, String> {
    if planned_seconds <= 0 {
        return Err("学习模式时长必须大于 0 秒".to_string());
    }

    if focus_seconds <= 0 {
        return Err("番茄钟时长必须大于 0 秒".to_string());
    }

    if break_seconds <= 0 {
        return Err("短休息时长必须大于 0 秒".to_string());
    }

    if long_break_seconds <= 0 {
        return Err("长休息时长必须大于 0 秒".to_string());
    }

    if long_break_interval <= 0 {
        return Err("长休息间隔必须大于 0".to_string());
    }

    if mode != "normal" && mode != "strict" {
        return Err("未知的专注模式".to_string());
    }

    let whitelist_enabled = mode == "strict" || whitelist_enabled.unwrap_or(true);
    let connection = open_database(&database_path(&app)?)?;
    ensure_current_device_can_start_study_mode(&connection)?;
    if get_active_study_mode_record(&connection)?.is_some() {
        return Err("已有学习模式正在进行中".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let session = insert_focus_session(&connection, focus_seconds, &mode, subject_id, &now)?;
    connection
        .execute(
            "
            INSERT INTO study_modes (
              mode,
              subject_id,
              planned_seconds,
              focus_seconds,
              break_seconds,
              long_break_seconds,
              long_break_interval,
              state_revision,
              phase,
              cycle_index,
              started_at,
              phase_started_at,
              accumulated_study_seconds,
              current_session_id,
              whitelist_enabled,
              status,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 'focus', 1, ?8, ?8, 0, ?9, ?10, 'active', ?8, ?8)
            ",
            params![
                mode,
                subject_id,
                planned_seconds,
                focus_seconds,
                break_seconds,
                long_break_seconds,
                long_break_interval,
                now,
                session.id,
                whitelist_enabled
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), true, Some(session.id))?;
    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

pub fn start_study_mode_with_links(
    app: AppHandle,
    state: &AppState,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    mode: String,
    subject_id: Option<i64>,
    links: StudyModeLinks,
) -> Result<StudyModeState, String> {
    if planned_seconds <= 0 {
        return Err("Study mode duration must be greater than 0 seconds.".to_string());
    }

    if focus_seconds <= 0 {
        return Err("Focus duration must be greater than 0 seconds.".to_string());
    }

    if break_seconds <= 0 {
        return Err("Break duration must be greater than 0 seconds.".to_string());
    }

    if long_break_seconds <= 0 {
        return Err("Long break duration must be greater than 0 seconds.".to_string());
    }

    if long_break_interval <= 0 {
        return Err("Long break interval must be greater than 0.".to_string());
    }

    if mode != "normal" && mode != "strict" {
        return Err("Unknown focus mode.".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    ensure_current_device_can_start_study_mode(&connection)?;
    if get_active_study_mode_record(&connection)?.is_some() {
        return Err("A study mode is already running.".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let session = insert_focus_session(&connection, focus_seconds, &mode, subject_id, &now)?;
    let whitelist_enabled = true;
    connection
        .execute(
            "
            INSERT INTO study_modes (
              mode,
              subject_id,
              planned_seconds,
              focus_seconds,
              break_seconds,
              long_break_seconds,
              long_break_interval,
              state_revision,
              phase,
              cycle_index,
              started_at,
              phase_started_at,
              accumulated_study_seconds,
              current_session_id,
              schedule_block_id,
              today_plan_item_id,
              whitelist_enabled,
              status,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 'focus', 1, ?8, ?8, 0, ?9, ?10, ?11, ?12, 'active', ?8, ?8)
            ",
            params![
                mode,
                subject_id,
                planned_seconds,
                focus_seconds,
                break_seconds,
                long_break_seconds,
                long_break_interval,
                now,
                session.id,
                links.schedule_block_id,
                links.today_plan_item_id,
                whitelist_enabled
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state, true, Some(session.id))?;
    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn get_study_mode_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    advance_study_mode(&app, state.inner())
}

fn ensure_current_device_can_start_study_mode(connection: &Connection) -> Result<(), String> {
    let owner = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![PRIMARY_OWNER_DEVICE_ID_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    let Some(owner) = owner else {
        return Ok(());
    };
    let device_id = load_or_create_device_id(connection)?;
    if owner == device_id {
        Ok(())
    } else {
        Err("当前设备不是主控端，请先切换为主控设备后再开始新的专注。".to_string())
    }
}

#[tauri::command]
pub fn confirm_study_break(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        return Ok(current_state);
    }

    if current_state.phase != "awaiting_break" {
        return Err("当前还没有到休息确认时间".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Ok(idle_study_mode_state());
    };

    if record.paused_at.is_some() {
        return Err("学习模式暂停中，请先继续再开始休息".to_string());
    }

    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    if study_elapsed_seconds(&record, now)? >= record.planned_seconds {
        complete_study_mode_record(&connection, state.inner(), &record, now, "completed")?;
        return load_current_study_mode_state(&connection, now);
    }

    if let Some(session_id) = record.current_session_id {
        let actual_seconds = focus_session_actual_seconds(&record, now)?;
        finish_running_focus_session(
            &connection,
            session_id,
            actual_seconds,
            now,
            "pomodoro_completed",
        )?;
    }

    let next_accumulated = study_elapsed_seconds(&record, now)?;

    connection
        .execute(
            "
            UPDATE study_modes
            SET phase = 'break',
                state_revision = state_revision + 1,
                phase_started_at = ?1,
                phase_paused_seconds = 0,
                paused_stage_elapsed_seconds = 0,
                paused_at = NULL,
                accumulated_study_seconds = ?2,
                current_session_id = NULL,
                last_control_device_id = ?4,
                last_control_action = ?5,
                last_control_at = ?6,
                updated_at = ?1
            WHERE id = ?3 AND status = 'active'
            ",
            params![
                now.to_rfc3339(),
                next_accumulated,
                record.id,
                device_id,
                CONTROL_CONFIRM_BREAK,
                now.timestamp_millis()
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), true, None)?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn pause_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        return Ok(current_state);
    }

    if !matches!(current_state.phase.as_str(), "focus" | "awaiting_break") {
        return Err("休息阶段不能暂停".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Ok(idle_study_mode_state());
    };

    if record.paused_at.is_some() {
        return load_current_study_mode_state(&connection, Utc::now());
    }

    let now_dt = Utc::now();
    let now = now_dt.to_rfc3339();
    let device_id = load_or_create_device_id(&connection)?;
    let paused_stage_elapsed_seconds = phase_elapsed_seconds(&record, now_dt)?;
    connection
        .execute(
            "
            UPDATE study_modes
            SET paused_at = ?1,
                state_revision = state_revision + 1,
                paused_stage_elapsed_seconds = ?2,
                last_control_device_id = ?3,
                last_control_action = ?4,
                last_control_at = ?5,
                updated_at = ?1
            WHERE id = ?6 AND status = 'active'
            ",
            params![
                now,
                paused_stage_elapsed_seconds,
                device_id,
                CONTROL_PAUSE,
                now_dt.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn resume_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Ok(idle_study_mode_state());
    };

    let Some(paused_at) = record.paused_at.as_deref() else {
        return advance_study_mode(&app, state.inner());
    };

    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    let paused_seconds = seconds_since(paused_at, now)?;
    connection
        .execute(
            "
            UPDATE study_modes
            SET paused_at = NULL,
                state_revision = state_revision + 1,
                total_paused_seconds = total_paused_seconds + ?1,
                phase_started_at = ?2,
                phase_paused_seconds = 0,
                paused_stage_elapsed_seconds = 0,
                last_control_device_id = ?4,
                last_control_action = ?5,
                last_control_at = ?6,
                updated_at = ?3
            WHERE id = ?7 AND status = 'active'
            ",
            params![
                paused_seconds,
                (now - Duration::seconds(record.paused_stage_elapsed_seconds.max(0))).to_rfc3339(),
                now.to_rfc3339(),
                device_id,
                CONTROL_RESUME,
                now.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    let next_state = advance_study_mode(&app, state.inner())?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn update_study_mode_subject(
    app: AppHandle,
    state: State<'_, AppState>,
    subject_id: Option<i64>,
) -> Result<StudyModeState, String> {
    let _ = advance_study_mode(&app, state.inner())?;
    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Ok(idle_study_mode_state());
    };

    validate_subject_id(&connection, subject_id)?;

    let now_dt = Utc::now();
    let now = now_dt.to_rfc3339();
    let device_id = load_or_create_device_id(&connection)?;
    connection
        .execute(
            "
            UPDATE study_modes
            SET subject_id = ?1,
                state_revision = state_revision + 1,
                last_control_device_id = ?3,
                last_control_action = ?4,
                last_control_at = ?5,
                updated_at = ?2
            WHERE id = ?6 AND status = 'active'
            ",
            params![
                subject_id,
                now,
                device_id,
                CONTROL_SWITCH_SUBJECT,
                now_dt.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    if let Some(session_id) = record.current_session_id {
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

    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn emergency_exit_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let _ = advance_study_mode(&app, state.inner())?;
    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Ok(idle_study_mode_state());
    };

    if record.mode != "strict" {
        return Err("只有严格模式支持应急退出".to_string());
    }

    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    if let Some(session_id) = record.current_session_id {
        if let Ok(session) = get_focus_session_by_id(&connection, session_id) {
            if session.status == "running" {
                let actual_seconds = focus_session_actual_seconds(&record, now)?;
                emergency_exit_running_focus_session(&connection, session_id, actual_seconds, now)?;
            }
        }
    }

    connection
        .execute(
            "
            UPDATE study_modes
            SET phase = 'emergency_exited',
                state_revision = state_revision + 1,
                status = 'emergency_exited',
                finish_reason = 'emergency_exit',
                ended_at = ?1,
                current_session_id = NULL,
                accumulated_study_seconds = ?2,
                last_control_device_id = ?3,
                last_control_action = ?4,
                last_control_at = ?5,
                updated_at = ?1
            WHERE id = ?6 AND status = 'active'
            ",
            params![
                now.to_rfc3339(),
                study_elapsed_seconds(&record, now)?
                    .min(record.planned_seconds)
                    .max(0),
                device_id,
                CONTROL_EMERGENCY_EXIT,
                now.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), false, None)?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn reset_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let connection = open_database(&database_path(&app)?)?;
    if let Some(record) = get_active_study_mode_record(&connection)? {
        let now = Utc::now();
        let device_id = load_or_create_device_id(&connection)?;
        if let Some(session_id) = record.current_session_id {
            if let Ok(session) = get_focus_session_by_id(&connection, session_id) {
                if session.status == "running" {
                    let actual_seconds = focus_session_actual_seconds(&record, now)?;
                    finish_running_focus_session(
                        &connection,
                        session_id,
                        actual_seconds,
                        now,
                        "manual_end",
                    )?;
                }
            }
        }

        connection
            .execute(
                "
                UPDATE study_modes
                SET phase = 'finished',
                    state_revision = state_revision + 1,
                    status = 'finished',
                    finish_reason = 'manual_reset',
                    ended_at = ?1,
                    current_session_id = NULL,
                    accumulated_study_seconds = ?2,
                    last_control_device_id = ?3,
                    last_control_action = ?4,
                    last_control_at = ?5,
                    updated_at = ?1
                WHERE id = ?6 AND status = 'active'
                ",
                params![
                    now.to_rfc3339(),
                    study_elapsed_seconds(&record, now)?
                        .min(record.planned_seconds)
                        .max(0),
                    device_id,
                    CONTROL_FINISH,
                    now.timestamp_millis(),
                    record.id
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    set_runtime_state(state.inner(), false, None)?;
    trigger_shared_sync(&app, "focus_state_change");
    Ok(idle_study_mode_state())
}

pub fn tick_background_study_mode(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let before_marker = current_study_runtime_marker(app).ok().flatten();
    let study_state = advance_study_mode(app, state.inner())?;
    let after_marker = study_runtime_marker(&study_state);
    if before_marker != after_marker {
        let app = app.clone();
        thread::spawn(move || {
            let _ = crate::commands::sync::sync_object_storage_after_external_change(
                app,
                "focus_state_change",
            );
        });
    }
    if study_state.focus_enforcement_active {
        if let Some(session_id) = study_state
            .current_session
            .as_ref()
            .map(|session| session.id)
        {
            let _ = crate::commands::monitor::check_focus_foreground_app_for_session(
                app,
                state.inner(),
                session_id,
            );
        } else {
            let _ = crate::commands::monitor::check_focus_foreground_app_for_active_mode(
                app,
                state.inner(),
            );
        }
    }

    Ok(())
}

pub fn sync_study_runtime_state(app: &AppHandle) -> Result<(), String> {
    let connection = open_database(&database_path(app)?)?;
    let state = app.state::<AppState>();
    if let Some(record) = get_active_study_mode_record(&connection)? {
        set_runtime_state(state.inner(), true, record.current_session_id)?;
    } else {
        set_runtime_state(state.inner(), false, None)?;
    }

    Ok(())
}

fn current_study_runtime_marker(app: &AppHandle) -> Result<Option<StudyRuntimeSyncMarker>, String> {
    let connection = open_database(&database_path(app)?)?;
    let state = load_current_study_mode_state(&connection, Utc::now())?;
    Ok(study_runtime_marker(&state))
}

fn study_runtime_marker(state: &StudyModeState) -> Option<StudyRuntimeSyncMarker> {
    if state.id.is_none() && state.status == "idle" {
        return None;
    }

    Some(StudyRuntimeSyncMarker {
        id: state.id,
        state_revision: state.state_revision.unwrap_or(0).max(0),
        phase: state.phase.clone(),
        status: state.status.clone(),
        subject_id: state.subject_id,
        cycle_index: state.cycle_index,
        paused_at: state.paused_at.clone(),
        current_session_id: state.current_session.as_ref().map(|session| session.id),
        break_kind: state.break_kind.clone(),
    })
}

#[tauri::command]
pub fn finish_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let now = Utc::now().to_rfc3339();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'finished',
                end_reason = 'completed',
                updated_at = ?2
            WHERE id = ?3
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state
        .active_session_id
        .lock()
        .map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(session)
}

#[tauri::command]
pub fn emergency_exit_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;
    let session = get_focus_session_by_id(&connection, session_id)?;

    if session.status != "running" {
        return Err("只有进行中的严格模式专注可以应急退出".to_string());
    }

    if session.mode != "strict" {
        return Err("只有严格模式支持应急退出".to_string());
    }

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'emergency_exited',
                end_reason = 'emergency_exit',
                emergency_exit_count = emergency_exit_count + 1,
                updated_at = ?2
            WHERE id = ?3
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let updated_session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state
        .active_session_id
        .lock()
        .map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(updated_session)
}

#[tauri::command]
pub fn interrupt_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    actual_seconds: i64,
) -> Result<FocusSession, String> {
    let now = Utc::now().to_rfc3339();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'interrupted',
                end_reason = 'user_marked_interrupted',
                updated_at = ?2
            WHERE id = ?3 AND status = 'running'
            ",
            params![actual_seconds.max(0), now, session_id],
        )
        .map_err(|error| error.to_string())?;

    let session = get_focus_session_by_id(&connection, session_id)?;
    let mut active_session_id = state
        .active_session_id
        .lock()
        .map_err(|error| error.to_string())?;
    if *active_session_id == Some(session_id) {
        *active_session_id = None;
    }
    Ok(session)
}

#[tauri::command]
pub fn recover_active_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<FocusSessionRecovery>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let session = connection
        .query_row(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            WHERE status = 'running'
            ORDER BY id DESC
            LIMIT 1
            ",
            [],
            row_to_focus_session,
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some(session) = session else {
        *state
            .active_session_id
            .lock()
            .map_err(|error| error.to_string())? = None;
        return Ok(None);
    };

    let started_at = DateTime::parse_from_rfc3339(&session.started_at)
        .map_err(|error| error.to_string())?
        .with_timezone(&Utc);
    let elapsed_seconds = (Utc::now() - started_at).num_seconds().max(0);

    *state
        .active_session_id
        .lock()
        .map_err(|error| error.to_string())? = Some(session.id);

    Ok(Some(FocusSessionRecovery {
        recovery_status: "resumed".to_string(),
        remaining_seconds: (session.planned_seconds - elapsed_seconds).max(0),
        session,
        elapsed_seconds,
    }))
}

#[tauri::command]
pub fn list_focus_sessions(app: AppHandle) -> Result<Vec<FocusSession>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            WHERE status != 'running' AND actual_seconds >= ?1
            ORDER BY id DESC
            LIMIT 20
            ",
        )
        .map_err(|error| error.to_string())?;

    let sessions = statement
        .query_map(params![MIN_RECORDED_FOCUS_SECONDS], row_to_focus_session)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(sessions)
}

#[tauri::command]
pub fn delete_focus_session(app: AppHandle, session_id: i64) -> Result<(), String> {
    let mut connection = open_database(&database_path(&app)?)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let now = Utc::now().timestamp_millis();

    let mut event_ids_statement = transaction
        .prepare("SELECT id FROM app_events WHERE session_id = ?1")
        .map_err(|error| error.to_string())?;
    let event_ids = event_ids_statement
        .query_map(params![session_id], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(event_ids_statement);

    for event_id in event_ids {
        mark_entity_deleted(&transaction, "app_event", event_id, now)?;
    }
    mark_entity_deleted(&transaction, "focus_session", session_id, now)?;

    transaction
        .execute(
            "
            UPDATE study_modes
            SET current_session_id = NULL,
                state_revision = state_revision + 1,
                updated_at = ?1
            WHERE current_session_id = ?2
            ",
            params![Utc::now().to_rfc3339(), session_id],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "
            DELETE FROM app_events
            WHERE session_id = ?1
            ",
            params![session_id],
        )
        .map_err(|error| error.to_string())?;

    let deleted = transaction
        .execute(
            "
            DELETE FROM focus_sessions
            WHERE id = ?1 AND status != 'running'
            ",
            params![session_id],
        )
        .map_err(|error| error.to_string())?;

    if deleted == 0 {
        return Err("学习记录不存在，或仍在进行中无法删除".to_string());
    }

    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_focus_session_subject(
    app: AppHandle,
    session_id: i64,
    subject_id: Option<i64>,
) -> Result<FocusSession, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    validate_subject_id(&connection, subject_id)?;

    let now = Utc::now().to_rfc3339();
    let changed = connection
        .execute(
            "
            UPDATE focus_sessions
            SET subject_id = ?1,
                updated_at = ?2
            WHERE id = ?3
            ",
            params![subject_id, now, session_id],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("专注记录不存在".to_string());
    }

    let session = get_focus_session_by_id(&connection, session_id)?;
    trigger_shared_sync(&app, "focus_history_change");
    Ok(session)
}

#[tauri::command]
pub fn list_subjects(app: AppHandle) -> Result<Vec<Subject>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, name, color, enabled, created_at, updated_at
            FROM subjects
            WHERE enabled = 1
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let subjects = statement
        .query_map([], row_to_subject)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(subjects)
}

#[tauri::command]
pub fn get_focus_stats_summary(app: AppHandle) -> Result<FocusStatsSummary, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;
    let now = Utc::now();
    let today_start = shanghai_day_start(now)?;
    let week_start = shanghai_week_start(now)?;
    let month_start = shanghai_month_start(now)?;
    let tomorrow_start = today_start + Duration::days(1);
    let sessions = load_focus_session_stats_rows(&connection)?;
    let active = get_active_study_mode_record(&connection)?;

    let today_seconds = sum_session_seconds_for_window(
        &sessions,
        active.as_ref(),
        today_start,
        tomorrow_start,
        now,
    )?;
    let week_seconds = sum_session_seconds_for_window(
        &sessions,
        active.as_ref(),
        week_start,
        now + Duration::seconds(1),
        now,
    )?;
    let month_seconds = sum_session_seconds_for_window(
        &sessions,
        active.as_ref(),
        month_start,
        now + Duration::seconds(1),
        now,
    )?;
    let interruption_count = total_interruptions(&connection)?;
    let subjects = subject_stats(&connection, &sessions, active.as_ref(), now)?;

    Ok(FocusStatsSummary {
        today_seconds,
        week_seconds,
        month_seconds,
        interruption_count,
        subjects,
    })
}

fn total_interruptions(connection: &rusqlite::Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(interruption_count), 0) FROM focus_sessions",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn subject_stats(
    connection: &rusqlite::Connection,
    sessions: &[FocusSessionStatsRow],
    active: Option<&StudyModeRecord>,
    now: DateTime<Utc>,
) -> Result<Vec<SubjectStats>, String> {
    let mut totals = std::collections::HashMap::<Option<i64>, i64>::new();
    for session in sessions {
        let seconds = effective_session_seconds(session, active, now)?;
        if seconds > 0 {
            *totals.entry(session.subject_id).or_default() += seconds;
        }
    }

    let mut statement = connection
        .prepare(
            "
            SELECT s.id, s.name, s.color, s.enabled, s.created_at, s.updated_at
            FROM subjects s
            WHERE s.enabled = 1
            ORDER BY s.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let stats = statement
        .query_map([], |row| {
            let subject_id = row.get::<_, i64>(0)?;
            Ok(SubjectStats {
                subject: Subject {
                    id: subject_id,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    enabled: {
                        let enabled: i64 = row.get(3)?;
                        enabled != 0
                    },
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                },
                total_seconds: totals.get(&Some(subject_id)).copied().unwrap_or(0),
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(stats)
}

#[derive(Debug, Clone)]
struct FocusSessionStatsRow {
    id: i64,
    subject_id: Option<i64>,
    actual_seconds: i64,
    started_at: String,
    ended_at: Option<String>,
    status: String,
}

fn load_focus_session_stats_rows(
    connection: &rusqlite::Connection,
) -> Result<Vec<FocusSessionStatsRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, subject_id, planned_seconds, actual_seconds, started_at, ended_at, status
            FROM focus_sessions
            ORDER BY started_at ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(FocusSessionStatsRow {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                actual_seconds: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                status: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(rows)
}

fn sum_session_seconds_for_window(
    sessions: &[FocusSessionStatsRow],
    active: Option<&StudyModeRecord>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<i64, String> {
    let mut total = 0;
    for session in sessions {
        total += session_seconds_in_window(session, active, window_start, window_end, now)?;
    }
    Ok(total)
}

fn session_seconds_in_window(
    session: &FocusSessionStatsRow,
    active: Option<&StudyModeRecord>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<i64, String> {
    let seconds = effective_session_seconds(session, active, now)?;
    if seconds <= 0 {
        return Ok(0);
    }
    let interval_start = parse_rfc3339(&session.started_at)?;
    let interval_end = effective_session_end(session, active, now)?;
    if interval_end <= interval_start
        || interval_end <= window_start
        || interval_start >= window_end
    {
        return Ok(0);
    }
    let overlap_start = interval_start.max(window_start);
    let overlap_end = interval_end.min(window_end);
    let wall_seconds = (interval_end - interval_start).num_seconds().max(1);
    let overlap_seconds = (overlap_end - overlap_start).num_seconds().max(0);
    if overlap_seconds >= wall_seconds {
        return Ok(seconds);
    }
    Ok(((seconds as f64) * (overlap_seconds as f64) / (wall_seconds as f64)).round() as i64)
}

fn effective_session_seconds(
    session: &FocusSessionStatsRow,
    active: Option<&StudyModeRecord>,
    now: DateTime<Utc>,
) -> Result<i64, String> {
    if session.status == "running" {
        if active.and_then(|record| record.current_session_id) == Some(session.id) {
            return active
                .map(|record| focus_session_actual_seconds(record, now))
                .unwrap_or(Ok(0));
        }
        return Ok(0);
    }
    Ok(session.actual_seconds.max(0))
}

fn effective_session_end(
    session: &FocusSessionStatsRow,
    active: Option<&StudyModeRecord>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, String> {
    if session.status == "running"
        && active.and_then(|record| record.current_session_id) == Some(session.id)
    {
        return Ok(now);
    }
    if let Some(ended_at) = session.ended_at.as_deref() {
        return parse_rfc3339(ended_at);
    }
    Ok(parse_rfc3339(&session.started_at)? + Duration::seconds(session.actual_seconds.max(0)))
}

fn shanghai_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid Shanghai offset")
}

fn shanghai_day_start(now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let offset = shanghai_offset();
    let local = now.with_timezone(&offset);
    offset
        .with_ymd_and_hms(local.year(), local.month(), local.day(), 0, 0, 0)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| "Unable to calculate Beijing day start.".to_string())
}

fn shanghai_week_start(now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let day_start = shanghai_day_start(now)?;
    Ok(day_start
        - Duration::days(
            now.with_timezone(&shanghai_offset())
                .weekday()
                .num_days_from_monday() as i64,
        ))
}

fn shanghai_month_start(now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let offset = shanghai_offset();
    let local = now.with_timezone(&offset);
    offset
        .with_ymd_and_hms(local.year(), local.month(), 1, 0, 0, 0)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| "Unable to calculate Beijing month start.".to_string())
}

fn advance_study_mode(app: &AppHandle, state: &AppState) -> Result<StudyModeState, String> {
    let connection = open_database(&database_path(app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state, false, None)?;
        return load_current_study_mode_state(&connection, Utc::now());
    };

    let now = Utc::now();
    if record.paused_at.is_some() {
        set_runtime_state(state, true, record.current_session_id)?;
        return study_mode_record_to_state(&connection, &record, now);
    }

    match record.phase.as_str() {
        "focus" => {
            if record.current_session_id.is_none() {
                let session = insert_focus_session(
                    &connection,
                    record.focus_seconds,
                    &record.mode,
                    record.subject_id,
                    &now.to_rfc3339(),
                )?;
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET current_session_id = ?1,
                            state_revision = state_revision + 1,
                            phase_started_at = ?2,
                            paused_stage_elapsed_seconds = 0,
                            phase_paused_seconds = 0,
                            updated_at = ?2
                        WHERE id = ?3 AND status = 'active'
                        ",
                        params![session.id, now.to_rfc3339(), record.id],
                    )
                    .map_err(|error| error.to_string())?;
                set_runtime_state(state, true, Some(session.id))?;
                return load_current_study_mode_state(&connection, now);
            }

            let phase_elapsed_seconds = phase_elapsed_seconds(&record, now)?;
            let focus_run_seconds = record.focus_seconds.min(
                record
                    .planned_seconds
                    .saturating_sub(record.accumulated_study_seconds)
                    .max(0),
            );
            if focus_run_seconds <= 0 {
                complete_study_mode_record(&connection, state, &record, now, "completed")?;
                return load_study_mode_state_by_id(&connection, record.id, now);
            }
            if phase_elapsed_seconds >= focus_run_seconds {
                let focus_end =
                    parse_rfc3339(&record.phase_started_at)? + Duration::seconds(focus_run_seconds);
                let next_accumulated = (record.accumulated_study_seconds + focus_run_seconds)
                    .min(record.planned_seconds);
                if next_accumulated >= record.planned_seconds {
                    complete_study_mode_record_at(
                        &connection,
                        state,
                        &record,
                        focus_end,
                        "completed",
                        Some(next_accumulated),
                    )?;
                    return load_study_mode_state_by_id(&connection, record.id, now);
                }
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET phase = 'awaiting_break',
                            state_revision = state_revision + 1,
                            phase_started_at = ?1,
                            accumulated_study_seconds = ?2,
                            phase_paused_seconds = 0,
                            paused_stage_elapsed_seconds = 0,
                            updated_at = ?3
                        WHERE id = ?4 AND status = 'active'
                        ",
                        params![
                            focus_end.to_rfc3339(),
                            next_accumulated,
                            now.to_rfc3339(),
                            record.id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        "awaiting_break" => {
            let phase_elapsed_seconds = phase_elapsed_seconds(&record, now)?;
            let remaining_study_seconds = record
                .planned_seconds
                .saturating_sub(record.accumulated_study_seconds)
                .max(0);
            if remaining_study_seconds <= 0 || phase_elapsed_seconds >= remaining_study_seconds {
                let completed_at = parse_rfc3339(&record.phase_started_at)?
                    + Duration::seconds(remaining_study_seconds);
                complete_study_mode_record_at(
                    &connection,
                    state,
                    &record,
                    completed_at,
                    "completed",
                    Some(record.planned_seconds),
                )?;
                return load_study_mode_state_by_id(&connection, record.id, now);
            }
        }
        "break" => {
            let phase_elapsed_seconds = phase_elapsed_seconds(&record, now)?;
            if phase_elapsed_seconds >= effective_break_seconds(&record) {
                let next_started_at = parse_rfc3339(&record.phase_started_at)?
                    + Duration::seconds(effective_break_seconds(&record));
                let session = insert_focus_session(
                    &connection,
                    record.focus_seconds,
                    &record.mode,
                    record.subject_id,
                    &next_started_at.to_rfc3339(),
                )?;
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET phase = 'focus',
                            state_revision = state_revision + 1,
                            phase_started_at = ?1,
                            cycle_index = cycle_index + 1,
                            phase_paused_seconds = 0,
                            paused_stage_elapsed_seconds = 0,
                            current_session_id = ?2,
                            updated_at = ?4
                        WHERE id = ?3 AND status = 'active'
                        ",
                        params![
                            next_started_at.to_rfc3339(),
                            session.id,
                            record.id,
                            now.to_rfc3339()
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        _ => {}
    }

    let refreshed = get_active_study_mode_record(&connection)?;
    if let Some(refreshed) = refreshed {
        set_runtime_state(state, true, refreshed.current_session_id)?;
    } else {
        set_runtime_state(state, false, None)?;
    }

    load_current_study_mode_state(&connection, now)
}

fn complete_study_mode_record(
    connection: &Connection,
    state: &AppState,
    record: &StudyModeRecord,
    now: DateTime<Utc>,
    finish_reason: &str,
) -> Result<(), String> {
    complete_study_mode_record_at(connection, state, record, now, finish_reason, None)
}

fn complete_study_mode_record_at(
    connection: &Connection,
    state: &AppState,
    record: &StudyModeRecord,
    now: DateTime<Utc>,
    finish_reason: &str,
    accumulated_override: Option<i64>,
) -> Result<(), String> {
    if let Some(session_id) = record.current_session_id {
        if let Ok(session) = get_focus_session_by_id(connection, session_id) {
            if session.status == "running" {
                let actual_seconds = focus_session_actual_seconds(record, now)?;
                finish_running_focus_session(
                    connection,
                    session_id,
                    actual_seconds,
                    now,
                    "completed",
                )?;
            }
        }
    }

    let _ = crate::commands::schedule::mark_schedule_block_completed(
        connection,
        record.schedule_block_id,
        record.id,
        record.current_session_id,
        &now.to_rfc3339(),
    );

    connection
        .execute(
            "
            UPDATE study_modes
            SET phase = 'finished',
                state_revision = state_revision + 1,
                status = 'finished',
                finish_reason = ?1,
                ended_at = ?2,
                current_session_id = NULL,
                accumulated_study_seconds = ?3,
                paused_stage_elapsed_seconds = 0,
                phase_paused_seconds = 0,
                updated_at = ?2
            WHERE id = ?4 AND status = 'active'
            ",
            params![
                finish_reason,
                now.to_rfc3339(),
                accumulated_override
                    .unwrap_or_else(|| study_elapsed_seconds(record, now)
                        .unwrap_or(record.accumulated_study_seconds))
                    .min(record.planned_seconds)
                    .max(0),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state, false, None)
}

fn load_current_study_mode_state(
    connection: &Connection,
    now: DateTime<Utc>,
) -> Result<StudyModeState, String> {
    if let Some(record) = get_active_study_mode_record(connection)? {
        return study_mode_record_to_state(connection, &record, now);
    }

    if let Some(record) = get_latest_study_mode_record(connection)? {
        return study_mode_record_to_state(connection, &record, now);
    }

    Ok(idle_study_mode_state())
}

fn load_study_mode_state_by_id(
    connection: &Connection,
    study_mode_id: i64,
    now: DateTime<Utc>,
) -> Result<StudyModeState, String> {
    let record = get_study_mode_record_by_id(connection, study_mode_id)?;
    study_mode_record_to_state(connection, &record, now)
}

fn idle_study_mode_state() -> StudyModeState {
    StudyModeState {
        id: None,
        state_revision: None,
        phase: "idle".to_string(),
        status: "idle".to_string(),
        mode: "normal".to_string(),
        subject_id: None,
        planned_seconds: 0,
        focus_seconds: 0,
        break_seconds: 0,
        long_break_seconds: 0,
        long_break_interval: 4,
        effective_break_seconds: 0,
        break_kind: "short".to_string(),
        cycle_index: 0,
        started_at: None,
        phase_started_at: None,
        paused_at: None,
        ended_at: None,
        current_session: None,
        study_elapsed_seconds: 0,
        study_remaining_seconds: 0,
        phase_elapsed_seconds: 0,
        phase_remaining_seconds: 0,
        focus_enforcement_active: false,
        whitelist_enabled: true,
        is_paused: false,
    }
}

fn study_mode_record_to_state(
    connection: &Connection,
    record: &StudyModeRecord,
    now: DateTime<Utc>,
) -> Result<StudyModeState, String> {
    let current_session = record
        .current_session_id
        .and_then(|session_id| get_focus_session_by_id(connection, session_id).ok());
    let study_elapsed_seconds = if record.status == "active" {
        study_elapsed_seconds(record, now)?
    } else {
        record.accumulated_study_seconds.max(0)
    };
    let phase_elapsed_seconds = if record.status == "active" {
        phase_elapsed_seconds(record, now)?
    } else {
        0
    };
    let break_kind = break_kind_for_cycle(record.cycle_index, record.long_break_interval);
    let effective_break_seconds = effective_break_seconds(record);
    let focus_run_seconds = record.focus_seconds.min(
        record
            .planned_seconds
            .saturating_sub(record.accumulated_study_seconds)
            .max(0),
    );
    let phase_remaining_seconds = match record.phase.as_str() {
        "focus" => (focus_run_seconds - phase_elapsed_seconds).max(0),
        "awaiting_break" => 0,
        "break" => (effective_break_seconds - phase_elapsed_seconds).max(0),
        _ => 0,
    };
    let whitelist_enabled = record.mode == "strict" || record.whitelist_enabled;
    let focus_enforcement_active = whitelist_enabled
        && record.status == "active"
        && matches!(record.phase.as_str(), "focus" | "awaiting_break");

    Ok(StudyModeState {
        id: Some(record.id),
        state_revision: Some(record.state_revision.max(0)),
        phase: record.phase.clone(),
        status: record.status.clone(),
        mode: record.mode.clone(),
        subject_id: record.subject_id,
        planned_seconds: record.planned_seconds,
        focus_seconds: record.focus_seconds,
        break_seconds: record.break_seconds,
        long_break_seconds: record.long_break_seconds,
        long_break_interval: record.long_break_interval,
        effective_break_seconds,
        break_kind: break_kind.to_string(),
        cycle_index: record.cycle_index,
        started_at: Some(record.started_at.clone()),
        phase_started_at: Some(record.phase_started_at.clone()),
        paused_at: record.paused_at.clone(),
        ended_at: record.ended_at.clone(),
        current_session,
        study_elapsed_seconds,
        study_remaining_seconds: (record.planned_seconds - study_elapsed_seconds).max(0),
        phase_elapsed_seconds,
        phase_remaining_seconds,
        focus_enforcement_active,
        whitelist_enabled,
        is_paused: record.paused_at.is_some(),
    })
}

fn get_active_study_mode_record(
    connection: &Connection,
) -> Result<Option<StudyModeRecord>, String> {
    connection
        .query_row(
            "
            SELECT id, state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, whitelist_enabled, status
            FROM study_modes
            WHERE status = 'active'
            ORDER BY state_revision DESC, updated_at DESC, id DESC
            LIMIT 1
            ",
            [],
            row_to_study_mode_record,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_latest_study_mode_record(
    connection: &Connection,
) -> Result<Option<StudyModeRecord>, String> {
    connection
        .query_row(
            "
            SELECT id, state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, whitelist_enabled, status
            FROM study_modes
            ORDER BY state_revision DESC, updated_at DESC, id DESC
            LIMIT 1
            ",
            [],
            row_to_study_mode_record,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_study_mode_record_by_id(
    connection: &Connection,
    id: i64,
) -> Result<StudyModeRecord, String> {
    connection
        .query_row(
            "
            SELECT id, state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, whitelist_enabled, status
            FROM study_modes
            WHERE id = ?1
            ",
            params![id],
            row_to_study_mode_record,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "学习模式记录不存在".to_string())
}

fn insert_focus_session(
    connection: &Connection,
    planned_seconds: i64,
    mode: &str,
    subject_id: Option<i64>,
    now: &str,
) -> Result<FocusSession, String> {
    connection
        .execute(
            "
            INSERT INTO focus_sessions (
              mode,
              subject_id,
              planned_seconds,
              actual_seconds,
              started_at,
              status,
              interruption_count,
              emergency_exit_count,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, 0, ?4, 'running', 0, 0, ?4, ?4)
            ",
            params![mode, subject_id, planned_seconds, now],
        )
        .map_err(|error| error.to_string())?;

    get_focus_session_by_id(connection, connection.last_insert_rowid())
}

fn finish_running_focus_session(
    connection: &Connection,
    session_id: i64,
    actual_seconds: i64,
    now: DateTime<Utc>,
    end_reason: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'finished',
                end_reason = ?3,
                updated_at = ?2
            WHERE id = ?4 AND status = 'running'
            ",
            params![
                actual_seconds.max(0),
                now.to_rfc3339(),
                end_reason,
                session_id
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn emergency_exit_running_focus_session(
    connection: &Connection,
    session_id: i64,
    actual_seconds: i64,
    now: DateTime<Utc>,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE focus_sessions
            SET actual_seconds = ?1,
                ended_at = ?2,
                status = 'emergency_exited',
                end_reason = 'emergency_exit',
                emergency_exit_count = emergency_exit_count + 1,
                updated_at = ?2
            WHERE id = ?3 AND status = 'running'
            ",
            params![actual_seconds.max(0), now.to_rfc3339(), session_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn set_runtime_state(
    state: &AppState,
    study_mode_active: bool,
    active_session_id: Option<i64>,
) -> Result<(), String> {
    *state
        .study_mode_active
        .lock()
        .map_err(|error| error.to_string())? = study_mode_active;
    *state
        .active_session_id
        .lock()
        .map_err(|error| error.to_string())? = active_session_id;
    Ok(())
}

fn study_elapsed_seconds(record: &StudyModeRecord, now: DateTime<Utc>) -> Result<i64, String> {
    let current_phase_seconds = match record.phase.as_str() {
        "focus" | "awaiting_break" => phase_elapsed_seconds(record, now)?,
        _ => 0,
    };
    Ok((record.accumulated_study_seconds + current_phase_seconds)
        .min(record.planned_seconds)
        .max(0))
}

fn phase_elapsed_seconds(record: &StudyModeRecord, now: DateTime<Utc>) -> Result<i64, String> {
    if record.paused_at.is_some() {
        return Ok(record.paused_stage_elapsed_seconds.max(0));
    }
    Ok(seconds_since(&record.phase_started_at, now)?.max(0))
}

fn focus_session_actual_seconds(
    record: &StudyModeRecord,
    now: DateTime<Utc>,
) -> Result<i64, String> {
    let remaining_total = record
        .planned_seconds
        .saturating_sub(record.accumulated_study_seconds)
        .max(0);
    let phase_elapsed = phase_elapsed_seconds(record, now)?;
    let actual_seconds = match record.phase.as_str() {
        "awaiting_break" => record.focus_seconds + phase_elapsed.min(remaining_total),
        _ => phase_elapsed.min(record.focus_seconds).min(remaining_total),
    };
    Ok(actual_seconds.max(0))
}

fn seconds_since(started_at: &str, now: DateTime<Utc>) -> Result<i64, String> {
    Ok((now - parse_rfc3339(started_at)?).num_seconds().max(0))
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|error| error.to_string())?
        .with_timezone(&Utc))
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn get_focus_session_by_id(
    connection: &Connection,
    session_id: i64,
) -> Result<FocusSession, String> {
    connection
        .query_row(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count,
                   created_at, updated_at
            FROM focus_sessions
            WHERE id = ?1
            ",
            params![session_id],
            row_to_focus_session,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "专注记录不存在".to_string())
}

fn validate_subject_id(connection: &Connection, subject_id: Option<i64>) -> Result<(), String> {
    if let Some(subject_id) = subject_id {
        let subject_exists = connection
            .query_row(
                "SELECT 1 FROM subjects WHERE id = ?1 AND enabled = 1",
                params![subject_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();

        if !subject_exists {
            return Err("科目不存在或已停用".to_string());
        }
    }

    Ok(())
}

fn break_kind_for_cycle(cycle_index: i64, long_break_interval: i64) -> &'static str {
    if long_break_interval > 0 && cycle_index > 0 && cycle_index % long_break_interval == 0 {
        "long"
    } else {
        "short"
    }
}

fn effective_break_seconds(record: &StudyModeRecord) -> i64 {
    if break_kind_for_cycle(record.cycle_index, record.long_break_interval) == "long" {
        record.long_break_seconds
    } else {
        record.break_seconds
    }
}

fn row_to_study_mode_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudyModeRecord> {
    Ok(StudyModeRecord {
        id: row.get(0)?,
        state_revision: row.get(1)?,
        mode: row.get(2)?,
        subject_id: row.get(3)?,
        planned_seconds: row.get(4)?,
        focus_seconds: row.get(5)?,
        break_seconds: row.get(6)?,
        long_break_seconds: row.get(7)?,
        long_break_interval: row.get(8)?,
        phase: row.get(9)?,
        cycle_index: row.get(10)?,
        started_at: row.get(11)?,
        phase_started_at: row.get(12)?,
        paused_at: row.get(13)?,
        _total_paused_seconds: row.get(14)?,
        _phase_paused_seconds: row.get(15)?,
        accumulated_study_seconds: row.get(16)?,
        paused_stage_elapsed_seconds: row.get(17)?,
        ended_at: row.get(18)?,
        current_session_id: row.get(19)?,
        schedule_block_id: row.get(20)?,
        _today_plan_item_id: row.get(21)?,
        whitelist_enabled: row.get(22)?,
        status: row.get(23)?,
    })
}

fn row_to_focus_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<FocusSession> {
    Ok(FocusSession {
        id: row.get(0)?,
        mode: row.get(1)?,
        subject_id: row.get(2)?,
        planned_seconds: row.get(3)?,
        actual_seconds: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        status: row.get(7)?,
        end_reason: row.get(8)?,
        interruption_count: row.get(9)?,
        emergency_exit_count: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn row_to_subject(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subject> {
    let enabled: i64 = row.get(3)?;

    Ok(Subject {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        enabled: enabled != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
