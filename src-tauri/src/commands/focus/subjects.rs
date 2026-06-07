#[tauri::command]
pub fn list_subjects(app: AppHandle) -> Result<Vec<Subject>, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3");
    let connection = open_database(&db_path)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, name, color, enabled, created_at, updated_at
            FROM subjects
            WHERE enabled = 1
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let subjects = statement
        .query_map([], row_to_subject)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(subjects)
}

