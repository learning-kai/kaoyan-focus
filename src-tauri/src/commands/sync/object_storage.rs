#[tauri::command]
pub fn upload_database_to_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let secret_changed = !settings.secret_access_key.is_empty();
    let normalized =
        normalize_object_storage_settings(resolve_object_storage_secret(&connection, settings)?)?;
    persist_object_storage_settings(&connection, &normalized, secret_changed)?;
    let auto = with_object_storage_sync_lock(|| sync_r2_v3_object_storage(app, "manual_upload", false))?;
    Ok(ObjectStorageSyncResult {
        success: auto.status == "synced",
        message: auto.message,
        object_url: auto.object_url.unwrap_or_else(|| {
            format!(
                "{}/{}",
                normalized.endpoint.trim_end_matches('/'),
                r2_v3_prefix(&normalized)
            )
        }),
        bytes: auto.bytes,
        backup_path: auto.backup_path,
    })
}
#[tauri::command]
pub fn download_database_from_object_storage(
    app: AppHandle,
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSyncResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let secret_changed = !settings.secret_access_key.is_empty();
    let normalized =
        normalize_object_storage_settings(resolve_object_storage_secret(&connection, settings)?)?;
    persist_object_storage_settings(&connection, &normalized, secret_changed)?;
    let auto = with_object_storage_sync_lock(|| sync_r2_v3_object_storage(app, "manual_download", true))?;
    Ok(ObjectStorageSyncResult {
        success: auto.status == "synced",
        message: auto.message,
        object_url: auto.object_url.unwrap_or_else(|| {
            format!(
                "{}/{}",
                normalized.endpoint.trim_end_matches('/'),
                r2_v3_prefix(&normalized)
            )
        }),
        bytes: auto.bytes,
        backup_path: auto.backup_path,
    })
}
#[tauri::command]
pub async fn auto_sync_object_storage_database(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_object_storage_sync_lock(|| auto_sync_object_storage_database_pull_only(app))
    })
    .await
    .map_err(|error| format!("object storage auto sync background task failed: {error}"))?
}

#[tauri::command]
pub async fn sync_object_storage_state_change(
    app: AppHandle,
    trigger: Option<String>,
) -> Result<ObjectStorageAutoSyncResult, String> {
    let trigger = trigger.unwrap_or_else(|| "state_change".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        with_object_storage_sync_lock(|| {
            auto_sync_object_storage_database_blocking_with_trigger(app, &trigger)
        })
    })
    .await
    .map_err(|error| format!("object storage state sync background task failed: {error}"))?
}

