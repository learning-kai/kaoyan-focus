pub fn sync_feishu_bridge_after_local_change(app: AppHandle, trigger: &'static str) {
    if FEISHU_LOCAL_CHANGE_SYNC_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result = (|| -> Result<(), String> {
            let database_path = database_path(&app)?;
            let _sync_guard = FEISHU_SYNC_LOCK
                .lock()
                .map_err(|_| "飞书同步锁状态异常，请重启应用后再试。".to_string())?;
            let result = sync_feishu_bridge_blocking_locked(
                database_path,
                trigger.to_string(),
                Uuid::new_v4().to_string(),
                Utc::now(),
            )?;
            if result.status == "failed" {
                return Err(result.message);
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Feishu local-change sync failed: {error}");
            runtime_health::mark_task_error("feishu_background_sync", &error, Some(60));
        } else {
            runtime_health::mark_task_success("feishu_background_sync", Some(60));
        }
        FEISHU_LOCAL_CHANGE_SYNC_PENDING.store(false, Ordering::SeqCst);
    });
}

fn is_local_change_trigger(trigger: &str) -> bool {
    trigger.ends_with("_change")
}

#[tauri::command]
pub async fn rebuild_feishu_tasklists_from_local(
    app: AppHandle,
) -> Result<FeishuRebuildResult, String> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();
    let database_path = database_path(&app)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        rebuild_feishu_tasklists_from_local_blocking(
            database_path,
            app_data_dir,
            run_id,
            started_at,
        )
    })
    .await
    .map_err(|error| format!("飞书任务清单重建后台任务失败：{error}"))?
}

fn sync_feishu_bridge_blocking(
    database_path: std::path::PathBuf,
    trigger: String,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuSyncResult, String> {
    let _sync_guard = match FEISHU_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Ok(skipped_result("已有飞书同步正在执行，本次已跳过。"));
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err("飞书同步锁状态异常，请重启应用后再试。".to_string());
        }
    };
    sync_feishu_bridge_blocking_locked(database_path, trigger, run_id, started_at)
}

fn sync_feishu_bridge_blocking_locked(
    database_path: std::path::PathBuf,
    trigger: String,
    run_id: String,
    started_at: DateTime<Utc>,
) -> Result<FeishuSyncResult, String> {
    let mut connection = open_database(&database_path)?;

    let result: Result<FeishuSyncResult, String> = (|| {
        let settings = read_feishu_settings_for_api(&connection)?;
        if !settings.enabled {
            return Ok(skipped_result("飞书同步已关闭。"));
        }
        if settings.app_id.is_empty() || settings.app_secret.is_empty() {
            return Ok(skipped_result("未配置飞书 App ID 或 App Secret。"));
        }
        let token = ensure_access_token(&connection)?;
        let feishu = FeishuClient::new(token.access_token)?;
        let containers = ensure_feishu_containers(&connection, &feishu)?;
        let mut counters = SyncCounters {
            pushed_count: 0,
            pulled_count: 0,
            deleted_count: 0,
            conflict_count: 0,
            task_count: 0,
            calendar_count: 0,
        };
        sync_tasks(
            &mut connection,
            &feishu,
            &containers.tasklists,
            &mut counters,
        )?;
        sync_calendar_events(
            &mut connection,
            &feishu,
            &containers.calendar_id,
            is_local_change_trigger(&trigger),
            &mut counters,
        )?;
        Ok(FeishuSyncResult {
            status: "synced".to_string(),
            message: format!(
                "飞书同步完成：任务 {} 项，日历 {} 项。",
                counters.task_count, counters.calendar_count
            ),
            pushed_count: counters.pushed_count,
            pulled_count: counters.pulled_count,
            deleted_count: counters.deleted_count,
            conflict_count: counters.conflict_count,
            task_count: counters.task_count,
            calendar_count: counters.calendar_count,
            synced_at: Utc::now().to_rfc3339(),
        })
    })();

    let final_result = match result {
        Ok(value) => value,
        Err(error) => FeishuSyncResult {
            status: "failed".to_string(),
            message: error.clone(),
            pushed_count: 0,
            pulled_count: 0,
            deleted_count: 0,
            conflict_count: 0,
            task_count: 0,
            calendar_count: 0,
            synced_at: Utc::now().to_rfc3339(),
        },
    };
    record_feishu_run(&connection, &run_id, &trigger, started_at, &final_result)?;
    if final_result.status == "failed" {
        return Err(final_result.message);
    }
    Ok(final_result)
}

