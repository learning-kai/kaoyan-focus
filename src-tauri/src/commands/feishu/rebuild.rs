fn rebuild_feishu_tasklists_from_local_blocking(
    database_path: PathBuf,
    app_data_dir: PathBuf,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuRebuildResult, String> {
    let _sync_guard = match FEISHU_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Err("已有飞书同步正在执行，请稍后再重建。".to_string());
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err("飞书同步锁状态异常，请重启应用后再试。".to_string());
        }
    };
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let backup_path = create_feishu_local_database_backup(&database_path, &app_data_dir)?;
    let mut connection = open_database(&database_path)?;
    let result = rebuild_feishu_tasklists_inner(&mut connection, &app_data_dir);

    let final_result = match result {
        Ok(value) => value,
        Err(error) => {
            let failed = FeishuSyncResult {
                status: "failed".to_string(),
                message: error.clone(),
                pushed_count: 0,
                pulled_count: 0,
                deleted_count: 0,
                conflict_count: 0,
                task_count: 0,
                calendar_count: 0,
                synced_at: Utc::now().to_rfc3339(),
            };
            let _ = record_feishu_run(&connection, &run_id, "rebuild_tasks", started_at, &failed);
            return Err(error);
        }
    };

    let sync_result = FeishuSyncResult {
        status: final_result.status.clone(),
        message: format!(
            "{} 本地备份：{}；飞书备份：{}",
            final_result.message,
            backup_path.to_string_lossy(),
            final_result.remote_backup_path
        ),
        pushed_count: final_result.uploaded_task_count,
        pulled_count: 0,
        deleted_count: final_result.deleted_tasklist_count,
        conflict_count: 0,
        task_count: final_result.uploaded_task_count,
        calendar_count: 0,
        synced_at: final_result.synced_at.clone(),
    };
    record_feishu_run(
        &connection,
        &run_id,
        "rebuild_tasks",
        started_at,
        &sync_result,
    )?;

    Ok(FeishuRebuildResult {
        backup_path: backup_path.to_string_lossy().to_string(),
        ..final_result
    })
}

fn rebuild_feishu_tasklists_inner(
    connection: &mut Connection,
    app_data_dir: &Path,
) -> Result<FeishuRebuildResult, String> {
    let settings = read_feishu_settings_for_api(connection)?;
    if settings.app_id.is_empty() || settings.app_secret.is_empty() {
        return Err("未配置飞书 App ID 或 App Secret。".to_string());
    }
    let token = ensure_access_token(connection)?;
    let feishu = FeishuClient::new(token.access_token)?;
    let remote_tasklists =
        feishu.get_paged("/open-apis/task/v2/tasklists?page_size=100&user_id_type=open_id")?;
    let backup_path = export_feishu_tasklists_backup(&feishu, app_data_dir, &remote_tasklists)?;
    let app_tasklists = remote_tasklists
        .iter()
        .filter(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .map(is_app_tasklist_name)
                .unwrap_or(false)
        })
        .filter_map(|item| {
            item.get("guid")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();

    let mut deleted_tasklist_count = 0;
    for tasklist_guid in app_tasklists {
        feishu.delete(&format!(
            "/open-apis/task/v2/tasklists/{}?user_id_type=open_id",
            encode_path_segment(&tasklist_guid)
        ))?;
        deleted_tasklist_count += 1;
    }

    clear_feishu_tasklist_settings(connection)?;
    connection
        .execute(
            "DELETE FROM feishu_sync_links WHERE remote_kind = ?1",
            params![REMOTE_FEISHU_TASK],
        )
        .map_err(|error| error.to_string())?;

    let tasklists = create_fresh_feishu_tasklists(connection, &feishu)?;
    let mut counters = SyncCounters {
        pushed_count: 0,
        pulled_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        task_count: 0,
        calendar_count: 0,
    };
    for task in load_local_tasks(connection)?
        .into_iter()
        .filter(|task| task.deleted_at.is_none())
    {
        if task.title.trim().is_empty() {
            continue;
        }
        let tasklist_guid = tasklists
            .get(task.tasklist_key)
            .or_else(|| tasklists.get(TASKLIST_KEY_GENERAL))
            .ok_or_else(|| "飞书任务清单未初始化。".to_string())?;
        replace_remote_task_in_tasklist(
            connection,
            &feishu,
            tasklist_guid,
            &task,
            None,
            &mut counters,
        )?;
    }

    Ok(FeishuRebuildResult {
        status: "rebuilt".to_string(),
        message: format!(
            "飞书任务清单已按本地数据重建：删除旧清单 {} 个，上传任务 {} 条。",
            deleted_tasklist_count, counters.pushed_count
        ),
        backup_path: String::new(),
        remote_backup_path: backup_path.to_string_lossy().to_string(),
        deleted_tasklist_count,
        uploaded_task_count: counters.pushed_count,
        tasklist_count: tasklists.len() as i64,
        synced_at: Utc::now().to_rfc3339(),
    })
}

fn create_feishu_local_database_backup(
    database_path: &Path,
    app_data_dir: &Path,
) -> Result<PathBuf, String> {
    let backup_path = app_data_dir.join(format!(
        "kaoyan-focus.before-feishu-rebuild-{}.sqlite3",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(database_path, &backup_path)
        .map_err(|error| format!("备份本地数据库失败：{error}"))?;
    Ok(backup_path)
}

fn export_feishu_tasklists_backup(
    feishu: &FeishuClient,
    app_data_dir: &Path,
    tasklists: &[Value],
) -> Result<PathBuf, String> {
    let backup_dir = app_data_dir.join("feishu-backups");
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_dir.join(format!(
        "feishu-tasklists-before-rebuild-{}.json",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let mut exported = Vec::new();
    for tasklist in tasklists.iter().filter(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .map(is_app_tasklist_name)
            .unwrap_or(false)
    }) {
        let guid = tasklist
            .get("guid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let tasks = if guid.is_empty() {
            Vec::new()
        } else {
            feishu.get_paged(&format!(
                "/open-apis/task/v2/tasklists/{}/tasks?page_size=100&user_id_type=open_id",
                encode_path_segment(guid)
            ))?
        };
        exported.push(json!({
            "tasklist": tasklist,
            "tasks": tasks,
        }));
    }
    let payload = json!({
        "exported_at": Utc::now().to_rfc3339(),
        "source": "kaoyan-focus-feishu-rebuild",
        "tasklists": exported,
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|error| format!("序列化飞书备份失败：{error}"))?;
    fs::write(&backup_path, bytes).map_err(|error| format!("写入飞书备份失败：{error}"))?;
    Ok(backup_path)
}