fn sync_r2_v3_object_storage(
    app: AppHandle,
    trigger: &str,
    pull_only: bool,
) -> Result<ObjectStorageAutoSyncResult, String> {
    let started_at = Utc::now();
    let sync_id = Uuid::new_v4().to_string();
    eprintln!(
        "R2 v3 sync start id={} trigger={} pull_only={}",
        sync_id, trigger, pull_only
    );
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_object_storage_settings(&connection, true)?;
    if !settings.enabled {
        let result = skipped_object_storage_auto_sync(
            "object_storage_disabled",
            "object storage sync disabled",
            None,
        );
        record_object_sync_result(
            &app, &sync_id, trigger, started_at, &result, None, None, None, None, None,
        );
        eprintln!(
            "R2 v3 sync finish id={} status={} reason={:?}",
            sync_id, result.status, result.skipped_reason
        );
        return Ok(result);
    }
    if !object_storage_configured(&settings) {
        let result = skipped_object_storage_auto_sync(
            "object_storage_not_configured",
            "object storage not configured",
            None,
        );
        record_object_sync_result(
            &app, &sync_id, trigger, started_at, &result, None, None, None, None, None,
        );
        eprintln!(
            "R2 v3 sync finish id={} status={} reason={:?}",
            sync_id, result.status, result.skipped_reason
        );
        return Ok(result);
    }

    let normalized = normalize_object_storage_settings(settings)?;
    let object_url = format!(
        "{}/{}",
        normalized.endpoint.trim_end_matches('/'),
        r2_v3_prefix(&normalized)
    );
    let local_database_path = database_path(&app)?;

    for attempt in 0..3 {
        eprintln!(
            "R2 v3 sync stage id={} attempt={} open_db",
            sync_id,
            attempt + 1
        );
        let mut connection = open_database(&local_database_path)?;
        let device_id = load_or_create_device_id(&connection)?;
        eprintln!(
            "R2 v3 sync stage id={} attempt={} load_versions",
            sync_id,
            attempt + 1
        );
        let local_entity_versions = load_local_entity_versions(&connection)?;
        let applied_operation_ids = load_applied_operation_ids(&connection)?;
        let exported_at = Utc::now().timestamp_millis();
        eprintln!(
            "R2 v3 sync stage id={} attempt={} export_payload",
            sync_id,
            attempt + 1
        );
        let local_payload =
            export_shared_sync_payload(&connection, device_id.clone(), exported_at)?;
        let local_active_snapshot = shared_active_study_snapshot(&local_payload);
        let upload_context = active_upload_filter_context(&local_payload, &device_id);
        let local_pending_operations = if pull_only {
            Vec::new()
        } else {
            filter_passive_active_upload_operations(
                payload_entity_operations(&local_payload, &device_id, &local_entity_versions)?,
                &upload_context,
            )
        };
        eprintln!(
            "R2 v3 sync stage id={} attempt={} exported pending_ops={}",
            sync_id,
            attempt + 1,
            local_pending_operations.len()
        );

        let remote_state = with_s3_runtime(async {
            eprintln!(
                "R2 v3 sync stage id={} attempt={} create_client",
                sync_id,
                attempt + 1
            );
            let client = object_storage_client(&normalized).await?;
            if let Some(active_snapshot) = local_active_snapshot.as_ref() {
                if !pull_only {
                    eprintln!(
                        "R2 v3 sync stage id={} attempt={} acquire_active_lock",
                        sync_id,
                        attempt + 1
                    );
                    if !try_acquire_r2_v3_active_lock(
                        &client,
                        &normalized,
                        &device_id,
                        active_snapshot,
                    )
                    .await?
                    {
                        eprintln!(
                            "R2 v3 sync active lock conflict ignored before merge sync_id={}",
                            active_snapshot.sync_id
                        );
                    }
                }
            } else {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} release_active_lock",
                    sync_id,
                    attempt + 1
                );
                release_r2_v3_active_lock_if_owned(&client, &normalized, &device_id).await?;
            }
            if !pull_only {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} upload_pending_ops",
                    sync_id,
                    attempt + 1
                );
                upload_payload_operations(&client, &normalized, &local_pending_operations).await?;
            }
            eprintln!(
                "R2 v3 sync stage id={} attempt={} load_remote_state",
                sync_id,
                attempt + 1
            );
            load_r2_v3_remote_state(
                &client,
                &normalized,
                &device_id,
                &applied_operation_ids,
                &local_entity_versions,
            )
            .await
        });
        let remote_state = match remote_state {
            Ok(remote_state) => remote_state,
            Err(message) if message == "r2_active_lock_conflict" => {
                let result = skipped_object_storage_auto_sync(
                    "r2_active_lock_conflict",
                    "Another device already holds the active focus lock.",
                    Some(object_url),
                );
                record_object_sync_result(
                    &app,
                    &sync_id,
                    trigger,
                    started_at,
                    &result,
                    Some(&local_payload),
                    None,
                    local_active_snapshot.as_ref(),
                    None,
                    None,
                );
                return Ok(result);
            }
            Err(message) => return Err(message),
        };
        eprintln!(
            "R2 v3 sync stage id={} attempt={} remote_loaded ops={} bytes={}",
            sync_id,
            attempt + 1,
            remote_state.operation_count,
            remote_state.bytes
        );

        let remote_report = format!(
            "{}; r2_v3 manifest={} ops={} migratedLegacy={}",
            validate_sync_payload(&remote_state.payload, Some(Utc::now().timestamp_millis())),
            remote_state.manifest.is_some(),
            remote_state.operation_count,
            remote_state.migrated_legacy
        );
        let local_primary_owner = (
            local_payload.primary_owner_device_id.clone(),
            local_payload.primary_owner_updated_at,
        );
        let merged_payload = if pull_only {
            merge_remote_payload_into_local(
                local_payload,
                remote_state.payload.clone(),
                device_id.clone(),
                exported_at,
            )
        } else {
            merge_shared_sync_payloads(
                local_payload,
                remote_state.payload.clone(),
                device_id.clone(),
                exported_at,
            )
        };

        let backup_path = create_local_sync_backup(
            &app,
            &local_database_path,
            if pull_only {
                "r2-v3-pull"
            } else {
                "r2-v3-auto"
            },
        )?;
        eprintln!(
            "R2 v3 sync stage id={} attempt={} import_payload",
            sync_id,
            attempt + 1
        );
        import_shared_sync_payload(&mut connection, &merged_payload)?;
        let _ = crate::commands::focus::sync_study_runtime_state(&app);

        eprintln!(
            "R2 v3 sync stage id={} attempt={} refresh_payload",
            sync_id,
            attempt + 1
        );
        let refreshed_payload = export_shared_sync_payload(
            &connection,
            device_id.clone(),
            Utc::now().timestamp_millis(),
        )?;
        let primary_owner_changed = local_primary_owner
            != (
                refreshed_payload.primary_owner_device_id.clone(),
                refreshed_payload.primary_owner_updated_at,
            );
        let refreshed_active_snapshot = shared_active_study_snapshot(&refreshed_payload);
        let active_state_changed = local_active_snapshot != refreshed_active_snapshot;
        let took_over_active_mode = refreshed_active_snapshot.is_some()
            && local_active_snapshot
                .as_ref()
                .map(|snapshot| snapshot.sync_id.as_str())
                != refreshed_active_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.sync_id.as_str());

        if pull_only {
            persist_applied_operations(&connection, &remote_state.applied_operations)?;
            persist_payload_entity_versions(&connection, &remote_state.payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 pull-only sync completed".to_string(),
                direction: Some("download".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
                primary_owner_changed,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} pull_only=true",
                sync_id, result.status
            );
            return Ok(result);
        }

        let write_success = with_s3_runtime(async {
            eprintln!(
                "R2 v3 sync stage id={} attempt={} write_create_client",
                sync_id,
                attempt + 1
            );
            let client = object_storage_client(&normalized).await?;
            let version_seed = apply_operations_to_version_map(
                apply_operations_to_version_map(
                    payload_entity_version_map(&remote_state.payload, &device_id)?,
                    &remote_state.applied_operations,
                ),
                &local_pending_operations,
            );
            let refreshed_ops = filter_passive_active_upload_operations(
                payload_entity_operations(&refreshed_payload, &device_id, &version_seed)?,
                &active_upload_filter_context(&refreshed_payload, &device_id),
            );
            eprintln!(
                "R2 v3 sync stage id={} attempt={} upload_refreshed_ops={}",
                sync_id,
                attempt + 1,
                refreshed_ops.len()
            );
            upload_payload_operations(&client, &normalized, &refreshed_ops).await?;
            let primary_owner_manifest_changed = remote_state
                .manifest
                .as_ref()
                .map(|manifest| {
                    manifest.primary_owner_device_id != refreshed_payload.primary_owner_device_id
                        || manifest.primary_owner_updated_at
                            != refreshed_payload.primary_owner_updated_at
                })
                .unwrap_or(true);
            if local_pending_operations.is_empty()
                && refreshed_ops.is_empty()
                && !primary_owner_manifest_changed
            {
                return Ok::<(bool, Vec<R2V3Operation>), String>((true, refreshed_ops));
            }
            let mut watermarks = remote_state.watermarks.clone();
            for operation in local_pending_operations.iter().chain(refreshed_ops.iter()) {
                watermarks
                    .entry(operation.device_id.clone())
                    .and_modify(|value| *value = (*value).max(operation.seq))
                    .or_insert(operation.seq);
            }
            let should_compact = remote_state.manifest.is_none()
                || remote_state.migrated_legacy
                || remote_state.operation_count >= R2_V3_SNAPSHOT_OP_THRESHOLD
                || remote_state.bytes >= R2_V3_SNAPSHOT_BYTES_THRESHOLD
                || remote_state
                    .manifest
                    .as_ref()
                    .and_then(|manifest| manifest.current_snapshot_key.as_ref())
                    .is_none();
            let success = if should_compact {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} write_snapshot_manifest",
                    sync_id,
                    attempt + 1
                );
                write_r2_v3_snapshot_and_manifest(
                    &client,
                    &normalized,
                    refreshed_payload.clone(),
                    watermarks,
                    remote_state.manifest_etag.as_deref(),
                    &device_id,
                )
                .await?
            } else {
                eprintln!(
                    "R2 v3 sync stage id={} attempt={} write_manifest",
                    sync_id,
                    attempt + 1
                );
                write_r2_v3_manifest(
                    &client,
                    &normalized,
                    remote_state
                        .manifest
                        .as_ref()
                        .and_then(|manifest| manifest.current_snapshot_key.clone()),
                    watermarks,
                    refreshed_payload.primary_owner_device_id.clone(),
                    refreshed_payload.primary_owner_updated_at,
                    remote_state.manifest_etag.as_deref(),
                )
                .await?
            };
            Ok::<(bool, Vec<R2V3Operation>), String>((success, refreshed_ops))
        })?;

        if write_success.0 {
            let mut applied_operations = remote_state.applied_operations.clone();
            applied_operations.extend(local_pending_operations.clone());
            applied_operations.extend(write_success.1.clone());
            persist_applied_operations(&connection, &applied_operations)?;
            persist_payload_entity_versions(&connection, &refreshed_payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 sync completed with manifest CAS protection".to_string(),
                direction: Some("download_upload".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
                primary_owner_changed,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} pull_only=false",
                sync_id, result.status
            );
            return Ok(result);
        }

        if !local_pending_operations.is_empty() || !write_success.1.is_empty() {
            let mut applied_operations = remote_state.applied_operations.clone();
            applied_operations.extend(local_pending_operations.clone());
            applied_operations.extend(write_success.1.clone());
            persist_applied_operations(&connection, &applied_operations)?;
            persist_payload_entity_versions(&connection, &refreshed_payload, &device_id)?;
            let result = ObjectStorageAutoSyncResult {
                status: "synced".to_string(),
                message: "R2 v3 ops uploaded; manifest CAS will be reconciled by the next sync"
                    .to_string(),
                direction: Some("download_upload".to_string()),
                skipped_reason: None,
                synced_at: Utc::now().to_rfc3339(),
                object_url: Some(object_url),
                bytes: remote_state.bytes as u64,
                backup_path: backup_path.clone(),
                active_state_changed,
                took_over_active_mode,
                primary_owner_changed,
            };
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                Some("manifestConflict=true uploadedOps=true".to_string()),
            );
            emit_study_sync_state_changed(&app, &result);
            eprintln!(
                "R2 v3 sync finish id={} status={} manifest_conflict_uploaded_ops=true",
                sync_id, result.status
            );
            return Ok(result);
        }

        if attempt == 2 {
            let result = skipped_object_storage_auto_sync(
                "r2_manifest_conflict",
                "R2 manifest conflict; local data kept and retry will continue next time",
                Some(object_url),
            );
            record_object_sync_result(
                &app,
                &sync_id,
                trigger,
                started_at,
                &result,
                Some(&refreshed_payload),
                Some(&remote_state.payload),
                local_active_snapshot.as_ref(),
                Some(remote_report),
                None,
            );
            return Ok(result);
        }
    }

    Ok(skipped_object_storage_auto_sync(
        "r2_retry_exhausted",
        "R2 v3 sync retry exhausted",
        None,
    ))
}

