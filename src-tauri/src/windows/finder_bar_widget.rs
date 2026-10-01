use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::commands::{
    focus::StudyModeState,
    settings::{get_app_settings, AppSettings},
};

pub const FINDER_BAR_WIDGET_LABEL: &str = "finder-bar-widget";
const FINDER_BAR_CLASS: &str = "MyFinderApp";
const MODULE_WIDTH: i32 = 184;
const MODULE_HEIGHT: i32 = 28;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{
            FindWindowW, GetClientRect, GetWindowLongPtrW, SetParent, SetWindowLongPtrW,
            SetWindowPos, GWL_EXSTYLE, GWL_STYLE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
            WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
        },
    },
};

static CREATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderBarWidgetStatus {
    pub embedded: bool,
}

pub fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(FINDER_BAR_WIDGET_LABEL) {
        embed_window(&window)?;
        return Ok(window);
    }

    let _guard = CREATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(FINDER_BAR_WIDGET_LABEL) {
        embed_window(&window)?;
        return Ok(window);
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == FINDER_BAR_WIDGET_LABEL);
    let mut builder = if let Some(config) = config {
        WebviewWindowBuilder::from_config(app, config).map_err(|error| error.to_string())?
    } else {
        WebviewWindowBuilder::new(
            app,
            FINDER_BAR_WIDGET_LABEL,
            WebviewUrl::App(PathBuf::from("index.html?windowLabel=finder-bar-widget")),
        )
    };
    builder = builder
        .title("")
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .inner_size(MODULE_WIDTH as f64, MODULE_HEIGHT as f64);
    let window = builder.build().map_err(|error| error.to_string())?;
    embed_window(&window)?;
    Ok(window)
}

pub fn show(app: &AppHandle) -> Result<FinderBarWidgetStatus, String> {
    let window = ensure(app)?;
    embed_window(&window)?;
    window.show().map_err(|error| error.to_string())?;
    Ok(FinderBarWidgetStatus { embedded: true })
}

pub fn hide(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FINDER_BAR_WIDGET_LABEL) {
        let _ = window.hide();
    }
    Ok(())
}

pub fn apply_current_settings(app: &AppHandle) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    if settings.focus_widget_mode == "finder_bar" && settings.focus_widget_enabled {
        let active = crate::windows::focus_widget::background_study_mode_is_active(app);
        let _ = sync_visibility_with_active_status(app, &settings, active);
    } else {
        hide(app)?;
    }
    Ok(())
}

pub fn sync_visibility_with_study_mode_state(
    app: &AppHandle,
    settings: &AppSettings,
    state: &StudyModeState,
) -> Result<(), String> {
    if settings.focus_widget_mode != "finder_bar" || !settings.focus_widget_enabled {
        return hide(app);
    }
    if state.status == "active" {
        let _ = show(app)?;
    } else {
        hide(app)?;
    }
    Ok(())
}

pub fn sync_visibility_with_active_status(
    app: &AppHandle,
    settings: &AppSettings,
    active: bool,
) -> Result<(), String> {
    if settings.focus_widget_mode != "finder_bar" || !settings.focus_widget_enabled {
        return hide(app);
    }
    if active {
        let _ = show(app)?;
    } else {
        hide(app)?;
    }
    Ok(())
}

#[cfg(windows)]
fn finder_bar_hwnd() -> Option<HWND> {
    let class_name: Vec<u16> = FINDER_BAR_CLASS
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd = unsafe { FindWindowW(PCWSTR(class_name.as_ptr()), PCWSTR::null()) }.ok()?;
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

#[cfg(not(windows))]
fn finder_bar_hwnd() -> Option<()> {
    None
}

fn embed_window(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        let parent = finder_bar_hwnd()
            .ok_or_else(|| "未找到 MyDockFinder Finder Bar（MyFinderApp）".to_string())?;
        let child = window.hwnd().map_err(|error| error.to_string())?;
        unsafe {
            let style = GetWindowLongPtrW(child, GWL_STYLE);
            let next_style =
                (style & !(WS_POPUP.0 as isize)) | (WS_CHILD.0 as isize) | (WS_VISIBLE.0 as isize);
            let ex_style = GetWindowLongPtrW(child, GWL_EXSTYLE);
            let next_ex_style =
                (ex_style | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize))
                    & !(0x00040000isize);
            if style != next_style {
                SetWindowLongPtrW(child, GWL_STYLE, next_style);
            }
            if ex_style != next_ex_style {
                SetWindowLongPtrW(child, GWL_EXSTYLE, next_ex_style);
            }
            SetParent(child, Some(parent)).map_err(|error| error.to_string())?;
            let mut rect = RECT::default();
            GetClientRect(parent, &mut rect).map_err(|error| error.to_string())?;
            let x = (rect.right - MODULE_WIDTH - 8).max(0);
            SetWindowPos(
                child,
                None,
                x,
                0,
                MODULE_WIDTH,
                MODULE_HEIGHT,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Err("Finder Bar 嵌入仅支持 Windows".to_string())
    }
}
