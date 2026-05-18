use crate::{
    focus::{session::FocusSession, subject::Subject},
    storage::db::open_database,
    sync_package::mark_entity_deleted,
    AppState,
};
use chrono::{DateTime, Datelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::thread;
use tauri::{AppHandle, Manager, State};

const MIN_RECORDED_FOCUS_SECONDS: i64 = 60;

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
    pub is_paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StudyRuntimeSyncMarker {
    id: Option<i64>,
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
    mode: String,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    phase: String,
    cycle_index: i64,
    started_at: String,
    phase_started_at: String,
    paused_at: Option<String>,
    total_paused_seconds: i64,
    phase_paused_seconds: i64,
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

    let connection = open_database(&database_path(&app)?)?;
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
              phase,
              cycle_index,
              started_at,
              phase_started_at,
              current_session_id,
              status,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'focus', 1, ?8, ?8, ?9, 'active', ?8, ?8)
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
                session.id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), true, Some(session.id))?;
    load_current_study_mode_state(&connection, Utc::now())
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
    if get_active_study_mode_record(&connection)?.is_some() {
        return Err("A study mode is already running.".to_string());
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
              phase,
              cycle_index,
              started_at,
              phase_started_at,
              current_session_id,
              schedule_block_id,
              today_plan_item_id,
              status,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'focus', 1, ?8, ?8, ?9, ?10, ?11, 'active', ?8, ?8)
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
                links.today_plan_item_id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state, true, Some(session.id))?;
    load_current_study_mode_state(&connection, Utc::now())
}

#[tauri::command]
pub fn get_study_mode_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    advance_study_mode(&app, state.inner())
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
    if study_elapsed_seconds(&record, now)? >= record.planned_seconds {
        complete_study_mode_record(&connection, state.inner(), &record, now, "completed")?;
        return load_current_study_mode_state(&connection, now);
    }

    if let Some(session_id) = record.current_session_id {
        let actual_seconds = phase_elapsed_seconds(&record, now)?;
        finish_running_focus_session(
            &connection,
            session_id,
            actual_seconds,
            now,
            "pomodoro_completed",
        )?;
    }

    connection
        .execute(
            "
            UPDATE study_modes
            SET phase = 'break',
                phase_started_at = ?1,
                phase_paused_seconds = 0,
                paused_at = NULL,
                current_session_id = NULL,
                updated_at = ?1
            WHERE id = ?2 AND status = 'active'
            ",
            params![now.to_rfc3339(), record.id],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), true, None)?;
    load_current_study_mode_state(&connection, now)
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

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE study_modes
            SET paused_at = ?1,
                updated_at = ?1
            WHERE id = ?2 AND status = 'active'
            ",
            params![now, record.id],
        )
        .map_err(|error| error.to_string())?;

    load_current_study_mode_state(&connection, Utc::now())
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
    let paused_seconds = seconds_since(paused_at, now)?;
    connection
        .execute(
            "
            UPDATE study_modes
            SET paused_at = NULL,
                total_paused_seconds = total_paused_seconds + ?1,
                phase_paused_seconds = phase_paused_seconds + ?1,
                updated_at = ?2
            WHERE id = ?3 AND status = 'active'
            ",
            params![paused_seconds, now.to_rfc3339(), record.id],
        )
        .map_err(|error| error.to_string())?;

    advance_study_mode(&app, state.inner())
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

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE study_modes
            SET subject_id = ?1,
                updated_at = ?2
            WHERE id = ?3 AND status = 'active'
            ",
            params![subject_id, now, record.id],
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

    load_current_study_mode_state(&connection, Utc::now())
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
    if let Some(session_id) = record.current_session_id {
        if let Ok(session) = get_focus_session_by_id(&connection, session_id) {
            if session.status == "running" {
                let actual_seconds = phase_elapsed_seconds(&record, now)?;
                emergency_exit_running_focus_session(&connection, session_id, actual_seconds, now)?;
            }
        }
    }

    connection
        .execute(
            "
            UPDATE study_modes
            SET phase = 'emergency_exited',
                status = 'emergency_exited',
                finish_reason = 'emergency_exit',
                ended_at = ?1,
                current_session_id = NULL,
                updated_at = ?1
            WHERE id = ?2 AND status = 'active'
            ",
            params![now.to_rfc3339(), record.id],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), false, None)?;
    load_current_study_mode_state(&connection, now)
}