fn with_object_storage_sync_lock<F>(work: F) -> Result<ObjectStorageAutoSyncResult, String>
where
    F: FnOnce() -> Result<ObjectStorageAutoSyncResult, String>,
{
    let _guard = match OBJECT_STORAGE_SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Ok(skipped_object_storage_auto_sync(
                "object_storage_sync_in_flight",
                "object storage sync already in flight, skipped",
                None,
            ));
        }
        Err(TryLockError::Poisoned(_)) => {
            return Err(
                "object storage sync lock is poisoned, please restart the app and retry"
                    .to_string(),
            );
        }
    };
    work()
}
fn auto_sync_object_storage_database_pull_only(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    sync_r2_v3_object_storage(app, "periodic_pull", true)
}
pub(crate) fn sync_object_storage_after_external_change(
    app: AppHandle,
    trigger: &str,
) -> Result<ObjectStorageAutoSyncResult, String> {
    with_object_storage_sync_lock(|| {
        auto_sync_object_storage_database_blocking_with_trigger(app, trigger)
    })
}

pub(crate) fn poll_object_storage_for_remote_changes(
    app: AppHandle,
) -> Result<ObjectStorageAutoSyncResult, String> {
    with_object_storage_sync_lock(|| {
        if has_pending_object_storage_local_changes(app.clone()).unwrap_or(false) {
            auto_sync_object_storage_database_blocking_with_trigger(app, "periodic_local_change")
        } else {
            auto_sync_object_storage_database_pull_only(app)
        }
    })
}

