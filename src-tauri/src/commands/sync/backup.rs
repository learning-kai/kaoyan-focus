fn backup_database_path_with_prefix(app_data_dir: &Path, prefix: &str) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    app_data_dir.join(format!("kaoyan-focus.before-{prefix}-{stamp}.sqlite3"))
}

fn sqlite_sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut value = database_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Remove {} failed: {error}", path.display())),
    }
}

fn remove_sqlite_sidecars_best_effort(database_path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let sidecar = sqlite_sidecar_path(database_path, suffix);
        if let Err(error) = remove_file_if_exists(&sidecar) {
            eprintln!("SQLite sidecar cleanup skipped for {}: {error}", sidecar.display());
        }
    }
}

fn create_sqlite_snapshot(source_path: &Path, snapshot_path: &Path) -> Result<(), String> {
    if !source_path.exists() {
        return Err(format!("Local database does not exist: {}", source_path.display()));
    }

    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    remove_file_if_exists(snapshot_path)?;
    remove_sqlite_sidecars_best_effort(snapshot_path);

    let connection = Connection::open(source_path)
        .map_err(|error| format!("Open local database for snapshot failed: {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|error| error.to_string())?;
    let snapshot = snapshot_path.to_string_lossy().to_string();
    connection
        .execute("VACUUM INTO ?1", rusqlite::params![snapshot])
        .map_err(|error| format!("Create SQLite snapshot failed: {error}"))?;
    validate_sqlite_database(snapshot_path)?;
    Ok(())
}

fn replace_local_database_from_temp(
    app: &AppHandle,
    local_database_path: &Path,
    temp_path: &Path,
    backup_prefix: &str,
) -> Result<Option<String>, String> {
    validate_sqlite_database(temp_path)?;
    ensure_no_active_runtime(local_database_path)?;
    let backup_path = create_local_sync_backup(app, local_database_path, backup_prefix)?;

    remove_sqlite_sidecars_best_effort(local_database_path);
    fs::rename(temp_path, local_database_path)
        .or_else(|_| {
            fs::copy(temp_path, local_database_path)?;
            fs::remove_file(temp_path)
        })
        .map_err(|error| format!("Replace local database failed: {error}"))?;
    remove_sqlite_sidecars_best_effort(local_database_path);
    open_database(local_database_path)?;
    Ok(backup_path)
}

fn is_local_sync_backup_file_name(name: &str) -> bool {
    name.starts_with("kaoyan-focus.before-") && name.ends_with(".sqlite3")
}

fn resolve_local_sync_backup_path(app_data_dir: &Path, key: &str) -> Result<PathBuf, String> {
    let requested = PathBuf::from(key);
    let name = requested
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid local backup key.".to_string())?;

    if !is_local_sync_backup_file_name(name) {
        return Err("Invalid local backup key.".to_string());
    }

    let candidate = if requested.is_absolute() {
        requested
    } else if requested.components().count() == 1 {
        app_data_dir.join(requested)
    } else {
        return Err("Invalid local backup key.".to_string());
    };

    let canonical_app_data_dir = fs::canonicalize(app_data_dir)
        .map_err(|error| format!("Resolve app data dir failed: {error}"))?;
    let canonical_candidate = fs::canonicalize(&candidate)
        .map_err(|error| format!("Resolve local backup failed: {error}"))?;

    if canonical_candidate.parent() != Some(canonical_app_data_dir.as_path()) || !canonical_candidate.is_file() {
        return Err("Invalid local backup key.".to_string());
    }

    Ok(canonical_candidate)
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
    create_sqlite_snapshot(local_database_path, &backup_path)
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
        if !is_local_sync_backup_file_name(name) {
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
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        let path = resolve_local_sync_backup_path(&app_data_dir, key)?;
        return fs::read(&path).map_err(|error| format!("Read local backup failed: {error}"));
    }
    if source != "r2" {
        return Err("Unknown backup source.".to_string());
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

#[cfg(test)]
mod backup_tests {
    use super::{create_sqlite_snapshot, is_local_sync_backup_file_name, resolve_local_sync_backup_path};
    use rusqlite::Connection;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn local_backup_file_name_accepts_expected_pattern() {
        assert!(is_local_sync_backup_file_name(
            "kaoyan-focus.before-r2-v3-auto-20260607000000.sqlite3"
        ));
        assert!(!is_local_sync_backup_file_name("kaoyan-focus.sqlite3"));
        assert!(!is_local_sync_backup_file_name(
            "kaoyan-focus.before-r2-v3-auto-20260607000000.txt"
        ));
    }

    #[test]
    fn sqlite_snapshot_includes_wal_commits() {
        let temp = tempdir().expect("tempdir");
        let database_path = temp.path().join("source.sqlite3");
        let snapshot_path = temp.path().join("snapshot.sqlite3");
        let connection = Connection::open(&database_path).expect("open source database");
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                CREATE TABLE records (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                INSERT INTO records (value) VALUES ('from-wal');
                ",
            )
            .expect("seed source database");

        create_sqlite_snapshot(&database_path, &snapshot_path).expect("create sqlite snapshot");
        let snapshot = Connection::open(&snapshot_path).expect("open snapshot");
        let value: String = snapshot
            .query_row("SELECT value FROM records WHERE id = 1", [], |row| row.get(0))
            .expect("read snapshot value");
        assert_eq!(value, "from-wal");
    }

    #[test]
    fn local_backup_key_accepts_file_under_app_data() {
        let app_data_dir = tempdir().expect("tempdir");
        let backup_path = app_data_dir
            .path()
            .join("kaoyan-focus.before-r2-v3-auto-20260607000000.sqlite3");
        fs::write(&backup_path, b"backup").expect("write backup");

        let resolved = resolve_local_sync_backup_path(app_data_dir.path(), &backup_path.to_string_lossy())
            .expect("resolve absolute backup path");
        assert_eq!(resolved, fs::canonicalize(&backup_path).expect("canonical backup"));

        let resolved_by_name = resolve_local_sync_backup_path(
            app_data_dir.path(),
            "kaoyan-focus.before-r2-v3-auto-20260607000000.sqlite3",
        )
        .expect("resolve backup file name");
        assert_eq!(resolved_by_name, resolved);
    }

    #[test]
    fn local_backup_key_rejects_file_outside_app_data() {
        let app_data_dir = tempdir().expect("app data tempdir");
        let outside_dir = tempdir().expect("outside tempdir");
        let outside_backup = outside_dir
            .path()
            .join("kaoyan-focus.before-r2-v3-auto-20260607000000.sqlite3");
        fs::write(&outside_backup, b"backup").expect("write outside backup");

        assert!(resolve_local_sync_backup_path(app_data_dir.path(), &outside_backup.to_string_lossy()).is_err());
    }

    #[test]
    fn local_backup_key_rejects_non_backup_file_inside_app_data() {
        let app_data_dir = tempdir().expect("tempdir");
        let local_database = app_data_dir.path().join("kaoyan-focus.sqlite3");
        fs::write(&local_database, b"database").expect("write database");

        assert!(resolve_local_sync_backup_path(app_data_dir.path(), &local_database.to_string_lossy()).is_err());
    }

    #[test]
    fn local_backup_key_rejects_relative_path_with_parent_component() {
        let app_data_dir = tempdir().expect("app data tempdir");

        assert!(resolve_local_sync_backup_path(
            app_data_dir.path(),
            "../kaoyan-focus.before-r2-v3-auto-20260607000000.sqlite3"
        )
        .is_err());
    }
}


