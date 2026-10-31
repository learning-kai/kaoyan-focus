use std::{sync::Mutex, time::Duration};

#[cfg(windows)]
use ::windows::{
    core::PCWSTR,
    Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
            SWP_SHOWWINDOW, SW_RESTORE, SW_SHOWNORMAL,
        },
    },
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, UserAttentionType, WebviewWindowBuilder, WindowEvent,
};
#[cfg(windows)]
use tauri_winrt_notification::{
    Duration as ToastDuration, LoopableSound, Scenario, Sound, Toast, ToastDismissalReason,
};

mod commands {
    pub mod ai_scheduler;
    pub mod alarm;
    pub mod caldav;
    pub mod checklist;
    pub mod email;
    pub mod feishu;
    pub mod focus;
    pub mod health;
    pub mod micro_break;
    pub mod monitor;
    pub mod review;
    pub mod review_ai;
    pub mod schedule;
    pub mod settings;
    pub mod sync;
    pub mod whitelist;
}
mod background_tasks;
mod credential;
mod dashboard_server;
mod focus;
mod runtime_health;
mod storage;
mod sync_package;
mod whitelist;
mod windows;

pub struct AppState {
    active_session_id: Mutex<Option<i64>>,
    study_mode_active: Mutex<bool>,
    last_blocked_process: Mutex<Option<(i64, String)>>,
}

const REMINDER_NOTIFICATION_CLOSED_EVENT: &str = "study-reminder-notification-closed";
const CRITICAL_REMINDER_TOPMOST_RESET_MS: u64 = 8_000;
const MAIN_WINDOW_LABEL: &str = "main";

/// 主窗口关闭请求一律收敛成「隐藏到托盘」。
///
/// 之前只有存在活动学习模式/专注记录时才隐藏，空闲时关闭会把主窗口销毁；
/// 而进程还被悬浮小组件窗口留在后台，于是下一次启动被单实例插件拦下、
/// 回调里又找不到已销毁的 `main` 窗口，表现就是「应用在后台跑着却再也打不开，
/// 只能彻底结束进程」。现在窗口永不销毁，托盘、单实例唤起和提醒都能把它叫回来。
fn reveal_main_window(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.show()?;
    let _ = window.unminimize();
    let _ = window.set_fullscreen(false);
    window.set_focus()?;
    Ok(())
}

fn hide_main_window_to_tray(window: &tauri::Window) {
    let _ = window.set_fullscreen(false);
    let _ = window.hide();
}

/// 主窗口被销毁（旧版本遗留状态、WebView 崩溃）后按 `tauri.conf.json` 重建。
///
/// 建窗不能在当前主线程的消息处理里同步进行：托盘点击、单实例回调（`WM_COPYDATA`）
/// 都跑在主线程，同步建窗会重入事件循环。这里统一换到后台线程，建窗请求和随后的
/// `show()` 都进同一个事件队列，顺序上窗口一定先创建好再显示。
fn recreate_main_window(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(_guard) = main_window_create_lock()
            .get_or_init(|| Mutex::new(()))
            .lock()
        else {
            return;
        };

        // 并发调用时可能已经有另一条路径抢先建好了窗口。
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = reveal_main_window(&window);
            return;
        }

        match create_main_window(&app) {
            Ok(window) => {
                if let Err(error) = reveal_main_window(&window) {
                    eprintln!("Failed to show the recreated main window: {error}");
                }
            }
            Err(error) => eprintln!("Failed to recreate the main window: {error}"),
        }
    });
    Ok(())
}

fn main_window_create_lock() -> &'static std::sync::OnceLock<Mutex<()>> {
    static CREATE_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    &CREATE_LOCK
}

fn create_main_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    WebviewWindowBuilder::from_config(app, &config)?.build()
}

