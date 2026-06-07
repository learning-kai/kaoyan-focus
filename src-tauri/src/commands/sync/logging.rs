fn validate_sync_payload(payload: &SharedSyncPayload, local_now: Option<i64>) -> String {
    let mut warnings = Vec::new();
    if payload.schema_version <= 0 {
        warnings.push("schemaVersion is invalid".to_string());
    }
    if payload.device_id.trim().is_empty() {
        warnings.push("deviceId is empty".to_string());
    }
    let active_count = payload
        .study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(|status| status == "running" || status == "active")
                .unwrap_or(false)
        })
        .count();
    if active_count > 1 {
        warnings.push(format!("{active_count} active study modes were found"));
    }
    if let Some(now) = local_now {
        let drift_ms = now.saturating_sub(payload.exported_at).abs();
        if drift_ms > 120_000 {
            warnings.push(format!(
                "remote export clock drift is about {} seconds",
                drift_ms / 1000
            ));
        }
    }
    let entity_count = count_payload_entities(payload);
    let deleted_count = count_payload_deleted_entities(payload);
    if warnings.is_empty() {
        format!("validation passed: {entity_count} entities, {deleted_count} tombstones")
    } else {
        format!(
            "validation completed: {entity_count} entities, {deleted_count} tombstones; warnings: {}",
            warnings.join("; ")
        )
    }
}
fn record_object_sync_result(
    app: &AppHandle,
    sync_id: &str,
    trigger: &str,
    started_at: DateTime<Utc>,
    result: &ObjectStorageAutoSyncResult,
    payload: Option<&SharedSyncPayload>,
    remote_payload: Option<&SharedSyncPayload>,
    previous_active_snapshot: Option<&SharedActiveStudySnapshot>,
    validation_report: Option<String>,
    remote_backup_key: Option<String>,
) {
    let finished_at = Utc::now();
    let Ok(path) = database_path(app) else {
        return;
    };
    let Ok(connection) = open_database(&path) else {
        return;
    };
    let active_snapshot = payload.and_then(shared_active_study_snapshot);
    let remote_active_snapshot = remote_payload.and_then(shared_active_study_snapshot);
    let remote_exported_drift_seconds = remote_payload.map(|remote| {
        (Utc::now()
            .timestamp_millis()
            .saturating_sub(remote.exported_at))
        .abs()
            / 1000
    });
    let detail = build_sync_run_detail(
        trigger,
        result,
        previous_active_snapshot,
        active_snapshot.as_ref(),
        remote_active_snapshot.as_ref(),
        remote_exported_drift_seconds,
    );
    let record = SyncRunRecord {
        sync_id: sync_id.to_string(),
        backend: "object_storage".to_string(),
        trigger: trigger.to_string(),
        direction: result.direction.clone(),
        status: result.status.clone(),
        started_at,
        finished_at,
        device_id: payload.map(|value| value.device_id.clone()),
        remote_device_id: remote_payload.map(|value| value.device_id.clone()),
        remote_exported_at: remote_payload.map(|value| value.exported_at),
        local_exported_at: payload.map(|value| value.exported_at),
        bytes: result.bytes as i64,
        imported_count: payload.map(count_payload_entities).unwrap_or(0),
        exported_count: payload.map(count_payload_entities).unwrap_or(0),
        deleted_count: payload.map(count_payload_deleted_entities).unwrap_or(0),
        conflict_count: 0,
        active_state_changed: result.active_state_changed,
        took_over_active_mode: result.took_over_active_mode,
        validation_report,
        backup_path: result.backup_path.clone(),
        remote_backup_key,
        active_snapshot_sync_id: active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync_id.clone()),
        remote_active_snapshot_sync_id: remote_active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync_id.clone()),
        active_snapshot_phase: active_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.phase.clone()),
        remote_active_snapshot_phase: remote_active_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.phase.clone()),
        active_snapshot_updated_at: active_snapshot.as_ref().map(|snapshot| snapshot.updated_at),
        remote_snapshot_updated_at: remote_active_snapshot
            .as_ref()
            .map(|snapshot| snapshot.updated_at),
        remote_exported_drift_seconds,
        detail: Some(detail),
        error_message: result.skipped_reason.clone(),
    };
    let _ = insert_sync_run(&connection, &record);
}

