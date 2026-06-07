fn backup_database_path_with_prefix(app_data_dir: &Path, prefix: &str) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    app_data_dir.join(format!("kaoyan-focus.before-{prefix}-{stamp}.sqlite3"))
}

fn create_local_sync_backup(
    app: &AppHandle,
    local_database_path: &Path,
    prefix: &str,
) -> Result<Option<String>, String> {
    if !local_database_path.exists() {
        return Ok(None);
    }
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_database_path_with_prefix(&app_data_dir, prefix);
    fs::copy(local_database_path, &backup_path)
        .map_err(|error| format!("Create local sync backup failed: {error}"))?;
    prune_local_sync_backups_best_effort(&app_data_dir);
    Ok(Some(backup_path.to_string_lossy().to_string()))
}

fn list_object_storage_backup_entries(
    settings: &ObjectStorageSettings,
) -> Result<Vec<SyncBackupEntry>, String> {
    with_s3_runtime(async {
        let client = object_storage_client(settings).await?;
        let entries = list_all_object_storage_backup_objects(&client, settings)
            .await?
            .into_iter()
            .map(|object| SyncBackupEntry {
                source: "r2".to_string(),
                label: object
                    .key
                    .rsplit('/')
                    .next()
                    .unwrap_or(&object.key)
                    .to_string(),
                key: object.key,
                created_at: object.last_modified.map(|date| date.to_rfc3339()),
                bytes: object.size,
            })
            .collect();
        Ok(entries)
    })
}

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS).expect("valid UTC+8 offset")
}

fn current_backup_keep_dates(now: DateTime<Utc>) -> (NaiveDate, HashSet<NaiveDate>) {
    let today = now.with_timezone(&beijing_offset()).date_naive();
    let yesterday = (now - ChronoDuration::days(1))
        .with_timezone(&beijing_offset())
        .date_naive();
    let mut keep_dates = HashSet::new();
    keep_dates.insert(today);
    keep_dates.insert(yesterday);
    (today, keep_dates)
}

fn should_delete_backup(
    modified_at: Option<DateTime<Utc>>,
    today: NaiveDate,
    keep_dates: &HashSet<NaiveDate>,
) -> bool {
    let Some(modified_at) = modified_at else {
        return false;
    };
    let backup_date = modified_at.with_timezone(&beijing_offset()).date_naive();
    if backup_date > today {
        return false;
    }
    !keep_dates.contains(&backup_date)
}

fn prune_local_sync_backups(app_data_dir: &Path) -> Result<(), String> {
    if !app_data_dir.exists() {
        return Ok(());
    }

    let (today, keep_dates) = current_backup_keep_dates(Utc::now());
    for entry in fs::read_dir(app_data_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("kaoyan-focus.before-") || !name.ends_with(".sqlite3") {
            continue;
        }
        let modified_at = match entry.metadata() {
            Ok(metadata) => metadata.modified().ok().map(DateTime::<Utc>::from),
            Err(error) => {
                eprintln!(
                    "Keep local sync backup without metadata {}: {error}",
                    path.display()
                );
                None
            }
        };
        if !should_delete_backup(modified_at, today, &keep_dates) {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Delete local sync backup {} failed: {error}",
                    path.display()
                ));
            }
        }
    }

    Ok(())
}

fn prune_local_sync_backups_best_effort(app_data_dir: &Path) {
    if let Err(error) = prune_local_sync_backups(app_data_dir) {
        eprintln!("Local sync backup prune failed: {error}");
    }
}

async fn prune_object_storage_backups_with_client(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<(), String> {
    let (today, keep_dates) = current_backup_keep_dates(Utc::now());
    for object in list_all_object_storage_backup_objects(client, settings).await? {
        if object.last_modified.is_none() {
            eprintln!("Keep R2 backup without last_modified: {}", object.key);
            continue;
        }
        if !should_delete_backup(object.last_modified, today, &keep_dates) {
            continue;
        }
        delete_object_if_exists(client, settings, &object.key).await?;
    }
    Ok(())
}

async fn prune_object_storage_backups_with_client_best_effort(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) {
    if let Err(error) = prune_object_storage_backups_with_client(client, settings).await {
        eprintln!("R2 sync backup prune failed: {error}");
    }
}

fn prune_object_storage_backups(settings: &ObjectStorageSettings) -> Result<(), String> {
    with_s3_runtime(async {
        let client = object_storage_client(settings).await?;
        prune_object_storage_backups_with_client(&client, settings).await
    })
}

pub(crate) fn prune_sync_backups_best_effort(app: &AppHandle) {
    match app.path().app_data_dir() {
        Ok(app_data_dir) => prune_local_sync_backups_best_effort(&app_data_dir),
        Err(error) => eprintln!("Resolve app data dir for backup prune failed: {error}"),
    }

    match database_path(app)
        .and_then(|path| open_database(&path))
        .and_then(|connection| read_object_storage_settings(&connection, true))
    {
        Ok(settings) if settings.enabled && object_storage_configured(&settings) => {
            match normalize_object_storage_settings(settings) {
                Ok(normalized) => {
                    if let Err(error) = prune_object_storage_backups(&normalized) {
                        eprintln!("Scheduled R2 sync backup prune failed: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Normalize object storage settings for prune failed: {error}")
                }
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("Load object storage settings for prune failed: {error}"),
    }
}

fn load_backup_bytes(app: &AppHandle, source: &str, key: &str) -> Result<Vec<u8>, String> {
    if source == "local" {
        let path = PathBuf::from(key);
        return fs::read(&path).map_err(|error| format!("Read local backup failed: {error}"));
    }
    let connection = open_database(&database_path(app)?)?;
    let settings =
        normalize_object_storage_settings(read_object_storage_settings(&connection, true)?)?;
    with_s3_runtime(async {
        let client = object_storage_client(&settings).await?;
        let response = client
            .get_object()
            .bucket(&settings.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| format!("Download R2/S3 backup failed: {error}"))?;
        let bytes = response
            .body
            .collect()
            .await
            .map_err(|error| format!("Read R2/S3 backup failed: {error}"))?
            .into_bytes();
        Ok(bytes.to_vec())
    })
}


