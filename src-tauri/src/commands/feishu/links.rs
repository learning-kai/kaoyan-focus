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
            DELETE FROM sync_meta
            WHERE sync_id = ?1
              AND NOT (entity_type = ?2 AND local_id = ?3)
            ",
            params![sync_id, entity_type, local_id],
        )
        .map_err(|error| error.to_string())?;

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

