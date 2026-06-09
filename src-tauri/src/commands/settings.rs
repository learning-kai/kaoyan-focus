use crate::storage::db::open_database;
use crate::sync_package::load_or_create_device_id;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::Path};
use tauri::{AppHandle, Manager};

const DEFAULT_FOCUS_MINUTES_KEY: &str = "default_focus_minutes";
const DEFAULT_STUDY_MINUTES_KEY: &str = "default_study_minutes";
const DEFAULT_FOCUS_MODE_KEY: &str = "default_focus_mode";
const UI_THEME_KEY: &str = "ui_theme";
const LAUNCH_AT_STARTUP_KEY: &str = "launch_at_startup";
const AUTO_START_BREAK_AFTER_FOCUS_KEY: &str = "auto_start_break_after_focus";
const SCHEDULE_REMINDER_ENABLED_KEY: &str = "schedule_reminder_enabled";
const SCHEDULE_REMINDER_LEAD_MINUTES_KEY: &str = "schedule_reminder_lead_minutes";
const FOCUS_WIDGET_ENABLED_KEY: &str = "focus_widget_enabled";
const FOCUS_WIDGET_AUTO_FOLLOW_KEY: &str = "focus_widget_auto_follow";
const FOCUS_WIDGET_REMEMBER_GEOMETRY_KEY: &str = "focus_widget_remember_geometry";
const FOCUS_WIDGET_X_KEY: &str = "focus_widget_x";
const FOCUS_WIDGET_Y_KEY: &str = "focus_widget_y";
const FOCUS_WIDGET_WIDTH_KEY: &str = "focus_widget_width";
const FOCUS_WIDGET_HEIGHT_KEY: &str = "focus_widget_height";
const BREAK_MINUTES_KEY: &str = "break_minutes";
const LONG_BREAK_MINUTES_KEY: &str = "long_break_minutes";
const LONG_BREAK_INTERVAL_KEY: &str = "long_break_interval";
const EMERGENCY_COOLDOWN_SECONDS_KEY: &str = "emergency_cooldown_seconds";
const CHECKLIST_CATEGORY_NAMES_KEY: &str = "checklist_category_names";
const SYNC_BACKEND_KEY: &str = "sync_backend";
const PRIMARY_OWNER_DEVICE_ID_KEY: &str = "primary_owner_device_id";
const PRIMARY_OWNER_UPDATED_AT_KEY: &str = "primary_owner_updated_at";
const REMINDER_SOUND_SOURCE_KEY: &str = "reminder_sound_source";
const REMINDER_SOUND_ID_KEY: &str = "reminder_sound_id";
const REMINDER_SOUND_FILE_NAME_KEY: &str = "reminder_sound_file_name";
const REMINDER_SOUND_UPDATED_AT_KEY: &str = "reminder_sound_updated_at";
const REMINDER_SOUND_VOLUME_KEY: &str = "reminder_sound_volume";
const REMINDER_QUIET_HOURS_ENABLED_KEY: &str = "reminder_quiet_hours_enabled";
const REMINDER_QUIET_HOURS_START_KEY: &str = "reminder_quiet_hours_start";
const REMINDER_QUIET_HOURS_END_KEY: &str = "reminder_quiet_hours_end";
const DEFAULT_REMINDER_SOUND_SOURCE: &str = "builtin";
const DEFAULT_REMINDER_SOUND_ID: &str = "classic";
const CUSTOM_REMINDER_SOUND_FILE: &str = "custom-reminder-sound";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_study_minutes: i64,
    pub default_focus_minutes: i64,
    pub break_minutes: i64,
    pub long_break_minutes: i64,
    pub long_break_interval: i64,
    pub default_focus_mode: String,
    pub ui_theme: String,
    pub launch_at_startup: bool,
    pub auto_start_break_after_focus: bool,
    pub schedule_reminder_enabled: bool,
    pub schedule_reminder_lead_minutes: i64,
    pub focus_widget_enabled: bool,
    pub focus_widget_auto_follow: bool,
    pub focus_widget_remember_geometry: bool,
    pub focus_widget_x: Option<i64>,
    pub focus_widget_y: Option<i64>,
    pub focus_widget_width: Option<i64>,
    pub focus_widget_height: Option<i64>,
    pub sync_backend: String,
    pub primary_owner_device_id: Option<String>,
    pub primary_owner_updated_at: Option<i64>,
    pub emergency_cooldown_seconds: i64,
    pub checklist_category_names: String,
    pub reminder_sound_source: String,
    pub reminder_sound_id: String,
    pub reminder_sound_file_name: Option<String>,
    pub reminder_sound_updated_at: Option<i64>,
    pub reminder_sound_volume: i64,
    pub reminder_quiet_hours_enabled: bool,
    pub reminder_quiet_hours_start: String,
    pub reminder_quiet_hours_end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppDataLocation {
    pub app_data_dir: String,
    pub database_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSoundFile {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReminderSoundData {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_study_minutes: 120,
            default_focus_minutes: 25,
            break_minutes: 5,
            long_break_minutes: 15,
            long_break_interval: 4,
            default_focus_mode: "normal".to_string(),
            ui_theme: "dark".to_string(),
            launch_at_startup: false,
            auto_start_break_after_focus: false,
            schedule_reminder_enabled: true,
            schedule_reminder_lead_minutes: 5,
            focus_widget_enabled: false,
            focus_widget_auto_follow: true,
            focus_widget_remember_geometry: true,
            focus_widget_x: None,
            focus_widget_y: None,
            focus_widget_width: Some(280),
            focus_widget_height: Some(144),
            sync_backend: "webdav".to_string(),
            primary_owner_device_id: None,
            primary_owner_updated_at: None,
            emergency_cooldown_seconds: 60,
            checklist_category_names:
                "{\"politics\":\"政治\",\"english\":\"英语\",\"math\":\"数学\",\"major\":\"专业课\",\"general\":\"通用\"}"
                    .to_string(),
            reminder_sound_source: DEFAULT_REMINDER_SOUND_SOURCE.to_string(),
            reminder_sound_id: DEFAULT_REMINDER_SOUND_ID.to_string(),
            reminder_sound_file_name: None,
            reminder_sound_updated_at: None,
            reminder_sound_volume: 100,
            reminder_quiet_hours_enabled: false,
            reminder_quiet_hours_start: "22:30".to_string(),
            reminder_quiet_hours_end: "07:00".to_string(),
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
        default_focus_minutes: get_i64_setting(
            &connection,
            DEFAULT_FOCUS_MINUTES_KEY,
            defaults.default_focus_minutes,
        )?
        .clamp(1, 120),
        break_minutes: get_i64_setting(&connection, BREAK_MINUTES_KEY, defaults.break_minutes)?
            .clamp(1, 60),
        long_break_minutes: get_i64_setting(
            &connection,
            LONG_BREAK_MINUTES_KEY,
            defaults.long_break_minutes,
        )?
        .clamp(1, 120),
        long_break_interval: get_i64_setting(
            &connection,
            LONG_BREAK_INTERVAL_KEY,
            defaults.long_break_interval,
        )?
        .clamp(1, 12),
        default_focus_mode: normalize_mode(&get_string_setting(
            &connection,
            DEFAULT_FOCUS_MODE_KEY,
            &defaults.default_focus_mode,
        )?),
        ui_theme: normalize_theme(&get_string_setting(
            &connection,
            UI_THEME_KEY,
            &defaults.ui_theme,
        )?),
        launch_at_startup: get_bool_setting(
            &connection,
            LAUNCH_AT_STARTUP_KEY,
            defaults.launch_at_startup,
        )?,
        auto_start_break_after_focus: get_bool_setting(
            &connection,
            AUTO_START_BREAK_AFTER_FOCUS_KEY,
            defaults.auto_start_break_after_focus,
        )?,
        schedule_reminder_enabled: get_bool_setting(
            &connection,
            SCHEDULE_REMINDER_ENABLED_KEY,
            defaults.schedule_reminder_enabled,
        )?,
        schedule_reminder_lead_minutes: get_i64_setting(
            &connection,
            SCHEDULE_REMINDER_LEAD_MINUTES_KEY,
            defaults.schedule_reminder_lead_minutes,
        )?
        .clamp(0, 60),
        focus_widget_enabled: get_bool_setting(
            &connection,
            FOCUS_WIDGET_ENABLED_KEY,
            defaults.focus_widget_enabled,
        )?,
        focus_widget_auto_follow: get_bool_setting(
            &connection,
            FOCUS_WIDGET_AUTO_FOLLOW_KEY,
            defaults.focus_widget_auto_follow,
        )?,
        focus_widget_remember_geometry: get_bool_setting(
            &connection,
            FOCUS_WIDGET_REMEMBER_GEOMETRY_KEY,
            defaults.focus_widget_remember_geometry,
        )?,
        focus_widget_x: get_optional_i64_setting_allow_zero(&connection, FOCUS_WIDGET_X_KEY)?
            .map(|value| value.clamp(-32768, 32768)),
        focus_widget_y: get_optional_i64_setting_allow_zero(&connection, FOCUS_WIDGET_Y_KEY)?
            .map(|value| value.clamp(-32768, 32768)),
        focus_widget_width: get_optional_i64_setting_allow_zero(
            &connection,
            FOCUS_WIDGET_WIDTH_KEY,
        )?
        .or(defaults.focus_widget_width)
        .map(|value| value.clamp(240, 420)),
        focus_widget_height: get_optional_i64_setting_allow_zero(
            &connection,
            FOCUS_WIDGET_HEIGHT_KEY,
        )?
        .or(defaults.focus_widget_height)
        .map(|value| value.clamp(112, 240)),
        sync_backend: normalize_sync_backend(&get_string_setting(
            &connection,
            SYNC_BACKEND_KEY,
            &defaults.sync_backend,
        )?),
        primary_owner_device_id: normalize_optional_device_id(&get_string_setting(
            &connection,
            PRIMARY_OWNER_DEVICE_ID_KEY,
            "",
        )?),
        primary_owner_updated_at: get_optional_i64_setting(
            &connection,
            PRIMARY_OWNER_UPDATED_AT_KEY,
        )?,
        emergency_cooldown_seconds: get_i64_setting(
            &connection,
            EMERGENCY_COOLDOWN_SECONDS_KEY,
            defaults.emergency_cooldown_seconds,
        )?
        .clamp(0, 300),
        checklist_category_names: get_string_setting(
            &connection,
            CHECKLIST_CATEGORY_NAMES_KEY,
            &defaults.checklist_category_names,
        )?,
        reminder_sound_source: normalize_reminder_sound_source(&get_string_setting(
            &connection,
            REMINDER_SOUND_SOURCE_KEY,
            &defaults.reminder_sound_source,
        )?),
        reminder_sound_id: normalize_reminder_sound_id(&get_string_setting(
            &connection,
            REMINDER_SOUND_ID_KEY,
            &defaults.reminder_sound_id,
        )?),
        reminder_sound_file_name: normalize_optional_string(&get_string_setting(
            &connection,
            REMINDER_SOUND_FILE_NAME_KEY,
            "",
        )?),
        reminder_sound_updated_at: get_optional_i64_setting(
            &connection,
            REMINDER_SOUND_UPDATED_AT_KEY,
        )?,
        reminder_sound_volume: get_i64_setting(
            &connection,
            REMINDER_SOUND_VOLUME_KEY,
            defaults.reminder_sound_volume,
        )?
        .clamp(0, 100),
        reminder_quiet_hours_enabled: get_bool_setting(
            &connection,
            REMINDER_QUIET_HOURS_ENABLED_KEY,
            defaults.reminder_quiet_hours_enabled,
        )?,
        reminder_quiet_hours_start: normalize_time_of_day(&get_string_setting(
            &connection,
            REMINDER_QUIET_HOURS_START_KEY,
            &defaults.reminder_quiet_hours_start,
        )?),
        reminder_quiet_hours_end: normalize_time_of_day(&get_string_setting(
            &connection,
            REMINDER_QUIET_HOURS_END_KEY,
            &defaults.reminder_quiet_hours_end,
        )?),
    })
}

