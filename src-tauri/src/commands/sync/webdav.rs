#[tauri::command]
pub fn get_webdav_settings(app: AppHandle) -> Result<WebDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    read_webdav_settings(&connection, false)
}

#[tauri::command]
pub fn save_webdav_settings(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let password_changed = !settings.password.is_empty();
    let normalized = normalize_settings(resolve_webdav_secret(&connection, settings)?)?;
    persist_webdav_settings(&connection, &normalized, password_changed)?;

    Ok(redact_webdav_settings(normalized))
}

#[tauri::command]
pub fn test_webdav_connection(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavStatus, String> {
    let connection = open_database(&database_path(&app)?)?;
    let password_changed = !settings.password.is_empty();
    let normalized = normalize_settings(resolve_webdav_secret(&connection, settings)?)?;
    persist_webdav_settings(&connection, &normalized, password_changed)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(
        &client,
        Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?,
        remote_url.clone(),
        &normalized,
    )
    .header("Depth", "0")
    .body("")
    .send()
    .map_err(|error| format!("Connect to WebDAV failed: {error}"))?;

    let status = response.status();
    if status == StatusCode::OK || status.as_u16() == 207 {
        let headers = response.headers().clone();
        return Ok(WebDavStatus {
            configured: true,
            url: normalized.url,
            username: normalized.username,
            remote_path: normalized.remote_path,
            remote_exists: true,
            remote_size: content_length(&headers),
            last_modified: header_string(&headers, "last-modified"),
            message: "WebDAV connection succeeded; remote sync file is accessible.".to_string(),
        });
    }

    if status == StatusCode::NOT_FOUND {
        return Ok(WebDavStatus {
            configured: true,
            url: normalized.url,
            username: normalized.username,
            remote_path: normalized.remote_path,
            remote_exists: false,
            remote_size: None,
            last_modified: None,
            message: "WebDAV connection succeeded; remote sync file does not exist yet."
                .to_string(),
        });
    }

    Err(format!(
        "WebDAV connection failed with status: {}",
        status.as_u16()
    ))
}

#[tauri::command]
pub fn upload_database_to_webdav(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSyncResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let password_changed = !settings.password.is_empty();
    let normalized = normalize_settings(resolve_webdav_secret(&connection, settings)?)?;
    persist_webdav_settings(&connection, &normalized, password_changed)?;
    let database_path = database_path(&app)?;
    let bytes =
        fs::read(&database_path).map_err(|error| format!("Read local database failed: {error}"))?;

    if bytes.is_empty() {
        return Err("Local database is empty; cannot upload.".to_string());
    }

    let client = webdav_client()?;
    ensure_remote_directories(&client, &normalized)?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::PUT, remote_url.clone(), &normalized)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|error| format!("Upload to WebDAV failed: {error}"))?;

    if response.status().is_success()
        || response.status() == StatusCode::CREATED
        || response.status() == StatusCode::NO_CONTENT
    {
        return Ok(WebDavSyncResult {
            success: true,
            message: "Uploaded to WebDAV successfully.".to_string(),
            remote_url: remote_url.to_string(),
            bytes: fs::metadata(&database_path)
                .map(|meta| meta.len())
                .unwrap_or(0),
            backup_path: None,
        });
    }

    Err(format!(
        "Upload to WebDAV failed with status: {}",
        response.status().as_u16()
    ))
}

