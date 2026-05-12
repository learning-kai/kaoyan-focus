use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

mod commands {
    pub mod focus;
    pub mod monitor;
    pub mod settings;
    pub mod whitelist;
}
mod focus;
mod storage;
mod whitelist;
mod windows;

pub struct AppState {
    active_session_id: Mutex<Option<i64>>,
    study_mode_active: Mutex<bool>,
    last_blocked_process: Mutex<Option<(i64, String)>>,
}

impl AppState {
    fn should_prevent_exit(&self) -> Result<bool, String> {
        let has_active_session = self
            .active_session_id
            .lock()
            .map_err(|error| error.to_string())?
            .is_some();
        let study_mode_active = *self
            .study_mode_active
            .lock()
            .map_err(|error| error.to_string())?;
        Ok(has_active_session || study_mode_active)
    }
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
fn set_study_mode_active(state: tauri::State<'_, AppState>, active: bool) -> Result<(), String> {
    *state
        .study_mode_active
        .lock()
        .map_err(|error| error.to_string())? = active;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            active_session_id: Mutex::new(None),
            study_mode_active: Mutex::new(false),
            last_blocked_process: Mutex::new(None),
        })
        .setup(|app| {
            let show_item = MenuItem::with_id(app, "tray_show", "显示主界面", true, Option::<&str>::None)?;
            let quit_item = MenuItem::with_id(app, "tray_quit", "退出", true, Option::<&str>::None)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let icon = app.default_window_icon().cloned().ok_or_else(|| {
                tauri::Error::AssetNotFound("default window icon not found".to_string())
            })?;

            TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .tooltip("考研专注")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray_show" => {
                        let _ = show_main_window(app);
                    }
                    "tray_quit" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if state.should_prevent_exit().unwrap_or(false) {
                                let _ = show_main_window(app);
                                return;
                            }
                        }

                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    if state.should_prevent_exit().unwrap_or(false) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            set_study_mode_active,
            commands::focus::start_focus_session,
            commands::focus::finish_focus_session,
            commands::focus::emergency_exit_focus_session,
            commands::focus::interrupt_focus_session,
            commands::focus::recover_active_focus_session,
            commands::focus::list_focus_sessions,
            commands::focus::list_subjects,
            commands::focus::get_focus_stats_summary,
            commands::settings::get_app_settings,
            commands::settings::get_app_data_location,
            commands::settings::save_app_settings,
            commands::whitelist::create_whitelist_app,
            commands::whitelist::create_whitelist_website,
            commands::whitelist::list_recent_blocked_apps,
            commands::whitelist::list_whitelist_apps,
            commands::whitelist::list_running_processes,
            commands::whitelist::set_whitelist_app_enabled,
            commands::whitelist::delete_whitelist_app,
            commands::monitor::get_current_foreground_app,
            commands::monitor::check_focus_foreground_app,
            commands::monitor::list_interruption_summary
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main_window(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
    }

    Ok(())
}