fn auto_sync_object_storage_database_blocking_with_trigger(
    app: AppHandle,
    trigger: &str,
) -> Result<ObjectStorageAutoSyncResult, String> {
    sync_r2_v3_object_storage(app, trigger, false)
}

fn has_pending_object_storage_local_changes(app: AppHandle) -> Result<bool, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_object_storage_settings(&connection, true)?;
    if !settings.enabled || !object_storage_configured(&settings) {
        return Ok(false);
    }
    let local_database_path = database_path(&app)?;
    let connection = open_database(&local_database_path)?;
    let device_id = load_or_create_device_id(&connection)?;
    let entity_versions = load_local_entity_versions(&connection)?;
    let payload = export_shared_sync_payload(
        &connection,
        device_id.clone(),
        Utc::now().timestamp_millis(),
    )?;

    Ok(!filter_passive_active_upload_operations(
        payload_entity_operations(&payload, &device_id, &entity_versions)?,
        &active_upload_filter_context(&payload, &device_id),
    )
    .is_empty())
}
fn parse_https_url(value: &str, label: &str) -> Result<Url, String> {
    let parsed = Url::parse(value).map_err(|_| format!("{label} is invalid; use an https URL."))?;
    if parsed.scheme() != "https" {
        return Err(format!("{label} must use https://."));
    }
    Ok(parsed)
}