fn build_sync_run_detail(
    trigger: &str,
    result: &ObjectStorageAutoSyncResult,
    previous_active: Option<&SharedActiveStudySnapshot>,
    active: Option<&SharedActiveStudySnapshot>,
    remote_active: Option<&SharedActiveStudySnapshot>,
    remote_drift_seconds: Option<i64>,
) -> String {
    let decision = match (previous_active, active, remote_active) {
        (Some(previous), Some(current), Some(remote))
            if current.sync_id == remote.sync_id
                && previous.sync_id != current.sync_id
                && remote.updated_at > previous.updated_at =>
        {
            "takeover"
        }
        (Some(previous), Some(current), Some(remote))
            if previous.sync_id == current.sync_id && remote.updated_at < previous.updated_at =>
        {
            "rejected_stale_remote"
        }
        (Some(previous), Some(current), Some(remote))
            if previous.sync_id == current.sync_id
                && remote.sync_id != current.sync_id
                && remote.updated_at == previous.updated_at =>
        {
            "tie_kept_local"
        }
        (None, Some(current), Some(remote))
            if current.sync_id == remote.sync_id && result.took_over_active_mode =>
        {
            "accepted_remote"
        }
        _ if result.took_over_active_mode => "takeover",
        _ => "kept_local",
    };
    let pull_mode = if trigger == "periodic_pull" {
        "pull_only"
    } else {
        "full"
    };
    let drift = remote_drift_seconds
        .map(|seconds| seconds.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "{pull_mode} decision={decision}; localBefore={}; active={}; remote={}; remoteDriftSeconds={drift}",
        format_active_snapshot(previous_active),
        format_active_snapshot(active),
        format_active_snapshot(remote_active)
    )
}

fn format_active_snapshot(snapshot: Option<&SharedActiveStudySnapshot>) -> String {
    snapshot
        .map(|value| {
            format!(
                "{}:{}:{}",
                value.sync_id,
                value.phase.as_deref().unwrap_or("unknown"),
                value.updated_at
            )
        })
        .unwrap_or_else(|| "none".to_string())
}

