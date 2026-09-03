#[tauri::command]
#[allow(clippy::too_many_arguments)]
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
    let mut connection = open_database(&database_path(&app)?)?;
    ensure_current_device_can_start_study_mode(&connection)?;
    if get_active_study_mode_record(&connection)?.is_some() {
        return Err("已有学习模式正在进行中".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let session_id = {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let session = insert_focus_session(&transaction, focus_seconds, &mode, subject_id, &now)?;
        transaction
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
        transaction.commit().map_err(|error| error.to_string())?;
        session.id
    };

    set_runtime_state(state.inner(), true, Some(session_id))?;
    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_study_mode_with_links_on_connection(
    connection: &mut Connection,
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
    ensure_current_device_can_start_study_mode(connection)?;
    if get_active_study_mode_record(connection)?.is_some() {
        return Err("A study mode is already running.".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let session_id = {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let session = insert_focus_session(&transaction, focus_seconds, &mode, subject_id, &now)?;
        link_focus_session_to_study_source(&transaction, session.id, links)?;
        let whitelist_enabled = true;
        transaction
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
        transaction.commit().map_err(|error| error.to_string())?;
        session.id
    };

    set_runtime_state(state, true, Some(session_id))?;
    let next_state = load_current_study_mode_state(connection, Utc::now())?;
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
        sync_focus_widget_for_state(&app, &current_state);
        return Ok(current_state);
    }

    if current_state.phase != "awaiting_break" {
        return Err("当前还没有到休息确认时间".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
    };

    if record.paused_at.is_some() {
        return Err("学习模式暂停中，请先继续再开始休息".to_string());
    }

    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    if record.planned_seconds > 0 && study_elapsed_seconds(&record, now)? >= record.planned_seconds {
        complete_study_mode_record(&connection, state.inner(), &record, now, "completed")?;
        let next_state = load_current_study_mode_state(&connection, now)?;
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
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
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn skip_study_break(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StudyModeState, String> {
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        return Ok(current_state);
    }
    if !matches!(current_state.phase.as_str(), "awaiting_break" | "break") {
        return Err("当前不是可跳过的休息阶段".to_string());
    }
    if current_state.is_paused {
        return Err("学习模式暂停中，请先继续再跳过休息".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        return Err("没有正在运行的学习模式".to_string());
    };
    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    let next_accumulated = if record.phase == "awaiting_break" {
        let accumulated = study_elapsed_seconds(&record, now)?;
        if record.planned_seconds > 0 && accumulated >= record.planned_seconds {
            complete_study_mode_record(&connection, state.inner(), &record, now, "completed")?;
            return load_current_study_mode_state(&connection, now);
        }
        if let Some(session_id) = record.current_session_id {
            finish_running_focus_session(
                &connection,
                session_id,
                focus_session_actual_seconds(&record, now)?,
                now,
                if is_countup(&record) {
                    "manual_break"
                } else {
                    "pomodoro_completed"
                },
            )?;
        }
        accumulated
    } else {
        record.accumulated_study_seconds
    };
    let session = insert_focus_session(
        &connection,
        if is_countup(&record) { 0 } else { record.focus_seconds },
        &record.mode,
        record.subject_id,
        &now.to_rfc3339(),
    )?;
    connection.execute(
        "UPDATE study_modes SET phase = 'focus', state_revision = state_revision + 1,
         phase_started_at = ?1, cycle_index = cycle_index + 1, phase_paused_seconds = 0,
         paused_stage_elapsed_seconds = 0, paused_at = NULL, accumulated_study_seconds = ?2,
         current_session_id = ?3, last_control_device_id = ?4, last_control_action = ?5,
         last_control_at = ?6, updated_at = ?1 WHERE id = ?7 AND status = 'active'",
        params![now.to_rfc3339(), next_accumulated, session.id, device_id, CONTROL_SKIP_BREAK,
            now.timestamp_millis(), record.id],
    ).map_err(|error| error.to_string())?;
    set_runtime_state(state.inner(), true, Some(session.id))?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    sync_focus_widget_for_state(&app, &next_state);
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
        sync_focus_widget_for_state(&app, &current_state);
        return Ok(current_state);
    }

    if !matches!(current_state.phase.as_str(), "focus" | "awaiting_break") {
        return Err("休息阶段不能暂停".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
    };

    if record.paused_at.is_some() {
        let next_state = load_current_study_mode_state(&connection, Utc::now())?;
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
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
    sync_focus_widget_for_state(&app, &next_state);
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
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
    };

    let Some(paused_at) = record.paused_at.as_deref() else {
        let next_state = advance_study_mode(&app, state.inner())?;
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
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
    sync_focus_widget_for_state(&app, &next_state);
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
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
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
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn update_study_mode_whitelist(
    app: AppHandle,
    state: State<'_, AppState>,
    whitelist_enabled: bool,
) -> Result<StudyModeState, String> {
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        return Err("没有正在运行的学习模式".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        return Err("没有正在运行的学习模式".to_string());
    };

    if record.mode == "strict" {
        if !whitelist_enabled {
            return Err("强制模式下前台规则必须保持开启".to_string());
        }
        return load_current_study_mode_state(&connection, Utc::now());
    }

    if record.whitelist_enabled == whitelist_enabled {
        return load_current_study_mode_state(&connection, Utc::now());
    }

    // Foreground enforcement is device-local and is intentionally excluded from shared progress.
    connection
        .execute(
            "UPDATE study_modes SET whitelist_enabled = ?1 WHERE id = ?2 AND status = 'active'",
            params![whitelist_enabled, record.id],
        )
        .map_err(|error| error.to_string())?;

    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    sync_focus_widget_for_state(&app, &next_state);
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
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
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
                {
                    let raw = study_elapsed_seconds(&record, now)?;
                    if record.planned_seconds > 0 {
                        raw.min(record.planned_seconds)
                    } else {
                        raw
                    }
                    .max(0)
                },
                device_id,
                CONTROL_EMERGENCY_EXIT,
                now.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), false, None)?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    sync_focus_widget_for_state(&app, &next_state);
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
                    {
                        let raw = study_elapsed_seconds(&record, now)?;
                        if record.planned_seconds > 0 {
                            raw.min(record.planned_seconds)
                        } else {
                            raw
                        }
                        .max(0)
                    },
                    device_id,
                    CONTROL_FINISH,
                    now.timestamp_millis(),
                    record.id
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    set_runtime_state(state.inner(), false, None)?;
    let next_state = idle_study_mode_state();
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

fn validate_countup_break_seconds(break_seconds: i64) -> Result<(), String> {
    if !(COUNTUP_BREAK_MIN_SECONDS..=COUNTUP_BREAK_MAX_SECONDS).contains(&break_seconds) {
        return Err(format!(
            "休息时长需在 {} 到 {} 分钟之间",
            COUNTUP_BREAK_MIN_SECONDS / 60,
            COUNTUP_BREAK_MAX_SECONDS / 60
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn start_countup_study_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    break_seconds: i64,
    mode: String,
    subject_id: Option<i64>,
    whitelist_enabled: Option<bool>,
) -> Result<StudyModeState, String> {
    validate_countup_break_seconds(break_seconds)?;
    if mode != "normal" && mode != "strict" {
        return Err("未知的专注模式".to_string());
    }

    let whitelist_enabled = mode == "strict" || whitelist_enabled.unwrap_or(true);
    let mut connection = open_database(&database_path(&app)?)?;
    ensure_current_device_can_start_study_mode(&connection)?;
    if get_active_study_mode_record(&connection)?.is_some() {
        return Err("已有学习模式正在进行中".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let session_id = {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let session = insert_focus_session(&transaction, 0, &mode, subject_id, &now)?;
        transaction
            .execute(
                "
                INSERT INTO study_modes (
                  mode,
                  timer_kind,
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
                ) VALUES (?1, ?2, ?3, 0, 0, ?4, 900, 4, 1, 'focus', 1, ?5, ?5, 0, ?6, ?7, 'active', ?5, ?5)
                ",
                params![
                    mode,
                    TIMER_KIND_COUNTUP,
                    subject_id,
                    break_seconds,
                    now,
                    session.id,
                    whitelist_enabled
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        session.id
    };

    set_runtime_state(state.inner(), true, Some(session_id))?;
    let next_state = load_current_study_mode_state(&connection, Utc::now())?;
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
pub fn take_manual_break(
    app: AppHandle,
    state: State<'_, AppState>,
    break_seconds: i64,
) -> Result<StudyModeState, String> {
    validate_countup_break_seconds(break_seconds)?;
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        sync_focus_widget_for_state(&app, &current_state);
        return Ok(current_state);
    }
    if current_state.timer_kind != TIMER_KIND_COUNTUP {
        return Err("只有正计时模式支持手动开始休息".to_string());
    }
    if !matches!(current_state.phase.as_str(), "focus" | "awaiting_break") {
        return Err("当前阶段不能开始休息".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
    };
    if record.paused_at.is_some() {
        return Err("学习模式暂停中，请先继续再开始休息".to_string());
    }

    let now = Utc::now();
    let device_id = load_or_create_device_id(&connection)?;
    // 统一按已学时长累计：focus 计入本轮全部 elapsed，awaiting_break 再叠加等待期超时，与确认休息语义一致。
    let accumulated = study_elapsed_seconds(&record, now)?;

    if let Some(session_id) = record.current_session_id {
        let actual_seconds = focus_session_actual_seconds(&record, now)?;
        finish_running_focus_session(&connection, session_id, actual_seconds, now, "manual_break")?;
    }

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
                break_seconds = ?2,
                accumulated_study_seconds = ?3,
                current_session_id = NULL,
                last_control_device_id = ?4,
                last_control_action = ?5,
                last_control_at = ?6,
                updated_at = ?1
            WHERE id = ?7 AND status = 'active'
            ",
            params![
                now.to_rfc3339(),
                break_seconds,
                accumulated,
                device_id,
                CONTROL_MANUAL_BREAK,
                now.timestamp_millis(),
                record.id
            ],
        )
        .map_err(|error| error.to_string())?;

    set_runtime_state(state.inner(), true, None)?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn switch_study_timer_kind(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    planned_seconds: Option<i64>,
    focus_seconds: Option<i64>,
    break_seconds: Option<i64>,
    long_break_seconds: Option<i64>,
    long_break_interval: Option<i64>,
) -> Result<StudyModeState, String> {
    let current_state = advance_study_mode(&app, state.inner())?;
    if current_state.status != "active" {
        sync_focus_widget_for_state(&app, &current_state);
        return Ok(current_state);
    }
    if !matches!(current_state.phase.as_str(), "focus" | "awaiting_break") {
        return Err("休息与结束阶段不能切换计时模式".to_string());
    }

    let connection = open_database(&database_path(&app)?)?;
    let Some(record) = get_active_study_mode_record(&connection)? else {
        set_runtime_state(state.inner(), false, None)?;
        let next_state = idle_study_mode_state();
        sync_focus_widget_for_state(&app, &next_state);
        return Ok(next_state);
    };

    let now = Utc::now();
    let now_text = now.to_rfc3339();
    let device_id = load_or_create_device_id(&connection)?;

    if kind == TIMER_KIND_COUNTUP {
        connection
            .execute(
                "
                UPDATE study_modes
                SET timer_kind = ?1,
                    state_revision = state_revision + 1,
                    last_control_device_id = ?2,
                    last_control_action = ?3,
                    last_control_at = ?4,
                    updated_at = ?5
                WHERE id = ?6 AND status = 'active'
                ",
                params![
                    TIMER_KIND_COUNTUP,
                    device_id,
                    CONTROL_SWITCH_TIMER_KIND,
                    now.timestamp_millis(),
                    now_text,
                    record.id
                ],
            )
            .map_err(|error| error.to_string())?;
        if let Some(session_id) = record.current_session_id {
            let _ = connection.execute(
                "UPDATE focus_sessions SET planned_seconds = 0, updated_at = ?1 WHERE id = ?2 AND status = 'running'",
                params![now_text, session_id],
            );
        }
    } else if kind == TIMER_KIND_POMODORO {
        let planned = planned_seconds.unwrap_or(0);
        let focus = focus_seconds.unwrap_or(0);
        let short_break = break_seconds.unwrap_or(0);
        let long_break = long_break_seconds.unwrap_or(0);
        let interval = long_break_interval.unwrap_or(0);
        if focus <= 0 {
            return Err("番茄钟时长必须大于 0 秒".to_string());
        }
        if short_break <= 0 {
            return Err("短休息时长必须大于 0 秒".to_string());
        }
        if long_break <= 0 {
            return Err("长休息时长必须大于 0 秒".to_string());
        }
        if interval <= 0 {
            return Err("长休息间隔必须大于 0".to_string());
        }
        let elapsed = study_elapsed_seconds(&record, now)?;
        if planned <= elapsed {
            return Err("总时长必须大于已累计学习时间".to_string());
        }
        connection
            .execute(
                "
                UPDATE study_modes
                SET timer_kind = ?1,
                    planned_seconds = ?2,
                    focus_seconds = ?3,
                    break_seconds = ?4,
                    long_break_seconds = ?5,
                    long_break_interval = ?6,
                    state_revision = state_revision + 1,
                    last_control_device_id = ?7,
                    last_control_action = ?8,
                    last_control_at = ?9,
                    updated_at = ?10
                WHERE id = ?11 AND status = 'active'
                ",
                params![
                    TIMER_KIND_POMODORO,
                    planned,
                    focus,
                    short_break,
                    long_break,
                    interval,
                    device_id,
                    CONTROL_SWITCH_TIMER_KIND,
                    now.timestamp_millis(),
                    now_text,
                    record.id
                ],
            )
            .map_err(|error| error.to_string())?;
        if let Some(session_id) = record.current_session_id {
            let _ = connection.execute(
                "UPDATE focus_sessions SET planned_seconds = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'running'",
                params![focus, now_text, session_id],
            );
        }
    } else {
        return Err("未知的计时模式".to_string());
    }

    set_runtime_state(state.inner(), true, record.current_session_id)?;
    let next_state = load_current_study_mode_state(&connection, now)?;
    sync_focus_widget_for_state(&app, &next_state);
    trigger_shared_sync(&app, "focus_state_change");
    Ok(next_state)
}

pub fn tick_background_study_mode(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let before_marker = current_study_runtime_marker(app).ok().flatten();
    let study_state = advance_study_mode(app, state.inner())?;
    sync_focus_widget_for_state(app, &study_state);
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
