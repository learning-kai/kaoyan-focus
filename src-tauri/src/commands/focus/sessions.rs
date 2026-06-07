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