fn insert_sync_run(connection: &Connection, record: &SyncRunRecord) -> Result<i64, String> {
    connection
        .execute(
            "
            INSERT INTO sync_runs (
              sync_id, backend, trigger, direction, status, started_at, finished_at, duration_ms,
              device_id, remote_device_id, remote_exported_at, local_exported_at, bytes,
              imported_count, exported_count, deleted_count, conflict_count,
              active_state_changed, took_over_active_mode, validation_report, backup_path,
              remote_backup_key, active_snapshot_sync_id, remote_active_snapshot_sync_id,
              active_snapshot_phase, remote_active_snapshot_phase, active_snapshot_updated_at,
              remote_snapshot_updated_at, remote_exported_drift_seconds, detail, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)
            ",
            rusqlite::params![
                record.sync_id,
                record.backend,
                record.trigger,
                record.direction,
                record.status,
                record.started_at.to_rfc3339(),
                record.finished_at.to_rfc3339(),
                (record.finished_at - record.started_at).num_milliseconds(),
                record.device_id,
                record.remote_device_id,
                record.remote_exported_at,
                record.local_exported_at,
                record.bytes,
                record.imported_count,
                record.exported_count,
                record.deleted_count,
                record.conflict_count,
                if record.active_state_changed { 1 } else { 0 },
                if record.took_over_active_mode { 1 } else { 0 },
                record.validation_report,
                record.backup_path,
                record.remote_backup_key,
                record.active_snapshot_sync_id,
                record.remote_active_snapshot_sync_id,
                record.active_snapshot_phase,
                record.remote_active_snapshot_phase,
                record.active_snapshot_updated_at,
                record.remote_snapshot_updated_at,
                record.remote_exported_drift_seconds,
                record.detail,
                record.error_message,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(connection.last_insert_rowid())
}

fn row_to_sync_run_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncRunSummary> {
    Ok(SyncRunSummary {
        id: row.get(0)?,
        sync_id: row.get(1)?,
        backend: row.get(2)?,
        trigger: row.get(3)?,
        direction: row.get(4)?,
        status: row.get(5)?,
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        duration_ms: row.get(8)?,
        device_id: row.get(9)?,
        remote_device_id: row.get(10)?,
        remote_exported_at: row.get(11)?,
        local_exported_at: row.get(12)?,
        bytes: row.get(13)?,
        imported_count: row.get(14)?,
        exported_count: row.get(15)?,
        deleted_count: row.get(16)?,
        conflict_count: row.get(17)?,
        active_state_changed: row.get::<_, i64>(18)? != 0,
        took_over_active_mode: row.get::<_, i64>(19)? != 0,
        validation_report: row.get(20)?,
        backup_path: row.get(21)?,
        remote_backup_key: row.get(22)?,
        active_snapshot_sync_id: row.get(23)?,
        remote_active_snapshot_sync_id: row.get(24)?,
        active_snapshot_phase: row.get(25)?,
        remote_active_snapshot_phase: row.get(26)?,
        active_snapshot_updated_at: row.get(27)?,
        remote_snapshot_updated_at: row.get(28)?,
        remote_exported_drift_seconds: row.get(29)?,
        detail: row.get(30)?,
        error_message: row.get(31)?,
    })
}

fn get_setting(connection: &Connection, key: &str, fallback: &str) -> Result<String, String> {
    Ok(connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn get_bool_setting(connection: &Connection, key: &str, fallback: bool) -> Result<bool, String> {
    let raw = get_setting(connection, key, if fallback { "true" } else { "false" })?;
    Ok(matches!(raw.as_str(), "true" | "1" | "yes" | "on"))
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            (key, value, updated_at),
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn persist_webdav_settings(
    connection: &Connection,
    settings: &WebDavSettings,
    password_changed: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(
        connection,
        WEBDAV_ENABLED_KEY,
        &settings.enabled.to_string(),
        &now,
    )?;
    set_setting(connection, WEBDAV_URL_KEY, &settings.url, &now)?;
    set_setting(connection, WEBDAV_USERNAME_KEY, &settings.username, &now)?;
    if password_changed {
        credential::set_secret(connection, WEBDAV_PASSWORD_KEY, &settings.password, &now)?;
    } else {
        credential::set_secret_if_changed(connection, WEBDAV_PASSWORD_KEY, "", &now)?;
    }
    set_setting(
        connection,
        WEBDAV_REMOTE_PATH_KEY,
        &settings.remote_path,
        &now,
    )?;
    Ok(())
}

fn persist_object_storage_settings(
    connection: &Connection,
    settings: &ObjectStorageSettings,
    secret_changed: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(
        connection,
        OBJECT_STORAGE_ENABLED_KEY,
        &settings.enabled.to_string(),
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_ENDPOINT_KEY,
        &settings.endpoint,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_BUCKET_KEY,
        &settings.bucket,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_ACCESS_KEY_ID_KEY,
        &settings.access_key_id,
        &now,
    )?;
    if secret_changed {
        credential::set_secret(
            connection,
            OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY,
            &settings.secret_access_key,
            &now,
        )?;
    } else {
        credential::set_secret_if_changed(
            connection,
            OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY,
            "",
            &now,
        )?;
    }
    set_setting(
        connection,
        OBJECT_STORAGE_REGION_KEY,
        &settings.region,
        &now,
    )?;
    set_setting(
        connection,
        OBJECT_STORAGE_OBJECT_KEY_KEY,
        &settings.object_key,
        &now,
    )?;
    Ok(())
}