#[tauri::command]
pub fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let normalized = AppSettings {
        default_study_minutes: settings.default_study_minutes.clamp(1, 720),
        default_focus_minutes: settings.default_focus_minutes.clamp(1, 120),
        break_minutes: settings.break_minutes.clamp(1, 60),
        long_break_minutes: settings.long_break_minutes.clamp(1, 120),
        long_break_interval: settings.long_break_interval.clamp(1, 12),
        default_focus_mode: normalize_mode(&settings.default_focus_mode),
        ui_theme: normalize_theme(&settings.ui_theme),
        launch_at_startup: settings.launch_at_startup,
        auto_start_break_after_focus: settings.auto_start_break_after_focus,
        schedule_reminder_enabled: settings.schedule_reminder_enabled,
        schedule_reminder_lead_minutes: settings.schedule_reminder_lead_minutes.clamp(0, 60),
        focus_widget_enabled: settings.focus_widget_enabled,
        focus_widget_auto_follow: settings.focus_widget_auto_follow,
        focus_widget_remember_geometry: settings.focus_widget_remember_geometry,
        focus_widget_x: settings
            .focus_widget_x
            .map(|value| value.clamp(-32768, 32768)),
        focus_widget_y: settings
            .focus_widget_y
            .map(|value| value.clamp(-32768, 32768)),
        focus_widget_width: settings
            .focus_widget_width
            .map(|value| value.clamp(240, 420)),
        focus_widget_height: settings
            .focus_widget_height
            .map(|value| value.clamp(112, 240)),
        sync_backend: normalize_sync_backend(&settings.sync_backend),
        primary_owner_device_id: settings
            .primary_owner_device_id
            .as_deref()
            .and_then(normalize_optional_device_id),
        primary_owner_updated_at: settings.primary_owner_updated_at,
        emergency_cooldown_seconds: settings.emergency_cooldown_seconds.clamp(0, 300),
        checklist_category_names: normalize_category_names(&settings.checklist_category_names)?,
        reminder_sound_source: normalize_reminder_sound_source(&settings.reminder_sound_source),
        reminder_sound_id: normalize_reminder_sound_id(&settings.reminder_sound_id),
        reminder_sound_file_name: settings
            .reminder_sound_file_name
            .as_deref()
            .and_then(normalize_optional_string),
        reminder_sound_updated_at: settings.reminder_sound_updated_at,
        reminder_sound_volume: settings.reminder_sound_volume.clamp(0, 100),
        reminder_quiet_hours_enabled: settings.reminder_quiet_hours_enabled,
        reminder_quiet_hours_start: normalize_time_of_day(&settings.reminder_quiet_hours_start),
        reminder_quiet_hours_end: normalize_time_of_day(&settings.reminder_quiet_hours_end),
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
        LONG_BREAK_MINUTES_KEY,
        &normalized.long_break_minutes.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        LONG_BREAK_INTERVAL_KEY,
        &normalized.long_break_interval.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        DEFAULT_FOCUS_MODE_KEY,
        &normalized.default_focus_mode,
        &now,
    )?;
    set_setting(&connection, UI_THEME_KEY, &normalized.ui_theme, &now)?;
    set_setting(
        &connection,
        LAUNCH_AT_STARTUP_KEY,
        if normalized.launch_at_startup {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        AUTO_START_BREAK_AFTER_FOCUS_KEY,
        if normalized.auto_start_break_after_focus {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        SCHEDULE_REMINDER_ENABLED_KEY,
        if normalized.schedule_reminder_enabled {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        SCHEDULE_REMINDER_LEAD_MINUTES_KEY,
        &normalized.schedule_reminder_lead_minutes.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_ENABLED_KEY,
        if normalized.focus_widget_enabled {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_AUTO_FOLLOW_KEY,
        if normalized.focus_widget_auto_follow {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_REMEMBER_GEOMETRY_KEY,
        if normalized.focus_widget_remember_geometry {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_X_KEY,
        &normalized
            .focus_widget_x
            .map(|value| value.to_string())
            .unwrap_or_default(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_Y_KEY,
        &normalized
            .focus_widget_y
            .map(|value| value.to_string())
            .unwrap_or_default(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_WIDTH_KEY,
        &normalized.focus_widget_width.unwrap_or(280).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_HEIGHT_KEY,
        &normalized.focus_widget_height.unwrap_or(144).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        SYNC_BACKEND_KEY,
        &normalized.sync_backend,
        &now,
    )?;
    set_setting(
        &connection,
        PRIMARY_OWNER_DEVICE_ID_KEY,
        normalized.primary_owner_device_id.as_deref().unwrap_or(""),
        &now,
    )?;
    set_setting(
        &connection,
        PRIMARY_OWNER_UPDATED_AT_KEY,
        &normalized.primary_owner_updated_at.unwrap_or(0).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        EMERGENCY_COOLDOWN_SECONDS_KEY,
        &normalized.emergency_cooldown_seconds.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        CHECKLIST_CATEGORY_NAMES_KEY,
        &normalized.checklist_category_names,
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_SOUND_SOURCE_KEY,
        &normalized.reminder_sound_source,
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_SOUND_ID_KEY,
        &normalized.reminder_sound_id,
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_SOUND_FILE_NAME_KEY,
        normalized.reminder_sound_file_name.as_deref().unwrap_or(""),
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_SOUND_UPDATED_AT_KEY,
        &normalized
            .reminder_sound_updated_at
            .unwrap_or(0)
            .to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_SOUND_VOLUME_KEY,
        &normalized.reminder_sound_volume.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_QUIET_HOURS_ENABLED_KEY,
        if normalized.reminder_quiet_hours_enabled {
            "1"
        } else {
            "0"
        },
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_QUIET_HOURS_START_KEY,
        &normalized.reminder_quiet_hours_start,
        &now,
    )?;
    set_setting(
        &connection,
        REMINDER_QUIET_HOURS_END_KEY,
        &normalized.reminder_quiet_hours_end,
        &now,
    )?;
    sync_launch_at_startup(&app, normalized.launch_at_startup)?;

    Ok(normalized)
}

pub fn save_focus_widget_geometry(
    app: &AppHandle,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
) -> Result<(), String> {
    let connection = open_database(&database_path(app)?)?;
    let now = Utc::now().to_rfc3339();

    set_setting(
        &connection,
        FOCUS_WIDGET_X_KEY,
        &x.clamp(-32768, 32768).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_Y_KEY,
        &y.clamp(-32768, 32768).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_WIDTH_KEY,
        &width.clamp(240, 420).to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        FOCUS_WIDGET_HEIGHT_KEY,
        &height.clamp(112, 240).to_string(),
        &now,
    )?;

    Ok(())
}

#[tauri::command]
pub fn save_custom_reminder_sound(
    app: AppHandle,
    file: ReminderSoundFile,
) -> Result<AppSettings, String> {
    let extension = validate_audio_file_name(&file.file_name)?;
    if file.bytes.is_empty() {
        return Err("铃声文件不能为空。".to_string());
    }
    if file.bytes.len() > 10 * 1024 * 1024 {
        return Err("铃声文件不能超过 10MB。".to_string());
    }

    let sound_dir = reminder_sound_dir(&app)?;
    fs::create_dir_all(&sound_dir).map_err(|error| error.to_string())?;
    remove_existing_custom_sounds(&sound_dir)?;
    let stored_file_name = format!("{CUSTOM_REMINDER_SOUND_FILE}.{extension}");
    fs::write(sound_dir.join(&stored_file_name), file.bytes).map_err(|error| error.to_string())?;

    let mut settings = get_app_settings(app.clone())?;
    settings.reminder_sound_source = "custom".to_string();
    settings.reminder_sound_id = DEFAULT_REMINDER_SOUND_ID.to_string();
    settings.reminder_sound_file_name = Some(stored_file_name);
    settings.reminder_sound_updated_at = Some(Utc::now().timestamp_millis());
    save_app_settings(app, settings)
}

#[tauri::command]
pub fn get_custom_reminder_sound(app: AppHandle) -> Result<Option<ReminderSoundData>, String> {
    let settings = get_app_settings(app.clone())?;
    let Some(file_name) = settings.reminder_sound_file_name else {
        return Ok(None);
    };
    let safe_file_name = safe_stored_sound_file_name(&file_name)?;
    let path = reminder_sound_dir(&app)?.join(&safe_file_name);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    Ok(Some(ReminderSoundData {
        mime_type: mime_type_for_file_name(&safe_file_name).to_string(),
        file_name: safe_file_name,
        bytes,
    }))
}

#[tauri::command]
pub fn reset_custom_reminder_sound(app: AppHandle) -> Result<AppSettings, String> {
    let sound_dir = reminder_sound_dir(&app)?;
    if sound_dir.exists() {
        remove_existing_custom_sounds(&sound_dir)?;
    }

    let mut settings = get_app_settings(app.clone())?;
    settings.reminder_sound_source = DEFAULT_REMINDER_SOUND_SOURCE.to_string();
    settings.reminder_sound_id = DEFAULT_REMINDER_SOUND_ID.to_string();
    settings.reminder_sound_file_name = None;
    settings.reminder_sound_updated_at = Some(Utc::now().timestamp_millis());
    save_app_settings(app, settings)
}

#[tauri::command]
pub fn get_sync_device_id(app: AppHandle) -> Result<String, String> {
    let connection = open_database(&database_path(&app)?)?;
    load_or_create_device_id(&connection)
}

#[tauri::command]
pub fn get_app_data_location(app: AppHandle) -> Result<AppDataLocation, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(app_data_location_from_dir(&app_data_dir))
}

#[tauri::command]
pub fn open_app_data_location(app: AppHandle) -> Result<AppDataLocation, String> {
    let location = get_app_data_location(app.clone())?;
    fs::create_dir_all(&location.app_data_dir).map_err(|error| error.to_string())?;
    open_path_in_file_explorer(&location.app_data_dir)?;
    Ok(location)
}

pub fn sync_launch_at_startup(app: &AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let _ = app;

        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;

            const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
            const RUN_VALUE_NAME: &str = "kaoyan-focus";

            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (run_key, _) = hkcu
                .create_subkey(RUN_KEY)
                .map_err(|error| error.to_string())?;

            if enabled {
                let executable = env::current_exe().map_err(|error| error.to_string())?;
                let command = format!("\"{}\"", executable.display());
                run_key
                    .set_value(RUN_VALUE_NAME, &command)
                    .map_err(|error| error.to_string())?;
            } else {
                let _ = run_key.delete_value(RUN_VALUE_NAME);
            }
        }
    }

    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
    }

    Ok(())
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn app_data_location_from_dir(app_data_dir: &Path) -> AppDataLocation {
    let database_path = app_data_dir.join("kaoyan-focus.sqlite3");

    AppDataLocation {
        app_data_dir: app_data_dir.to_string_lossy().to_string(),
        database_path: database_path.to_string_lossy().to_string(),
    }
}

fn open_path_in_file_explorer(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::{
            core::PCWSTR,
            Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        };

        let operation = wide_null("open");
        let target = wide_null(path);
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };

        let result_code = result.0 as isize;
        if result_code <= 32 {
            return Err(format!("打开数据目录失败，系统错误码：{result_code}"));
        }

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Err("当前平台暂不支持直接打开本地目录。".to_string())
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
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

fn get_bool_setting(
    connection: &rusqlite::Connection,
    key: &str,
    fallback: bool,
) -> Result<bool, String> {
    let raw = get_string_setting(connection, key, if fallback { "1" } else { "0" })?;
    let normalized = raw.trim().to_ascii_lowercase();
    Ok(match normalized.as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => fallback,
    })
}

fn get_optional_i64_setting(
    connection: &rusqlite::Connection,
    key: &str,
) -> Result<Option<i64>, String> {
    let raw = get_string_setting(connection, key, "")?;
    Ok(raw.trim().parse::<i64>().ok().filter(|value| *value > 0))
}

fn get_optional_i64_setting_allow_zero(
    connection: &rusqlite::Connection,
    key: &str,
) -> Result<Option<i64>, String> {
    let raw = get_string_setting(connection, key, "")?;
    Ok(raw.trim().parse::<i64>().ok())
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

fn normalize_theme(theme: &str) -> String {
    match theme {
        "light" | "mono" | "dawn" | "forest" | "sakura" => theme.to_string(),
        _ => "dark".to_string(),
    }
}

fn normalize_sync_backend(value: &str) -> String {
    if value == "object_storage" {
        "object_storage".to_string()
    } else {
        "webdav".to_string()
    }
}

fn normalize_reminder_sound_source(value: &str) -> String {
    if value == "custom" {
        "custom".to_string()
    } else {
        DEFAULT_REMINDER_SOUND_SOURCE.to_string()
    }
}

fn normalize_reminder_sound_id(value: &str) -> String {
    match value {
        "classic" | "bright" | "soft" | "urgent" | "short" => value.to_string(),
        _ => DEFAULT_REMINDER_SOUND_ID.to_string(),
    }
}

fn normalize_time_of_day(value: &str) -> String {
    let Some((hour, minute)) = value.trim().split_once(':') else {
        return "00:00".to_string();
    };

    let Ok(hour) = hour.parse::<u32>() else {
        return "00:00".to_string();
    };
    let Ok(minute) = minute.parse::<u32>() else {
        return "00:00".to_string();
    };

    if hour > 23 || minute > 59 {
        return "00:00".to_string();
    }

    format!("{hour:02}:{minute:02}")
}

fn normalize_optional_device_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn reminder_sound_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("sounds"))
}

fn remove_existing_custom_sounds(sound_dir: &Path) -> Result<(), String> {
    for extension in ["mp3", "wav", "ogg", "m4a"] {
        let path = sound_dir.join(format!("{CUSTOM_REMINDER_SOUND_FILE}.{extension}"));
        if path.exists() {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn validate_audio_file_name(file_name: &str) -> Result<String, String> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| "请选择 mp3、wav、ogg 或 m4a 音频文件。".to_string())?;
    match extension.as_str() {
        "mp3" | "wav" | "ogg" | "m4a" => Ok(extension),
        _ => Err("只支持 mp3、wav、ogg、m4a 音频文件。".to_string()),
    }
}

fn safe_stored_sound_file_name(file_name: &str) -> Result<String, String> {
    let path = Path::new(file_name);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "铃声文件名无效。".to_string())?;
    if name.starts_with(CUSTOM_REMINDER_SOUND_FILE) {
        Ok(name.to_string())
    } else {
        Err("铃声文件名无效。".to_string())
    }
}

fn mime_type_for_file_name(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

fn normalize_category_names(raw: &str) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(raw).unwrap_or_else(|_| {
        serde_json::json!({
            "politics": "政治",
            "english": "英语",
            "math": "数学",
            "major": "专业课",
            "general": "通用"
        })
    });

    let object = value
        .as_object_mut()
        .ok_or_else(|| "清单分类名称配置格式不正确".to_string())?;

    for (key, fallback) in [
        ("politics", "政治"),
        ("english", "英语"),
        ("math", "数学"),
        ("major", "专业课"),
        ("general", "通用"),
    ] {
        let next_value = object
            .get(key)
            .and_then(|item| item.as_str())
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .unwrap_or(fallback);
        object.insert(
            key.to_string(),
            serde_json::Value::String(next_value.to_string()),
        );
    }

    serde_json::to_string(object).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn app_data_location_uses_expected_database_file_name() {
        let app_data_dir = PathBuf::from(r"C:\Users\Lenovo\AppData\Roaming\kaoyan-focus");
        let location = app_data_location_from_dir(&app_data_dir);

        assert_eq!(
            location.app_data_dir,
            r"C:\Users\Lenovo\AppData\Roaming\kaoyan-focus"
        );
        assert_eq!(
            location.database_path,
            r"C:\Users\Lenovo\AppData\Roaming\kaoyan-focus\kaoyan-focus.sqlite3",
        );
    }
}