impl AppState {
    fn should_prevent_exit(&self, app: Option<&tauri::AppHandle>) -> Result<bool, String> {
        let has_active_session = self
            .active_session_id
            .lock()
            .map_err(|error| error.to_string())?
            .is_some();
        let study_mode_active = *self
            .study_mode_active
            .lock()
            .map_err(|error| error.to_string())?;
        let has_active_alarm = app
            .map(commands::alarm::app_has_active_alarm)
            .transpose()?
            .unwrap_or(false);
        Ok(has_active_session || study_mode_active || has_active_alarm)
    }
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
fn set_study_mode_active(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    active: bool,
) -> Result<(), String> {
    *state
        .study_mode_active
        .lock()
        .map_err(|error| error.to_string())? = active;
    sync_focus_widget_window(&app, active);
    Ok(())
}

#[tauri::command]
fn show_study_reminder(
    app: tauri::AppHandle,
    title: String,
    body: String,
    sound_id: Option<String>,
    notification_id: Option<String>,
    wake_window: Option<bool>,
) -> Result<(), String> {
    let should_wake_window = wake_window.unwrap_or(false);
    if should_wake_window {
        wake_main_window_for_reminder(&app);
    }

    #[cfg(windows)]
    {
        let app_id = app.config().identifier.as_str();
        let sound = toast_sound_for_id(sound_id.as_deref());
        let scenario = if should_wake_window {
            Scenario::Alarm
        } else {
            Scenario::Reminder
        };

        show_toast_with_close_event(
            &app,
            app_id,
            &title,
            &body,
            sound,
            scenario,
            notification_id.clone(),
        )
        .or_else(|_| {
            show_toast_with_close_event(
                &app,
                Toast::POWERSHELL_APP_ID,
                &title,
                &body,
                sound,
                scenario,
                notification_id,
            )
        })
        .map_err(|error| error.to_string())?;
    }

    #[cfg(not(windows))]
    {
        let _ = (app, title, body, sound_id, notification_id, wake_window);
    }

    Ok(())
}

#[cfg(windows)]
fn show_toast_with_close_event(
    app: &tauri::AppHandle,
    app_id: &str,
    title: &str,
    body: &str,
    sound: Option<Sound>,
    scenario: Scenario,
    notification_id: Option<String>,
) -> Result<(), tauri_winrt_notification::Error> {
    let app_for_dismiss = app.clone();
    let notification_id_for_dismiss = notification_id.clone();
    let app_for_activate = app.clone();
    let notification_id_for_activate = notification_id;

    Toast::new(app_id)
        .title(title)
        .text1(body)
        .scenario(scenario)
        .duration(ToastDuration::Long)
        .sound(sound)
        .on_dismissed(move |reason| {
            if matches!(reason, Some(ToastDismissalReason::UserCanceled)) {
                emit_reminder_notification_closed(
                    &app_for_dismiss,
                    notification_id_for_dismiss.as_deref(),
                );
            }
            Ok(())
        })
        .on_activated(move |_| {
            emit_reminder_notification_closed(
                &app_for_activate,
                notification_id_for_activate.as_deref(),
            );
            Ok(())
        })
        .show()
}

fn wake_main_window_for_reminder(app: &tauri::AppHandle) {
    // 先确保主窗口存在（可能被旧版本销毁过），再拿它做置顶和提示。
    let _ = show_main_window(app);
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    let _ = window.set_always_on_top(true);
    let _ = window.request_user_attention(Some(UserAttentionType::Critical));

    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(hwnd);
        }
    }

    let _ = window.set_focus();
    let reset_window = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(CRITICAL_REMINDER_TOPMOST_RESET_MS)).await;
        let _ = reset_window.set_always_on_top(false);
    });
}

pub(crate) fn sync_focus_widget_window(app: &tauri::AppHandle, _should_show: bool) {
    let _ = windows::focus_widget::sync_visibility_with_background_study_mode(app);
}

#[cfg(windows)]
fn emit_reminder_notification_closed(app: &tauri::AppHandle, notification_id: Option<&str>) {
    if let Some(notification_id) = notification_id {
        let _ = app.emit(
            REMINDER_NOTIFICATION_CLOSED_EVENT,
            serde_json::json!({ "notification_id": notification_id }),
        );
    }
}