fn normalize_settings(settings: WebDavSettings) -> Result<WebDavSettings, String> {
    let url = settings.url.trim().trim_end_matches('/').to_string();
    let username = settings.username.trim().to_string();
    let password_configured = !settings.password.is_empty() || settings.password_configured;
    let password = settings.password;
    let remote_path = settings
        .remote_path
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/");

    if settings.enabled {
        if url.is_empty() {
            return Err("Please enter the WebDAV URL.".to_string());
        }

        parse_https_url(&url, "WebDAV URL")?;

        if remote_path.is_empty() {
            return Err("Please enter the remote file path.".to_string());
        }
    } else if !url.is_empty() {
        parse_https_url(&url, "WebDAV URL")?;
    }

    Ok(WebDavSettings {
        enabled: settings.enabled,
        url,
        username,
        password,
        password_configured,
        remote_path: if remote_path.is_empty() {
            DEFAULT_REMOTE_PATH.to_string()
        } else {
            remote_path
        },
    })
}

fn normalize_object_storage_settings(
    settings: ObjectStorageSettings,
) -> Result<ObjectStorageSettings, String> {
    let endpoint = settings.endpoint.trim().trim_end_matches('/').to_string();
    let bucket = settings.bucket.trim().to_string();
    let access_key_id = settings.access_key_id.trim().to_string();
    let secret_access_key_configured =
        !settings.secret_access_key.is_empty() || settings.secret_access_key_configured;
    let secret_access_key = settings.secret_access_key;
    let region = settings.region.trim().to_string();
    let object_key = normalize_object_storage_key(&settings.object_key);

    if settings.enabled {
        if endpoint.is_empty() {
            return Err("Please enter the R2 endpoint.".to_string());
        }

        parse_https_url(&endpoint, "R2 endpoint")?;

        if bucket.is_empty() {
            return Err("Please enter the R2 bucket.".to_string());
        }

        if access_key_id.is_empty() {
            return Err("Please enter Access Key ID.".to_string());
        }

        if secret_access_key.trim().is_empty() {
            return Err("Please enter Secret Access Key.".to_string());
        }

        if object_key.contains("..") {
            return Err(
                "Object key is invalid; use a path like study-sync.json or a folder prefix."
                    .to_string(),
            );
        }
    } else if !endpoint.is_empty() {
        parse_https_url(&endpoint, "R2 endpoint")?;
    }

    Ok(ObjectStorageSettings {
        enabled: settings.enabled,
        endpoint,
        bucket,
        access_key_id,
        secret_access_key,
        secret_access_key_configured,
        region: if region.is_empty() {
            DEFAULT_OBJECT_REGION.to_string()
        } else {
            region
        },
        object_key,
    })
}

