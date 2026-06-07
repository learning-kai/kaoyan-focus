use crate::{runtime_health, storage::db::open_database};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn get_runtime_health(app: AppHandle) -> Result<runtime_health::RuntimeHealth, String> {
    let connection = open_database(&database_path(&app)?)?;
    runtime_health::runtime_health_from_database(&connection)
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}
