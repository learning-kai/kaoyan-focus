fn ensure_feishu_containers(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<FeishuContainers, String> {
    let tasklists = ensure_feishu_tasklists(connection, feishu)?;
    let calendar_id = match non_empty_setting(connection, FEISHU_CALENDAR_ID_KEY)? {
        Some(value) => value,
        None => {
            let id = find_or_create_calendar(feishu)?;
            set_setting(
                connection,
                FEISHU_CALENDAR_ID_KEY,
                &id,
                &Utc::now().to_rfc3339(),
            )?;
            id
        }
    };
    Ok(FeishuContainers {
        tasklists,
        calendar_id,
    })
}

fn ensure_feishu_tasklists(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<HashMap<String, String>, String> {
    let existing =
        feishu.get_paged("/open-apis/task/v2/tasklists?page_size=100&user_id_type=open_id")?;
    let legacy_tasklist_guid = non_empty_setting(connection, FEISHU_LEGACY_TASKLIST_GUID_KEY)?
        .or(non_empty_setting(connection, FEISHU_TASKLIST_GUID_KEY)?)
        .or_else(|| find_tasklist_guid(&existing, BRIDGE_CONTAINER_NAME));
    let mut tasklists = HashMap::new();
    for key in TASKLIST_KEYS {
        let setting_key = feishu_tasklist_setting_key(key);
        let guid = match non_empty_setting(connection, &setting_key)? {
            Some(value) => value,
            None => {
                let id = find_or_create_tasklist(feishu, &existing, tasklist_title_for_key(key))?;
                let now = Utc::now().to_rfc3339();
                set_setting(connection, &setting_key, &id, &now)?;
                if key == TASKLIST_KEY_GENERAL {
                    if let Some(legacy_id) = legacy_tasklist_guid
                        .as_deref()
                        .filter(|legacy_id| *legacy_id != id)
                    {
                        set_setting(connection, FEISHU_LEGACY_TASKLIST_GUID_KEY, legacy_id, &now)?;
                    }
                    set_setting(connection, FEISHU_TASKLIST_GUID_KEY, &id, &now)?;
                }
                id
            }
        };
        tasklists.insert(key.to_string(), guid);
    }
    Ok(tasklists)
}

fn create_fresh_feishu_tasklists(
    connection: &Connection,
    feishu: &FeishuClient,
) -> Result<HashMap<String, String>, String> {
    let mut tasklists = HashMap::new();
    let now = Utc::now().to_rfc3339();
    for key in TASKLIST_KEYS {
        let data = feishu.post(
            "/open-apis/task/v2/tasklists?user_id_type=open_id",
            json!({ "name": tasklist_title_for_key(key) }),
        )?;
        let id = data
            .get("tasklist")
            .and_then(|item| item.get("guid"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| "创建飞书任务清单后未返回 guid。".to_string())?;
        set_setting(connection, &feishu_tasklist_setting_key(key), &id, &now)?;
        if key == TASKLIST_KEY_GENERAL {
            set_setting(connection, FEISHU_TASKLIST_GUID_KEY, &id, &now)?;
        }
        tasklists.insert(key.to_string(), id);
    }
    Ok(tasklists)
}

fn find_or_create_tasklist(
    feishu: &FeishuClient,
    existing: &[Value],
    title: &str,
) -> Result<String, String> {
    if let Some(id) = find_tasklist_guid(existing, title) {
        return Ok(id);
    }
    let data = feishu.post(
        "/open-apis/task/v2/tasklists?user_id_type=open_id",
        json!({ "name": title }),
    )?;
    data.get("tasklist")
        .and_then(|item| item.get("guid"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "创建飞书任务清单后未返回 guid。".to_string())
}

fn find_tasklist_guid(existing: &[Value], title: &str) -> Option<String> {
    existing.iter().find_map(|item| {
        if item.get("name").and_then(Value::as_str) == Some(title) {
            item.get("guid")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        } else {
            None
        }
    })
}

fn find_or_create_calendar(feishu: &FeishuClient) -> Result<String, String> {
    for item in feishu.get_paged("/open-apis/calendar/v4/calendars?page_size=100")? {
        if item.get("summary").and_then(Value::as_str) == Some(BRIDGE_CONTAINER_NAME) {
            if let Some(id) = item.get("calendar_id").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
        }
    }
    let data = feishu.post(
        "/open-apis/calendar/v4/calendars",
        json!({
            "summary": BRIDGE_CONTAINER_NAME,
            "description": "kaoyan-focus bridge calendar"
        }),
    )?;
    data.get("calendar")
        .or_else(|| data.get("calendar_info"))
        .and_then(|item| item.get("calendar_id").or_else(|| item.get("id")))
        .and_then(Value::as_str)
        .or_else(|| data.get("calendar_id").and_then(Value::as_str))
        .map(ToString::to_string)
        .ok_or_else(|| "创建飞书日历后未返回 calendar_id。".to_string())
}

fn feishu_tasklist_setting_key(key: &str) -> String {
    format!("feishu_tasklist_guid_{key}")
}

fn tasklist_title_for_key(key: &str) -> &'static str {
    match key {
        TASKLIST_KEY_POLITICS => "考研专注 - 政治",
        TASKLIST_KEY_ENGLISH => "考研专注 - 英语",
        TASKLIST_KEY_MATH => "考研专注 - 数学",
        TASKLIST_KEY_MAJOR => "考研专注 - 专业课",
        TASKLIST_KEY_TODAY => "考研专注 - 今日任务",
        _ => "考研专注 - 通用",
    }
}

fn tasklist_key_for_board_scope(board_scope: &str) -> &'static str {
    match board_scope {
        "checklist:politics" => TASKLIST_KEY_POLITICS,
        "checklist:english" => TASKLIST_KEY_ENGLISH,
        "checklist:math" => TASKLIST_KEY_MATH,
        "checklist:major" => TASKLIST_KEY_MAJOR,
        _ => TASKLIST_KEY_GENERAL,
    }
}

fn sync_tasks(
    connection: &mut Connection,
    feishu: &FeishuClient,
    tasklists: &HashMap<String, String>,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let mut remote_tasks = Vec::new();
    for (key, tasklist_guid) in tasklists {
        fetch_remote_tasks_for_tasklist(feishu, key, tasklist_guid, &mut remote_tasks)?;
    }
    counters.task_count = remote_tasks.len() as i64;
    let remote_by_id = remote_tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect::<HashMap<_, _>>();
    let today_date = today_date_string();
    let stale_today_remote_ids =
        if let Some(today_tasklist_guid) = tasklists.get(TASKLIST_KEY_TODAY) {
            prune_stale_today_task_links(
                connection,
                feishu,
                today_tasklist_guid,
                &today_date,
                counters,
            )?
        } else {
            Vec::new()
        };

    for task in load_local_tasks(connection)? {
        if task.deleted_at.is_none() && task.title.trim().is_empty() {
            continue;
        }
        let tasklist_guid = tasklists
            .get(task.tasklist_key)
            .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
            .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
        if let Some(link) = get_link_by_sync_id(
            connection,
            task.entity_type,
            &task.sync_id,
            REMOTE_FEISHU_TASK,
        )? {
            if let Some(remote) = remote_by_id.get(&link.remote_id) {
                if task.deleted_at.is_some() {
                    feishu.delete(&format!(
                        "/open-apis/task/v2/tasks/{}",
                        encode_path_segment(&link.remote_id)
                    ))?;
                    mark_link_deleted(connection, link.id)?;
                    counters.deleted_count += 1;
                    continue;
                }
                let remote_in_current_tasklists =
                    tasklists.values().any(|guid| guid == &remote.tasklist_guid);
                sync_linked_task(
                    connection,
                    feishu,
                    &remote.tasklist_guid,
                    &task,
                    remote,
                    &link,
                    remote_in_current_tasklists,
                    counters,
                )?;
                let local_after_sync =
                    load_local_task_by_id(connection, task.entity_type, task.id).unwrap_or(task);
                let desired_tasklist_guid = tasklists
                    .get(local_after_sync.tasklist_key)
                    .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
                    .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
                if remote.tasklist_guid != *desired_tasklist_guid {
                    replace_remote_task_in_tasklist(
                        connection,
                        feishu,
                        desired_tasklist_guid,
                        &local_after_sync,
                        Some(remote.id.as_str()),
                        counters,
                    )?;
                }
            } else if task.deleted_at.is_none() {
                if link_points_to_current_tasklist(&link, tasklists) {
                    delete_local_task(connection, task.entity_type, task.id)?;
                    mark_link_deleted(connection, link.id)?;
                    counters.deleted_count += 1;
                } else {
                    replace_remote_task_in_tasklist(
                        connection,
                        feishu,
                        tasklist_guid,
                        &task,
                        Some(link.remote_id.as_str()),
                        counters,
                    )?;
                }
            } else {
                feishu.delete(&format!(
                    "/open-apis/task/v2/tasks/{}",
                    encode_path_segment(&link.remote_id)
                ))?;
                mark_link_deleted(connection, link.id)?;
                counters.deleted_count += 1;
            }
        } else if task.deleted_at.is_none() {
            replace_remote_task_in_tasklist(
                connection,
                feishu,
                tasklist_guid,
                &task,
                None,
                counters,
            )?;
        }
    }

    for remote in remote_tasks {
        if stale_today_remote_ids.iter().any(|id| id == &remote.id) {
            continue;
        }
        if get_link_by_remote_id(connection, REMOTE_FEISHU_TASK, &remote.id)?.is_some() {
            continue;
        }
        if let (Some(entity_type), Some(sync_id)) = (
            remote.marker_entity_type.as_deref(),
            remote.marker_sync_id.as_deref(),
        ) {
            let entity_type = match entity_type {
                ENTITY_CHECKLIST_TASK => Some(ENTITY_CHECKLIST_TASK),
                ENTITY_TODAY_PLAN_ITEM => Some(ENTITY_TODAY_PLAN_ITEM),
                _ => None,
            };
            if let Some(entity_type) = entity_type {
                if let Some((local_id, deleted_at)) =
                    get_sync_meta_local_by_sync_id(connection, entity_type, sync_id)?
                {
                    if entity_type == ENTITY_TODAY_PLAN_ITEM
                        && remote.tasklist_key == TASKLIST_KEY_TODAY
                        && is_stale_today_plan_item(connection, local_id, &today_date)?
                    {
                        delete_remote_task_if_present(feishu, &remote.id)?;
                        counters.deleted_count += 1;
                        continue;
                    }
                    upsert_link(
                        connection,
                        entity_type,
                        Some(local_id),
                        sync_id,
                        REMOTE_FEISHU_TASK,
                        &remote.id,
                        Some(&remote.tasklist_guid),
                        None,
                        None,
                        remote
                            .updated_millis
                            .map(|value| value.to_string())
                            .as_deref(),
                    )?;
                    if deleted_at.is_none() {
                        sync_linked_task(
                            connection,
                            feishu,
                            &remote.tasklist_guid,
                            &load_local_task_by_id(connection, entity_type, local_id)?,
                            &remote,
                            &get_link_by_sync_id(
                                connection,
                                entity_type,
                                sync_id,
                                REMOTE_FEISHU_TASK,
                            )?
                            .ok_or_else(|| "飞书任务链接写入后读取失败。".to_string())?,
                            tasklists.values().any(|guid| guid == &remote.tasklist_guid),
                            counters,
                        )?;
                    }
                    continue;
                }
            }
        }
        create_local_task_from_remote(connection, &remote)?;
        counters.pulled_count += 1;
    }
    Ok(())
}

fn fetch_remote_tasks_for_tasklist(
    feishu: &FeishuClient,
    tasklist_key: &str,
    tasklist_guid: &str,
    remote_tasks: &mut Vec<RemoteTask>,
) -> Result<(), String> {
    let remote_values = feishu.get_paged(&format!(
        "/open-apis/task/v2/tasklists/{}/tasks?page_size=100&user_id_type=open_id",
        encode_path_segment(tasklist_guid)
    ))?;
    remote_tasks.extend(
        remote_values
            .iter()
            .filter_map(|value| parse_remote_task(value, tasklist_key, tasklist_guid)),
    );
    Ok(())
}

fn prune_stale_today_task_links(
    connection: &Connection,
    feishu: &FeishuClient,
    today_tasklist_guid: &str,
    today_date: &str,
    counters: &mut SyncCounters,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT l.id, l.remote_id
            FROM feishu_sync_links l
            INNER JOIN today_plan_items t ON t.id = l.local_id
            WHERE l.entity_type = ?1
              AND l.remote_kind = ?2
              AND l.deleted_at IS NULL
              AND (l.remote_parent_id = ?3 OR l.remote_parent_id IS NULL)
              AND t.today_date <> ?4
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                ENTITY_TODAY_PLAN_ITEM,
                REMOTE_FEISHU_TASK,
                today_tasklist_guid,
                today_date
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let stale_links = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut remote_ids = Vec::new();
    for (link_id, remote_id) in stale_links {
        delete_remote_task_if_present(feishu, &remote_id)?;
        mark_link_deleted(connection, link_id)?;
        counters.deleted_count += 1;
        remote_ids.push(remote_id);
    }
    Ok(remote_ids)
}

fn is_stale_today_plan_item(
    connection: &Connection,
    local_id: i64,
    today_date: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT today_date <> ?1 FROM today_plan_items WHERE id = ?2",
            params![today_date, local_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| error.to_string())
}

fn delete_remote_task_if_present(feishu: &FeishuClient, remote_id: &str) -> Result<(), String> {
    feishu.delete(&format!(
        "/open-apis/task/v2/tasks/{}",
        encode_path_segment(remote_id)
    ))
}

fn link_points_to_current_tasklist(link: &FeishuLink, tasklists: &HashMap<String, String>) -> bool {
    link.remote_parent_id
        .as_deref()
        .map(|parent_id| tasklists.values().any(|guid| guid == parent_id))
        .unwrap_or(false)
}

fn replace_remote_task_in_tasklist(
    connection: &Connection,
    feishu: &FeishuClient,
    tasklist_guid: &str,
    task: &LocalTask,
    previous_remote_id: Option<&str>,
    counters: &mut SyncCounters,
) -> Result<RemoteTask, String> {
    let data = feishu.post(
        "/open-apis/task/v2/tasks?user_id_type=open_id",
        feishu_task_body(task, tasklist_guid),
    )?;
    let remote = data
        .get("task")
        .and_then(|value| parse_remote_task(value, task.tasklist_key, tasklist_guid))
        .or_else(|| parse_remote_task(&data, task.tasklist_key, tasklist_guid))
        .ok_or_else(|| "飞书创建任务后未返回任务信息。".to_string())?;
    upsert_link(
        connection,
        task.entity_type,
        Some(task.id),
        &task.sync_id,
        REMOTE_FEISHU_TASK,
        &remote.id,
        Some(tasklist_guid),
        None,
        None,
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )?;
    if let Some(previous_remote_id) = previous_remote_id.filter(|id| *id != remote.id) {
        feishu.delete(&format!(
            "/open-apis/task/v2/tasks/{}",
            encode_path_segment(previous_remote_id)
        ))?;
    }
    counters.pushed_count += 1;
    counters.task_count += 1;
    Ok(remote)
}

fn sync_linked_task(
    connection: &Connection,
    feishu: &FeishuClient,
    tasklist_guid: &str,
    local: &LocalTask,
    remote: &RemoteTask,
    link: &FeishuLink,
    apply_remote_tasklist: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let local_updated = parse_rfc3339_millis(&local.updated_at)?;
    let remote_updated = remote.updated_millis.unwrap_or(local_updated);
    if local_updated > remote_updated + 1_000 {
        let data = feishu.patch(
            &format!(
                "/open-apis/task/v2/tasks/{}?user_id_type=open_id",
                encode_path_segment(&remote.id)
            ),
            feishu_task_patch_body(local),
        )?;
        let next_remote = data
            .get("task")
            .and_then(|value| parse_remote_task(value, local.tasklist_key, tasklist_guid))
            .or_else(|| parse_remote_task(&data, local.tasklist_key, tasklist_guid))
            .unwrap_or_else(|| remote.clone());
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            REMOTE_FEISHU_TASK,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            next_remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        counters.pushed_count += 1;
    } else if remote_updated > local_updated + 1_000 {
        update_local_task_from_remote(
            connection,
            local.entity_type,
            local.id,
            remote,
            apply_remote_tasklist,
        )?;
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            REMOTE_FEISHU_TASK,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        counters.pulled_count += 1;
    } else {
        upsert_link(
            connection,
            local.entity_type,
            Some(local.id),
            &local.sync_id,
            &link.remote_kind,
            &remote.id,
            Some(tasklist_guid),
            None,
            None,
            remote
                .updated_millis
                .map(|value| value.to_string())
                .as_deref(),
        )?;
    }
    Ok(())
}

fn feishu_tasklist_statuses(connection: &Connection) -> Result<Vec<FeishuTasklistStatus>, String> {
    let mut statuses = Vec::new();
    for key in TASKLIST_KEYS {
        let guid = non_empty_setting(connection, &feishu_tasklist_setting_key(key))?;
        statuses.push(FeishuTasklistStatus {
            key: key.to_string(),
            label: tasklist_label_for_key(key).to_string(),
            ready: guid.is_some(),
            guid,
        });
    }
    Ok(statuses)
}

fn tasklist_label_for_key(key: &str) -> &'static str {
    match key {
        TASKLIST_KEY_POLITICS => "政治",
        TASKLIST_KEY_ENGLISH => "英语",
        TASKLIST_KEY_MATH => "数学",
        TASKLIST_KEY_MAJOR => "专业课",
        TASKLIST_KEY_TODAY => "今日任务",
        _ => "通用",
    }
}

