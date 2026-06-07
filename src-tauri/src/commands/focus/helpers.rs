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
