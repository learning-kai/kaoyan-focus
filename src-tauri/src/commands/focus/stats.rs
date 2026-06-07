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