#[cfg(windows)]
fn toast_sound_for_id(sound_id: Option<&str>) -> Option<Sound> {
    match sound_id.unwrap_or("classic") {
        "bright" => Some(Sound::Single(LoopableSound::Alarm5)),
        "soft" => Some(Sound::Reminder),
        "urgent" => Some(Sound::Loop(LoopableSound::Alarm10)),
        "short" => Some(Sound::SMS),
        "silent" => None,
        _ => Some(Sound::Loop(LoopableSound::Alarm2)),
    }
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

#[tauri::command]
fn open_study_dashboard(
    app: tauri::AppHandle,
) -> Result<dashboard_server::DashboardLaunch, String> {
    let launch = dashboard_server::ensure_running(app)?;
    open_external_url(launch.url.clone())?;
    Ok(launch)
}

#[tauri::command]
fn focus_widget_return_to_main(app: tauri::AppHandle) -> Result<(), String> {
    windows::focus_widget::bring_to_main(&app)
}

#[tauri::command]
fn hide_focus_widget(app: tauri::AppHandle) -> Result<(), String> {
    windows::focus_widget::hide(&app)
}

#[tauri::command]
fn focus_widget_get_dock_state(
    app: tauri::AppHandle,
) -> windows::focus_widget::FocusWidgetDockState {
    windows::focus_widget::get_dock_state(&app)
}

#[tauri::command]
fn focus_widget_get_always_on_top(app: tauri::AppHandle) -> Result<bool, String> {
    windows::focus_widget::get_always_on_top(&app)
}

#[tauri::command]
fn focus_widget_toggle_always_on_top(app: tauri::AppHandle) -> Result<bool, String> {
    windows::focus_widget::toggle_always_on_top(&app)
}

#[tauri::command]
fn focus_widget_peek_from_edge(
    app: tauri::AppHandle,
) -> Result<windows::focus_widget::FocusWidgetDockState, String> {
    windows::focus_widget::peek_from_edge(&app)
}

#[tauri::command]
fn focus_widget_collapse_to_edge(
    app: tauri::AppHandle,
) -> Result<windows::focus_widget::FocusWidgetDockState, String> {
    windows::focus_widget::collapse_to_edge(&app)
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Err(error) = show_main_window(app) {
                eprintln!("Failed to activate the existing main window: {error}");
            }
        }))
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
            if let Ok(settings) = commands::settings::get_app_settings(app_handle.clone()) {
                if let Err(error) = commands::settings::sync_launch_at_startup(
                    &app_handle,
                    settings.launch_at_startup,
                ) {
                    eprintln!("Failed to sync autostart setting: {error}");
                }
            }
            let _ = windows::focus_widget::restore_on_app_startup(&app_handle);
            background_tasks::start(&app_handle);

            let show_item =
                MenuItem::with_id(app, "tray_show", "显示主界面", true, Option::<&str>::None)?;
            let toggle_widget_item = MenuItem::with_id(
                app,
                "tray_toggle_focus_widget",
                "打开/关闭小悬浮窗",
                true,
                Option::<&str>::None,
            )?;
            let quit_item =
                MenuItem::with_id(app, "tray_quit", "退出", true, Option::<&str>::None)?;
            let menu = Menu::with_items(app, &[&show_item, &toggle_widget_item, &quit_item])?;
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
                    "tray_toggle_focus_widget" => {
                        if let Err(error) = windows::focus_widget::toggle_visibility(app) {
                            eprintln!("Failed to toggle focus widget from tray: {error}");
                        }
                    }
                    "tray_quit" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if state.should_prevent_exit(Some(app)).unwrap_or(false) {
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
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_)
                if window.label() == windows::focus_widget::FOCUS_WIDGET_LABEL =>
            {
                let _ = windows::focus_widget::refresh_geometry(window.app_handle());
            }
            WindowEvent::CloseRequested { api, .. }
                if window.label() == windows::focus_widget::FOCUS_WIDGET_LABEL =>
            {
                api.prevent_close();
                let _ = windows::focus_widget::hide(window.app_handle());
            }
            WindowEvent::CloseRequested { api, .. } => {
                // 一律拦截关闭：主窗口只隐藏到托盘，绝不销毁。
                api.prevent_close();
                hide_main_window_to_tray(window);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            set_study_mode_active,
            open_external_url,
            open_study_dashboard,
            focus_widget_return_to_main,
            hide_focus_widget,
            focus_widget_get_dock_state,
            focus_widget_get_always_on_top,
            focus_widget_toggle_always_on_top,
            focus_widget_peek_from_edge,
            focus_widget_collapse_to_edge,
            commands::alarm::list_alarms,
            commands::alarm::create_alarm,
            commands::alarm::update_alarm,
            commands::alarm::delete_alarm,
            commands::alarm::set_alarm_enabled,
            commands::alarm::dismiss_alarm,
            commands::alarm::trigger_due_alarms,
            commands::alarm::get_next_alarm,
            commands::alarm::has_active_alarm,
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
            commands::ai_scheduler::settings::get_ai_scheduler_settings,
            commands::ai_scheduler::settings::save_ai_scheduler_settings,
            commands::ai_scheduler::settings::test_ai_scheduler_connection,
            commands::ai_scheduler::apply::preview_ai_schedule,
            commands::ai_scheduler::apply::apply_ai_plan_proposal,
            commands::ai_scheduler::apply::discard_ai_plan_proposal,
            commands::ai_scheduler::apply::get_latest_ai_plan_proposal,
            commands::ai_scheduler::apply::update_ai_plan_proposal_items,
            commands::ai_scheduler::apply::regenerate_ai_plan_proposal,
            commands::schedule::get_schedule_page_data,
            commands::schedule::create_schedule_block,
            commands::schedule::create_schedule_block_from_today_item,
            commands::schedule::update_schedule_block,
            commands::schedule::move_schedule_block,
            commands::schedule::set_schedule_block_completed,
            commands::schedule::delete_schedule_block,
            commands::schedule::create_schedule_template,
            commands::schedule::update_schedule_template,
            commands::schedule::delete_schedule_template,
            commands::schedule::start_study_mode_from_schedule_block,
            commands::caldav::get_caldav_settings,
            commands::caldav::save_caldav_settings,
            commands::caldav::discover_caldav_calendars,
            commands::caldav::test_caldav_connection,
            commands::caldav::sync_caldav_calendar,
            commands::review::get_daily_review_page_data,
            commands::review::save_daily_review,
            commands::review::delete_daily_review,
            commands::review::get_weekly_review_page_data,
            commands::review::save_weekly_review,
            commands::review::delete_weekly_review,
            commands::review_ai::draft_review_with_ai,
            commands::focus::start_study_mode,
            commands::focus::start_countup_study_mode,
            commands::focus::get_study_mode_state,
            commands::focus::confirm_study_break,
            commands::focus::take_manual_break,
            commands::focus::switch_study_timer_kind,
            commands::focus::skip_study_break,
            commands::focus::pause_study_mode,
            commands::focus::resume_study_mode,
            commands::focus::update_study_mode_subject,
            commands::focus::update_study_mode_whitelist,
            commands::focus::emergency_exit_study_mode,
            commands::focus::reset_study_mode,
            commands::focus::start_focus_session,
            commands::focus::finish_focus_session,
            commands::focus::emergency_exit_focus_session,
            commands::focus::interrupt_focus_session,
            commands::focus::recover_active_focus_session,
            commands::focus::list_focus_sessions,
            commands::focus::list_focus_sessions_in_range,
            commands::focus::delete_focus_session,
            commands::focus::update_focus_session_subject,
            commands::focus::list_subjects,
            commands::focus::get_focus_stats_summary,
            commands::micro_break::get_micro_break_settings,
            commands::micro_break::save_micro_break_settings,
            commands::settings::get_app_settings,
            commands::settings::get_app_data_location,
            commands::settings::open_app_data_location,
            commands::settings::get_sync_device_id,
            commands::settings::save_app_settings,
            commands::settings::save_custom_reminder_sound,
            commands::settings::get_custom_reminder_sound,
            commands::settings::reset_custom_reminder_sound,
            commands::health::get_runtime_health,
            commands::email::get_email_reminder_settings,
            commands::email::save_email_reminder_settings,
            commands::email::test_email_reminder,
            commands::email::check_due_task_email_reminders,
            commands::feishu::get_feishu_sync_settings,
            commands::feishu::save_feishu_sync_settings,
            commands::feishu::get_feishu_sync_status,
            commands::feishu::start_feishu_oauth_login,
            commands::feishu::poll_feishu_oauth_login,
            commands::feishu::logout_feishu,
            commands::feishu::sync_feishu_bridge,
            commands::feishu::rebuild_feishu_tasklists_from_local,
            commands::feishu::list_feishu_sync_runs,
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
            commands::sync::sync_object_storage_state_change,
            commands::sync::list_sync_runs,
            commands::sync::list_sync_backups,
            commands::sync::preview_sync_backup,
            commands::sync::restore_sync_backup,
            commands::whitelist::create_whitelist_app,
            commands::whitelist::create_whitelist_website,
            commands::whitelist::create_potplayer_video_whitelist_file,
            commands::whitelist::create_potplayer_video_whitelist_directory,
            commands::whitelist::get_current_potplayer_media,
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

pub(crate) fn show_main_window(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    match app.get_webview_window(MAIN_WINDOW_LABEL) {
        Some(window) => reveal_main_window(&window),
        None => recreate_main_window(app),
    }
}
