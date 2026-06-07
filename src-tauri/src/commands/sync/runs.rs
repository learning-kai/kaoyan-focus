#[tauri::command]
pub fn get_object_storage_settings(app: AppHandle) -> Result<ObjectStorageSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    read_object_storage_settings(&connection, false)
}

#[tauri::command]
pub fn save_object_storage_settings(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let secret_changed = !settings.secret_access_key.is_empty();
    let normalized =
        normalize_object_storage_settings(resolve_object_storage_secret(&connection, settings)?)?;
    persist_object_storage_settings(&connection, &normalized, secret_changed)?;

    Ok(redact_object_storage_settings(normalized))
}

#[tauri::command]
pub fn test_object_storage_connection(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageStatus, String> {
    let connection = open_database(&database_path(&app)?)?;
    let secret_changed = !settings.secret_access_key.is_empty();
    let normalized =
        normalize_object_storage_settings(resolve_object_storage_secret(&connection, settings)?)?;
    persist_object_storage_settings(&connection, &normalized, secret_changed)?;
    let metadata = with_s3_runtime(async {
        let client = object_storage_client(&normalized).await?;
        fetch_object_storage_metadata(&client, &normalized).await
    })?;

    Ok(ObjectStorageStatus {
        configured: true,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        region: normalized.region,
        object_key: normalized.object_key,
        object_exists: metadata.exists,
        object_size: metadata.size,
        last_modified: metadata.last_modified.map(|value| value.to_rfc3339()),
        message: if metadata.exists {
            "R2 connection succeeded; remote sync data is accessible.".to_string()
        } else {
            "R2 connection succeeded; remote sync data does not exist yet.".to_string()
        },
    })
}

#[tauri::command]
pub fn list_sync_runs(app: AppHandle, limit: Option<i64>) -> Result<Vec<SyncRunSummary>, String> {
    let connection = open_database(&database_path(&app)?)?;
    let limit = limit.unwrap_or(10).clamp(1, 100);
    let mut statement = connection
        .prepare(
            "
            SELECT id, sync_id, backend, trigger, direction, status, started_at, finished_at,
                   duration_ms, device_id, remote_device_id, remote_exported_at, local_exported_at,
                   bytes, imported_count, exported_count, deleted_count, conflict_count,
                   active_state_changed, took_over_active_mode, validation_report, backup_path,
                   remote_backup_key, active_snapshot_sync_id, remote_active_snapshot_sync_id,
                   active_snapshot_phase, remote_active_snapshot_phase, active_snapshot_updated_at,
                   remote_snapshot_updated_at, remote_exported_drift_seconds, detail, error_message
            FROM sync_runs
            ORDER BY finished_at DESC, id DESC
            LIMIT ?1
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([limit], row_to_sync_run_summary)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_sync_backups(app: AppHandle) -> Result<Vec<SyncBackupEntry>, String> {
    let mut entries = Vec::new();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    if app_data_dir.exists() {
        for entry in fs::read_dir(&app_data_dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("kaoyan-focus.before-") || !name.ends_with(".sqlite3") {
                continue;
            }
            let metadata = entry.metadata().ok();
            entries.push(SyncBackupEntry {
                source: "local".to_string(),
                key: path.to_string_lossy().to_string(),
                label: name.to_string(),
                created_at: metadata
                    .as_ref()
                    .and_then(|value| value.modified().ok())
                    .map(DateTime::<Utc>::from)
                    .map(|value| value.to_rfc3339()),
                bytes: metadata.map(|value| value.len()),
            });
        }
    }

    if let Ok(connection) = open_database(&database_path(&app)?) {
        let settings = read_object_storage_settings(&connection, true)?;
        if settings.enabled && object_storage_configured(&settings) {
            let normalized = normalize_object_storage_settings(settings)?;
            if let Ok(remote) = list_object_storage_backup_entries(&normalized) {
                entries.extend(remote);
            }
        }
    }

    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(entries)
}

#[tauri::command]
pub fn preview_sync_backup(
    app: AppHandle,
    source: String,
    key: String,
) -> Result<SyncBackupPreview, String> {
    let bytes = load_backup_bytes(&app, &source, &key)?;
    let validation_report;
    let entity_count;
    let deleted_count;
    let exported_at;
    let device_id;
    if source == "local" {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        let temp_path = app_data_dir.join("kaoyan-focus.backup-preview.tmp");
        fs::write(&temp_path, &bytes).map_err(|error| error.to_string())?;
        validation_report = validate_sqlite_database(&temp_path)
            .map(|_| "SQLite integrity_check ok".to_string())
            .unwrap_or_else(|error| error);
        let _ = fs::remove_file(&temp_path);
        entity_count = 0;
        deleted_count = 0;
        exported_at = None;
        device_id = None;
    } else {
        let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Parse sync backup failed: {error}"))?;
        validation_report = validate_sync_payload(&payload, Some(Utc::now().timestamp_millis()));
        entity_count = count_payload_entities(&payload);
        deleted_count = count_payload_deleted_entities(&payload);
        exported_at = Some(payload.exported_at);
        device_id = Some(payload.device_id);
    }

    Ok(SyncBackupPreview {
        source,
        key,
        bytes: bytes.len() as u64,
        validation_report,
        entity_count,
        deleted_count,
        exported_at,
        device_id,
    })
}

#[tauri::command]
pub fn restore_sync_backup(app: AppHandle, source: String, key: String) -> Result<String, String> {
    let bytes = load_backup_bytes(&app, &source, &key)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;

    if source == "local" {
        let temp_path = app_data_dir.join("kaoyan-focus.restore.tmp");
        fs::write(&temp_path, &bytes).map_err(|error| error.to_string())?;
        validate_sqlite_database(&temp_path)?;
        let _ = create_local_sync_backup(&app, &local_database_path, "restore-current")?;
        fs::rename(&temp_path, &local_database_path)
            .or_else(|_| {
                fs::copy(&temp_path, &local_database_path)?;
                fs::remove_file(&temp_path)
            })
            .map_err(|error| format!("Replace local database failed: {error}"))?;
        return Ok("Restored the database from local backup.".to_string());
    }

    let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parse sync backup failed: {error}"))?;
    let _ = create_local_sync_backup(&app, &local_database_path, "restore-current")?;
    let mut connection = open_database(&local_database_path)?;
    import_shared_sync_payload(&mut connection, &payload)?;
    let _ = crate::commands::focus::sync_study_runtime_state(&app);
    Ok("Imported shared sync data from the R2/S3 backup.".to_string())
}

