fn webdav_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

fn webdav_request(
    client: &Client,
    method: Method,
    url: Url,
    settings: &WebDavSettings,
) -> reqwest::blocking::RequestBuilder {
    let request = client.request(method, url);
    if settings.username.is_empty() && settings.password.is_empty() {
        request
    } else {
        request.basic_auth(&settings.username, Some(&settings.password))
    }
}

fn remote_file_url(settings: &WebDavSettings) -> Result<Url, String> {
    let base = format!("{}/", settings.url.trim_end_matches('/'));
    let mut url = Url::parse(&base).map_err(|error| error.to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "WebDAV URL does not support path segments.".to_string())?;
        for segment in settings
            .remote_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            segments.push(segment);
        }
    }
    Ok(url)
}

fn ensure_remote_directories(client: &Client, settings: &WebDavSettings) -> Result<(), String> {
    let mut current = Url::parse(&format!("{}/", settings.url.trim_end_matches('/')))
        .map_err(|error| error.to_string())?;
    let parts: Vec<&str> = settings
        .remote_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if parts.len() <= 1 {
        return Ok(());
    }

    for part in &parts[..parts.len() - 1] {
        {
            let mut segments = current
                .path_segments_mut()
                .map_err(|_| "WebDAV URL does not support directory creation.".to_string())?;
            segments.push(part);
        }

        let response = webdav_request(
            client,
            Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
            current.clone(),
            settings,
        )
        .send()
        .map_err(|error| format!("Create remote WebDAV directory failed: {error}"))?;

        if response.status().is_success()
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
            || response.status() == StatusCode::CONFLICT
        {
            continue;
        }

        return Err(format!(
            "Create remote WebDAV directory failed with status: {}",
            response.status().as_u16()
        ));
    }

    Ok(())
}


fn validate_sqlite_database(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("File is not a valid SQLite database: {error}"))?;
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("SQLite integrity_check failed: {error}"))
        .and_then(|result| {
            if result == "ok" {
                Ok(())
            } else {
                Err(format!("SQLite integrity_check failed: {result}"))
            }
        })
}

fn ensure_no_active_runtime(path: &Path) -> Result<(), String> {
    if has_active_runtime(path)? {
        return Err(
            "A study mode is currently running; finish it before restoring data.".to_string(),
        );
    }

    Ok(())
}

fn has_active_runtime(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let connection = open_database(path)?;
    let active_runtime_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM study_modes sm
            WHERE sm.status = 'active'
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(active_runtime_count > 0)
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn fetch_remote_file_metadata(
    client: &Client,
    remote_url: &Url,
    settings: &WebDavSettings,
) -> Result<RemoteFileMetadata, String> {
    let response = webdav_request(client, Method::HEAD, remote_url.clone(), settings)
        .send()
        .map_err(|error| format!("Read WebDAV metadata failed: {error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(RemoteFileMetadata {
            exists: false,
            size: None,
            last_modified: None,
        });
    }

    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        return Ok(RemoteFileMetadata {
            exists: true,
            size: None,
            last_modified: None,
        });
    }

    if !response.status().is_success() {
        return Err(format!(
            "Read WebDAV metadata failed with status: {}",
            response.status().as_u16()
        ));
    }

    let headers = response.headers().clone();
    Ok(RemoteFileMetadata {
        exists: true,
        size: content_length(&headers),
        last_modified: parse_last_modified(&headers),
    })
}

fn parse_last_modified(headers: &HeaderMap) -> Option<DateTime<Utc>> {
    headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn local_database_modified_at(path: &Path) -> Result<DateTime<Utc>, String> {
    let modified = fs::metadata(path)
        .map_err(|error| format!("Read local database metadata failed: {error}"))?
        .modified()
        .map_err(|error| format!("Read local database modified time failed: {error}"))?;
    Ok(modified.into())
}

fn skipped_auto_sync(
    reason: &str,
    message: &str,
    remote_url: Option<String>,
) -> WebDavAutoSyncResult {
    WebDavAutoSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        direction: None,
        skipped_reason: Some(reason.to_string()),
        synced_at: Utc::now().to_rfc3339(),
        remote_url,
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
        primary_owner_changed: false,
    }
}

fn skipped_object_storage_auto_sync(
    reason: &str,
    message: &str,
    object_url: Option<String>,
) -> ObjectStorageAutoSyncResult {
    ObjectStorageAutoSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        direction: None,
        skipped_reason: Some(reason.to_string()),
        synced_at: Utc::now().to_rfc3339(),
        object_url,
        bytes: 0,
        backup_path: None,
        active_state_changed: false,
        took_over_active_mode: false,
        primary_owner_changed: false,
    }
}

fn emit_study_sync_state_changed(app: &AppHandle, result: &ObjectStorageAutoSyncResult) {
    if !result.active_state_changed && !result.primary_owner_changed {
        return;
    }

    let _ = app.emit(STUDY_SYNC_STATE_CHANGED_EVENT, result);
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }

    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
