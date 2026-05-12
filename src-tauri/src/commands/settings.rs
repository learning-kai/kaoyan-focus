use crate::storage::db::open_database;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const DEFAULT_FOCUS_MINUTES_KEY: &str = "default_focus_minutes";
const DEFAULT_STUDY_MINUTES_KEY: &str = "default_study_minutes";
const DEFAULT_FOCUS_MODE_KEY: &str = "default_focus_mode";
const BREAK_MINUTES_KEY: &str = "break_minutes";
const EMERGENCY_COOLDOWN_SECONDS_KEY: &str = "emergency_cooldown_seconds";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_study_minutes: i64,
    pub default_focus_minutes: i64,
    pub break_minutes: i64,
    pub default_focus_mode: String,
    pub emergency_cooldown_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppDataLocation {
    pub app_data_dir: String,
    pub database_path: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_study_minutes: 120,
            default_focus_minutes: 25,
            break_minutes: 5,
            default_focus_mode: "normal".to_string(),
            emergency_cooldown_seconds: 60,
        }
    }
}

#[tauri::command]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let defaults = AppSettings::default();

    Ok(AppSettings {
        default_study_minutes: get_i64_setting(
            &connection,
            DEFAULT_STUDY_MINUTES_KEY,
            defaults.default_study_minutes,
        )?
        .clamp(1, 720),
        default_focus_minutes: get_i64_setting(&connection, DEFAULT_FOCUS_MINUTES_KEY, defaults.default_focus_minutes)?
            .clamp(1, 120),
        break_minutes: get_i64_setting(&connection, BREAK_MINUTES_KEY, defaults.break_minutes)?.clamp(1, 60),
        default_focus_mode: normalize_mode(&get_string_setting(
            &connection,
            DEFAULT_FOCUS_MODE_KEY,
            &defaults.default_focus_mode,
        )?),
        emergency_cooldown_seconds: get_i64_setting(
            &connection,
            EMERGENCY_COOLDOWN_SECONDS_KEY,
            defaults.emergency_cooldown_seconds,
        )?
        .clamp(0, 300),
    })
}

#[tauri::command]
pub fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let normalized = AppSettings {
        default_study_minutes: settings.default_study_minutes.clamp(1, 720),
        default_focus_minutes: settings.default_focus_minutes.clamp(1, 120),
        break_minutes: settings.break_minutes.clamp(1, 60),
        default_focus_mode: normalize_mode(&settings.default_focus_mode),
        emergency_cooldown_seconds: settings.emergency_cooldown_seconds.clamp(0, 300),
    };
    let now = Utc::now().to_rfc3339();

    set_setting(
        &connection,
        DEFAULT_STUDY_MINUTES_KEY,
        &normalized.default_study_minutes.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        DEFAULT_FOCUS_MINUTES_KEY,
        &normalized.default_focus_minutes.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        BREAK_MINUTES_KEY,
        &normalized.break_minutes.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        DEFAULT_FOCUS_MODE_KEY,
        &normalized.default_focus_mode,
        &now,
    )?;
    set_setting(
        &connection,
        EMERGENCY_COOLDOWN_SECONDS_KEY,
        &normalized.emergency_cooldown_seconds.to_string(),
        &now,
    )?;

    Ok(normalized)
}

#[tauri::command]
pub fn get_app_data_location(app: AppHandle) -> Result<AppDataLocation, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let database_path = app_data_dir.join("kaoyan-focus.sqlite3");

    Ok(AppDataLocation {
        app_data_dir: app_data_dir.to_string_lossy().to_string(),
        database_path: database_path.to_string_lossy().to_string(),
    })
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn get_string_setting(
    connection: &rusqlite::Connection,
    key: &str,
    fallback: &str,
) -> Result<String, String> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn get_i64_setting(
    connection: &rusqlite::Connection,
    key: &str,
    fallback: i64,
) -> Result<i64, String> {
    let raw = get_string_setting(connection, key, &fallback.to_string())?;
    Ok(raw.parse::<i64>().unwrap_or(fallback))
}

fn set_setting(
    connection: &rusqlite::Connection,
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
            params![key, value, updated_at],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn normalize_mode(mode: &str) -> String {
    if mode == "strict" {
        "strict".to_string()
    } else {
        "normal".to_string()
    }
}
