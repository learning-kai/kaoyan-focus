#[tauri::command]
pub fn list_feishu_sync_runs(
    app: AppHandle,
    limit: Option<i64>,
) -> Result<Vec<FeishuSyncRunSummary>, String> {
    let connection = open_database(&database_path(&app)?)?;
    list_feishu_runs(&connection, limit.unwrap_or(5).clamp(1, 50))
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