fn normalize_object_storage_key(raw_key: &str) -> String {
    let object_key = raw_key.trim().trim_start_matches('/').replace('\\', "/");

    if object_key.is_empty() || object_key == DEFAULT_REMOTE_PATH {
        DEFAULT_OBJECT_KEY.to_string()
    } else {
        object_key
    }
}

fn object_storage_configured(settings: &ObjectStorageSettings) -> bool {
    !settings.endpoint.trim().is_empty()
        && !settings.bucket.trim().is_empty()
        && !settings.access_key_id.trim().is_empty()
        && !settings.secret_access_key.trim().is_empty()
        && !settings.object_key.trim().is_empty()
}

fn read_webdav_settings(
    connection: &Connection,
    include_secret: bool,
) -> Result<WebDavSettings, String> {
    let password_configured = credential::secret_configured(connection, WEBDAV_PASSWORD_KEY)?;
    let password = if include_secret {
        credential::get_secret(connection, WEBDAV_PASSWORD_KEY)?
    } else {
        String::new()
    };
    Ok(WebDavSettings {
        enabled: get_bool_setting(connection, WEBDAV_ENABLED_KEY, true)?,
        url: get_setting(connection, WEBDAV_URL_KEY, "")?,
        username: get_setting(connection, WEBDAV_USERNAME_KEY, "")?,
        password,
        password_configured,
        remote_path: get_setting(connection, WEBDAV_REMOTE_PATH_KEY, DEFAULT_REMOTE_PATH)?,
    })
}

fn resolve_webdav_secret(
    connection: &Connection,
    mut settings: WebDavSettings,
) -> Result<WebDavSettings, String> {
    if settings.password.is_empty() {
        settings.password = credential::get_secret(connection, WEBDAV_PASSWORD_KEY)?;
    }
    settings.password_configured = !settings.password.is_empty();
    Ok(settings)
}

fn redact_webdav_settings(mut settings: WebDavSettings) -> WebDavSettings {
    settings.password_configured = !settings.password.is_empty() || settings.password_configured;
    settings.password.clear();
    settings
}

fn read_object_storage_settings(
    connection: &Connection,
    include_secret: bool,
) -> Result<ObjectStorageSettings, String> {
    let secret_access_key_configured =
        credential::secret_configured(connection, OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY)?;
    let secret_access_key = if include_secret {
        credential::get_secret(connection, OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY)?
    } else {
        String::new()
    };
    Ok(ObjectStorageSettings {
        enabled: get_bool_setting(connection, OBJECT_STORAGE_ENABLED_KEY, false)?,
        endpoint: get_setting(connection, OBJECT_STORAGE_ENDPOINT_KEY, "")?,
        bucket: get_setting(connection, OBJECT_STORAGE_BUCKET_KEY, "")?,
        access_key_id: get_setting(connection, OBJECT_STORAGE_ACCESS_KEY_ID_KEY, "")?,
        secret_access_key,
        secret_access_key_configured,
        region: get_setting(connection, OBJECT_STORAGE_REGION_KEY, DEFAULT_OBJECT_REGION)?,
        object_key: normalize_object_storage_key(&get_setting(
            connection,
            OBJECT_STORAGE_OBJECT_KEY_KEY,
            "",
        )?),
    })
}