#[tauri::command]
pub fn reset_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let connection = open_database(&database_path(&app)?)?;
    if let Some(record) = get_active_study_mode_record(&connection)? {
        let now = Utc::now();
        if let Some(session_id) = record.current_session_id {
            if let Ok(session) = get_focus_session_by_id(&connection, session_id) {
                if session.status == "running" {
                    let actual_seconds = phase_elapsed_seconds(&record, now)?;
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
                    status = 'finished',
                    finish_reason = 'manual_reset',
                    ended_at = ?1,
                    current_session_id = NULL,
                    updated_at = ?1
                WHERE id = ?2 AND status = 'active'
                ",
                params![now.to_rfc3339(), record.id],
            )
            .map_err(|error| error.to_string())?;
    }

    set_runtime_state(state.inner(), false, None)?;
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
            let _ = crate::commands::sync::auto_sync_object_storage_database_blocking(app);
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

    get_focus_session_by_id(&connection, session_id)
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
    let today_prefix = now.format("%Y-%m-%d").to_string();
    let week_start =
        now.date_naive() - chrono::Days::new(now.weekday().num_days_from_monday() as u64);
    let month_prefix = now.format("%Y-%m").to_string();

    let today_seconds = sum_seconds_by_like(&connection, &today_prefix)?;
    let week_seconds = sum_seconds_since(&connection, &week_start.format("%Y-%m-%d").to_string())?;
    let month_seconds = sum_seconds_by_like(&connection, &month_prefix)?;
    let interruption_count = total_interruptions(&connection)?;
    let subjects = subject_stats(&connection)?;

    Ok(FocusStatsSummary {
        today_seconds,
        week_seconds,
        month_seconds,
        interruption_count,
        subjects,
    })
}

fn sum_seconds_by_like(connection: &rusqlite::Connection, prefix: &str) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished' AND actual_seconds >= ?2 AND started_at LIKE ?1 || '%'
            ",
            params![prefix, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn sum_seconds_since(connection: &rusqlite::Connection, date_prefix: &str) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT COALESCE(SUM(actual_seconds), 0)
            FROM focus_sessions
            WHERE status = 'finished' AND actual_seconds >= ?2 AND started_at >= ?1
            ",
            params![date_prefix, MIN_RECORDED_FOCUS_SECONDS],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
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

fn subject_stats(connection: &rusqlite::Connection) -> Result<Vec<SubjectStats>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT s.id, s.name, s.color, s.enabled, s.created_at, s.updated_at,
                   COALESCE(SUM(f.actual_seconds), 0) AS total_seconds
            FROM subjects s
            LEFT JOIN focus_sessions f
              ON f.subject_id = s.id
             AND f.status = 'finished'
             AND f.actual_seconds >= ?1
            WHERE s.enabled = 1
            GROUP BY s.id, s.name, s.color, s.enabled, s.created_at, s.updated_at
            ORDER BY s.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let stats = statement
        .query_map(params![MIN_RECORDED_FOCUS_SECONDS], |row| {
            Ok(SubjectStats {
                subject: Subject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    enabled: {
                        let enabled: i64 = row.get(3)?;
                        enabled != 0
                    },
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                },
                total_seconds: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(stats)
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

    let study_elapsed_seconds = study_elapsed_seconds(&record, now)?;
    if study_elapsed_seconds >= record.planned_seconds {
        complete_study_mode_record(&connection, state, &record, now, "completed")?;
        return load_study_mode_state_by_id(&connection, record.id, now);
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
                            phase_started_at = ?2,
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
            if phase_elapsed_seconds >= record.focus_seconds {
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET phase = 'awaiting_break',
                            updated_at = ?1
                        WHERE id = ?2 AND status = 'active'
                        ",
                        params![now.to_rfc3339(), record.id],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        "awaiting_break" => {}
        "break" => {
            let phase_elapsed_seconds = phase_elapsed_seconds(&record, now)?;
            if phase_elapsed_seconds >= effective_break_seconds(&record) {
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
                        SET phase = 'focus',
                            phase_started_at = ?1,
                            cycle_index = cycle_index + 1,
                            phase_paused_seconds = 0,
                            current_session_id = ?2,
                            updated_at = ?1
                        WHERE id = ?3 AND status = 'active'
                        ",
                        params![now.to_rfc3339(), session.id, record.id],
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
    if let Some(session_id) = record.current_session_id {
        if let Ok(session) = get_focus_session_by_id(connection, session_id) {
            if session.status == "running" {
                let actual_seconds = phase_elapsed_seconds(record, now)?;
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
                status = 'finished',
                finish_reason = ?1,
                ended_at = ?2,
                current_session_id = NULL,
                updated_at = ?2
            WHERE id = ?3 AND status = 'active'
            ",
            params![finish_reason, now.to_rfc3339(), record.id],
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
    let effective_now = if record.status == "active" {
        now
    } else if let Some(ended_at) = record.ended_at.as_deref() {
        parse_rfc3339(ended_at)?
    } else {
        now
    };
    let study_elapsed_seconds = if record.status == "active" {
        study_elapsed_seconds(record, now)?
    } else {
        seconds_since(&record.started_at, effective_now)?
            .saturating_sub(record.total_paused_seconds)
    };
    let phase_elapsed_seconds = if record.status == "active" {
        phase_elapsed_seconds(record, now)?
    } else {
        0
    };
    let break_kind = break_kind_for_cycle(record.cycle_index, record.long_break_interval);
    let effective_break_seconds = effective_break_seconds(record);
    let phase_remaining_seconds = match record.phase.as_str() {
        "focus" => (record.focus_seconds - phase_elapsed_seconds).max(0),
        "awaiting_break" => 0,
        "break" => (effective_break_seconds - phase_elapsed_seconds).max(0),
        _ => 0,
    };
    let focus_enforcement_active = record.status == "active"
        && matches!(record.phase.as_str(), "focus" | "awaiting_break")
        && current_session.is_some();

    Ok(StudyModeState {
        id: Some(record.id),
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
        is_paused: record.paused_at.is_some(),
    })
}

fn get_active_study_mode_record(
    connection: &Connection,
) -> Result<Option<StudyModeRecord>, String> {
    connection
        .query_row(
            "
            SELECT id, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, status
            FROM study_modes
            WHERE status = 'active'
            ORDER BY id DESC
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
            SELECT id, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, status
            FROM study_modes
            ORDER BY id DESC
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
            SELECT id, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval,
                   phase, cycle_index, started_at, phase_started_at, paused_at,
                   total_paused_seconds, phase_paused_seconds, ended_at,
                   current_session_id, schedule_block_id, today_plan_item_id, status
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
    Ok(seconds_since(&record.started_at, now)?
        .saturating_sub(record.total_paused_seconds)
        .saturating_sub(open_pause_seconds(record, now)?))
}

fn phase_elapsed_seconds(record: &StudyModeRecord, now: DateTime<Utc>) -> Result<i64, String> {
    Ok(seconds_since(&record.phase_started_at, now)?
        .saturating_sub(record.phase_paused_seconds)
        .saturating_sub(open_pause_seconds(record, now)?))
}

fn open_pause_seconds(record: &StudyModeRecord, now: DateTime<Utc>) -> Result<i64, String> {
    if let Some(paused_at) = record.paused_at.as_deref() {
        seconds_since(paused_at, now)
    } else {
        Ok(0)
    }
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
        mode: row.get(1)?,
        subject_id: row.get(2)?,
        planned_seconds: row.get(3)?,
        focus_seconds: row.get(4)?,
        break_seconds: row.get(5)?,
        long_break_seconds: row.get(6)?,
        long_break_interval: row.get(7)?,
        phase: row.get(8)?,
        cycle_index: row.get(9)?,
        started_at: row.get(10)?,
        phase_started_at: row.get(11)?,
        paused_at: row.get(12)?,
        total_paused_seconds: row.get(13)?,
        phase_paused_seconds: row.get(14)?,
        ended_at: row.get(15)?,
        current_session_id: row.get(16)?,
        schedule_block_id: row.get(17)?,
        _today_plan_item_id: row.get(18)?,
        status: row.get(19)?,
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
