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

