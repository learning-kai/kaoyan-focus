use std::{sync::Mutex, thread, time::Duration};

#[cfg(windows)]
use ::windows::{
    core::PCWSTR,
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
#[cfg(windows)]
use tauri_winrt_notification::{Duration as ToastDuration, LoopableSound, Scenario, Sound, Toast};

mod commands {
    pub mod checklist;
    pub mod focus;
    pub mod monitor;
    pub mod review;
    pub mod schedule;
    pub mod settings;
    pub mod sync;
    pub mod whitelist;
}
mod focus;
mod storage;
mod sync_package;
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

#[tauri::command]
fn show_study_reminder(app: tauri::AppHandle, title: String, body: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let app_id = app.config().identifier.as_str();

        Toast::new(app_id)
            .title(&title)
            .text1(&body)
            .scenario(Scenario::Reminder)
            .duration(ToastDuration::Long)
            .sound(Some(Sound::Loop(LoopableSound::Alarm2)))
            .show()
            .or_else(|_| {
                Toast::new(Toast::POWERSHELL_APP_ID)
                    .title(&title)
                    .text1(&body)
                    .scenario(Scenario::Reminder)
                    .duration(ToastDuration::Long)
                    .sound(Some(Sound::Loop(LoopableSound::Alarm2)))
                    .show()
            })
            .map_err(|error| error.to_string())?;
    }

    #[cfg(not(windows))]
    {
        let _ = (app, title, body);
    }

    Ok(())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("只支持打开 http 或 https 网页。".to_string());
    }

    #[cfg(windows)]
    {
        let operation = wide_null("open");
        let target = wide_null(trimmed);
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
            return Err(format!("打开网页失败，系统错误码：{result_code}"));
        }
    }

    #[cfg(not(windows))]
    {
        let _ = trimmed;
        return Err("当前平台暂不支持直接打开外部网页。".to_string());
    }

    Ok(())
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            active_session_id: Mutex::new(None),
            study_mode_active: Mutex::new(false),
            last_blocked_process: Mutex::new(None),
        })
        .setup(|app| {
            let app_handle = app.handle().clone();
            let _ = commands::focus::sync_study_runtime_state(&app_handle);
            thread::spawn(move || loop {
                let _ = commands::focus::tick_background_study_mode(&app_handle);
                thread::sleep(Duration::from_secs(3));
            });

            let show_item =
                MenuItem::with_id(app, "tray_show", "显示主界面", true, Option::<&str>::None)?;
            let quit_item =
                MenuItem::with_id(app, "tray_quit", "退出", true, Option::<&str>::None)?;
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
                        let _ = window.set_fullscreen(false);
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            set_study_mode_active,
            open_external_url,
            commands::checklist::get_checklist_page_data,
            commands::checklist::create_checklist_task,
            commands::checklist::update_checklist_task,
            commands::checklist::delete_checklist_task,
            commands::checklist::reorder_checklist_tasks,
            commands::checklist::complete_checklist_task,
            commands::checklist::add_task_to_today_plan,
            commands::checklist::create_today_plan_item,
            commands::checklist::update_today_plan_item,
            commands::checklist::delete_today_plan_item,
            commands::checklist::reorder_today_plan_items,
            commands::checklist::complete_today_plan_item,
            commands::schedule::get_schedule_page_data,
            commands::schedule::create_schedule_block,
            commands::schedule::create_schedule_block_from_today_item,
            commands::schedule::update_schedule_block,
            commands::schedule::move_schedule_block,
            commands::schedule::delete_schedule_block,
            commands::schedule::create_schedule_template,
            commands::schedule::update_schedule_template,
            commands::schedule::delete_schedule_template,
            commands::schedule::start_study_mode_from_schedule_block,
            commands::review::get_daily_review_page_data,
            commands::review::save_daily_review,
            commands::review::delete_daily_review,
            commands::focus::start_study_mode,
            commands::focus::get_study_mode_state,
            commands::focus::confirm_study_break,
            commands::focus::pause_study_mode,
            commands::focus::resume_study_mode,
            commands::focus::update_study_mode_subject,
            commands::focus::emergency_exit_study_mode,
            commands::focus::reset_study_mode,
            commands::focus::start_focus_session,
            commands::focus::finish_focus_session,
            commands::focus::emergency_exit_focus_session,
            commands::focus::interrupt_focus_session,
            commands::focus::recover_active_focus_session,
            commands::focus::list_focus_sessions,
            commands::focus::delete_focus_session,
            commands::focus::update_focus_session_subject,
            commands::focus::list_subjects,
            commands::focus::get_focus_stats_summary,
            commands::settings::get_app_settings,
            commands::settings::get_app_data_location,
            commands::settings::save_app_settings,
            show_study_reminder,
            commands::sync::get_webdav_settings,
            commands::sync::save_webdav_settings,
            commands::sync::test_webdav_connection,
            commands::sync::upload_database_to_webdav,
            commands::sync::download_database_from_webdav,
            commands::sync::auto_sync_webdav_database,
            commands::sync::get_object_storage_settings,
            commands::sync::save_object_storage_settings,
            commands::sync::test_object_storage_connection,
            commands::sync::upload_database_to_object_storage,
            commands::sync::download_database_from_object_storage,
            commands::sync::auto_sync_object_storage_database,
            commands::whitelist::create_whitelist_app,
            commands::whitelist::create_whitelist_website,
            commands::whitelist::list_recent_blocked_apps,
            commands::whitelist::list_whitelist_apps,
            commands::whitelist::list_running_processes,
            commands::whitelist::set_whitelist_app_enabled,
            commands::whitelist::update_whitelist_subject,
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