fn resolve_object_storage_secret(
    connection: &Connection,
    mut settings: ObjectStorageSettings,
) -> Result<ObjectStorageSettings, String> {
    if settings.secret_access_key.is_empty() {
        settings.secret_access_key =
            credential::get_secret(connection, OBJECT_STORAGE_SECRET_ACCESS_KEY_KEY)?;
    }
    settings.secret_access_key_configured = !settings.secret_access_key.is_empty();
    Ok(settings)
}

fn redact_object_storage_settings(mut settings: ObjectStorageSettings) -> ObjectStorageSettings {
    settings.secret_access_key_configured =
        !settings.secret_access_key.is_empty() || settings.secret_access_key_configured;
    settings.secret_access_key.clear();
    settings
}

#[cfg(test)]
mod object_storage_settings_tests {
    use super::{normalize_object_storage_settings, normalize_settings, ObjectStorageSettings, WebDavSettings};

    fn webdav_settings(url: &str, enabled: bool) -> WebDavSettings {
        WebDavSettings {
            enabled,
            url: url.to_string(),
            username: "user".to_string(),
            password: "password".to_string(),
            password_configured: true,
            remote_path: "kaoyan-focus/kaoyan-focus.sqlite3".to_string(),
        }
    }

    fn object_storage_settings(endpoint: &str, enabled: bool) -> ObjectStorageSettings {
        ObjectStorageSettings {
            enabled,
            endpoint: endpoint.to_string(),
            bucket: "bucket".to_string(),
            access_key_id: "access-key".to_string(),
            secret_access_key: "secret-key".to_string(),
            secret_access_key_configured: true,
            region: "auto".to_string(),
            object_key: "study-sync.json".to_string(),
        }
    }

    #[test]
    fn webdav_settings_accept_https_url() {
        let normalized = normalize_settings(webdav_settings("https://example.com/webdav/", true))
            .expect("https webdav url should be accepted");

        assert_eq!(normalized.url, "https://example.com/webdav");
    }

    #[test]
    fn webdav_settings_reject_http_url_when_enabled() {
        let error = normalize_settings(webdav_settings("http://example.com/webdav", true))
            .expect_err("http webdav url should be rejected");

        assert!(error.contains("https"));
    }

    #[test]
    fn webdav_settings_reject_http_url_when_disabled_but_present() {
        let error = normalize_settings(webdav_settings("http://example.com/webdav", false))
            .expect_err("saved http webdav url should be rejected even when disabled");

        assert!(error.contains("https"));
    }

    #[test]
    fn object_storage_settings_accept_https_endpoint() {
        let normalized = normalize_object_storage_settings(object_storage_settings(
            "https://account.r2.cloudflarestorage.com/",
            true,
        ))
        .expect("https object storage endpoint should be accepted");

        assert_eq!(normalized.endpoint, "https://account.r2.cloudflarestorage.com");
    }

    #[test]
    fn object_storage_settings_reject_http_endpoint_when_enabled() {
        let error = normalize_object_storage_settings(object_storage_settings(
            "http://account.r2.cloudflarestorage.com",
            true,
        ))
        .expect_err("http object storage endpoint should be rejected");

        assert!(error.contains("https"));
    }

    #[test]
    fn object_storage_settings_reject_http_endpoint_when_disabled_but_present() {
        let error = normalize_object_storage_settings(object_storage_settings(
            "http://account.r2.cloudflarestorage.com",
            false,
        ))
        .expect_err("saved http object storage endpoint should be rejected even when disabled");

        assert!(error.contains("https"));
    }
}