#[tauri::command]
pub fn download_database_from_webdav(
    app: AppHandle,
    settings: WebDavSettings,
) -> Result<WebDavSyncResult, String> {
    let password_changed = !settings.password.is_empty();
    let normalized = {
        let connection = open_database(&database_path(&app)?)?;
        normalize_settings(resolve_webdav_secret(&connection, settings)?)?
    };
    let local_database_path = database_path(&app)?;
    ensure_no_active_runtime(&local_database_path)?;
    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let response = webdav_request(&client, Method::GET, remote_url.clone(), &normalized)
        .send()
        .map_err(|error| format!("Download from WebDAV failed: {error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Err("Remote WebDAV sync file does not exist.".to_string());
    }

    if !response.status().is_success() {
        return Err(format!(
            "Download from WebDAV failed with status: {}",
            response.status().as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("Read WebDAV response failed: {error}"))?;

    if bytes.is_empty() {
        return Err("WebDAV returned an empty file.".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let temp_path = app_data_dir.join("kaoyan-focus.webdav-download.tmp");
    fs::write(&temp_path, &bytes)
        .map_err(|error| format!("Write temporary file failed: {error}"))?;
    validate_sqlite_database(&temp_path)?;

    let backup_path = create_local_sync_backup(&app, &local_database_path, "webdav")?;

    fs::rename(&temp_path, &local_database_path)
        .or_else(|_| {
            fs::copy(&temp_path, &local_database_path)?;
            fs::remove_file(&temp_path)
        })
        .map_err(|error| format!("Replace local database failed: {error}"))?;

    let connection = open_database(&local_database_path)?;
    persist_webdav_settings(&connection, &normalized, password_changed)?;

    Ok(WebDavSyncResult {
        success: true,
        message: "Downloaded and validated the database from WebDAV.".to_string(),
        remote_url: remote_url.to_string(),
        bytes: bytes.len() as u64,
        backup_path,
    })
}

#[tauri::command]
pub fn auto_sync_webdav_database(app: AppHandle) -> Result<WebDavAutoSyncResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_webdav_settings(&connection, true)?;
    if !settings.enabled {
        return Ok(skipped_auto_sync(
            "webdav_disabled",
            "WebDAV sync is disabled; automatic sync skipped.",
            None,
        ));
    }

    if settings.url.trim().is_empty() {
        return Ok(skipped_auto_sync(
            "webdav_not_configured",
            "WebDAV is not configured; automatic sync skipped.",
            None,
        ));
    }

    let normalized = normalize_settings(settings)?;
    let local_database_path = database_path(&app)?;
    if has_active_runtime(&local_database_path)? {
        return Ok(skipped_auto_sync(
            "study_mode_active",
            "Study mode is running; WebDAV automatic sync skipped.",
            Some(remote_file_url(&normalized)?.to_string()),
        ));
    }

    let local_modified = match local_database_modified_at(&local_database_path) {
        Ok(value) => value,
        Err(message) => {
            return Ok(skipped_auto_sync(
                "local_timestamp_unavailable",
                &message,
                Some(remote_file_url(&normalized)?.to_string()),
            ));
        }
    };

    let client = webdav_client()?;
    let remote_url = remote_file_url(&normalized)?;
    let remote_metadata = fetch_remote_file_metadata(&client, &remote_url, &normalized)?;

    if !remote_metadata.exists {
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "Remote sync file did not exist; uploaded local database.".to_string(),
            direction: Some("upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: None,
            active_state_changed: false,
            took_over_active_mode: false,
            primary_owner_changed: false,
        });
    }

    let Some(remote_modified) = remote_metadata.last_modified else {
        return Ok(skipped_auto_sync(
            "remote_timestamp_unavailable",
            "Remote did not return Last-Modified; automatic sync skipped.",
            Some(remote_url.to_string()),
        ));
    };

    let tolerance = ChronoDuration::seconds(2);
    if remote_modified > local_modified + tolerance {
        let download_result = download_database_from_webdav(app.clone(), normalized.clone())?;
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "Remote data was newer; downloaded, validated, backed up, and uploaded merged database.".to_string(),
            direction: Some("download_upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: download_result.backup_path,
            active_state_changed: false,
            took_over_active_mode: false,
            primary_owner_changed: false,
        });
    }

    if local_modified > remote_modified + tolerance {
        let upload_result = upload_database_to_webdav(app, normalized)?;
        return Ok(WebDavAutoSyncResult {
            status: "synced".to_string(),
            message: "Local data was newer; uploaded to WebDAV.".to_string(),
            direction: Some("upload".to_string()),
            skipped_reason: None,
            synced_at: Utc::now().to_rfc3339(),
            remote_url: Some(upload_result.remote_url),
            bytes: upload_result.bytes,
            backup_path: None,
            active_state_changed: false,
            took_over_active_mode: false,
            primary_owner_changed: false,
        });
    }

    Ok(WebDavAutoSyncResult {
        status: "skipped".to_string(),
        message: format!(
            "Local and remote timestamps are close; automatic sync skipped. Remote size: {}",
            remote_metadata
                .size
                .map(format_bytes)
                .unwrap_or_else(|| "unknown".to_string())
        ),
        direction: None,
        skipped_reason: Some("up_to_date".to_string()),
        synced_at: Utc::now().to_rfc3339(),
        remote_url: Some(remote_url.to_string()),
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
        primary_owner_changed: false,
    })
}

