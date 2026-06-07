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

