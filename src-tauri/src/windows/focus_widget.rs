use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ,
        RGN_ERROR, RGN_OR,
    },
    UI::WindowsAndMessaging::{
        CallWindowProcW, GetClientRect, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos,
        GWL_EXSTYLE, GWL_STYLE, GWL_WNDPROC, MA_NOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WM_MOUSEACTIVATE, WM_NCACTIVATE, WM_NCCALCSIZE,
        WM_NCPAINT, WNDPROC, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_NOACTIVATE,
        WS_EX_STATICEDGE, WS_EX_TOOLWINDOW, WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
        WS_SYSMENU, WS_THICKFRAME,
    },
};

use crate::{
    commands::{
        focus::StudyModeState,
        settings::{get_app_settings, save_app_settings, save_focus_widget_geometry, AppSettings},
    },
    storage::db::open_database,
};

pub const FOCUS_WIDGET_LABEL: &str = "focus-widget";

const MAIN_WINDOW_LABEL: &str = "main";
const FOCUS_WIDGET_TITLE: &str = "";
const FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT: &str = "focus-widget-dock-state-changed";
const FOCUS_WIDGET_STUDY_STATE_EVENT: &str = "focus-widget-study-state";
const FOCUS_WIDGET_STUDY_STATE_CHANGED_EVENT: &str = "focus-widget-study-state-changed";
const FOCUS_WIDGET_INITIALIZATION_SCRIPT: &str = r#"
  try {
    const url = new URL(window.location.href);
    if (url.searchParams.get('windowLabel') !== 'focus-widget') {
      url.searchParams.set('windowLabel', 'focus-widget');
      window.history.replaceState(null, '', url.toString());
    }
  } catch (_) {}
"#;

const DEFAULT_WIDTH: i64 = 280;
const DEFAULT_HEIGHT: i64 = 172;
const MIN_NORMAL_WIDTH: i64 = 240;
const MAX_NORMAL_WIDTH: i64 = 420;
const MIN_NORMAL_HEIGHT: i64 = 172;
const MAX_NORMAL_HEIGHT: i64 = 240;
const MIN_WINDOW_WIDTH: i64 = 36;
const MIN_WINDOW_HEIGHT: i64 = 36;
const COLLAPSED_SIDE_WIDTH: f64 = 36.0;
const COLLAPSED_SIDE_HEIGHT: f64 = 112.0;
const COLLAPSED_BAR_WIDTH: f64 = 172.0;
const COLLAPSED_BAR_HEIGHT: f64 = 36.0;
const EDGE_COLLAPSE_THRESHOLD: f64 = 18.0;
const EDGE_SAFE_MARGIN: f64 = 8.0;
const GEOMETRY_DEBOUNCE_MS: u64 = 250;
const DOCK_EXPAND_ANIMATION_MS: u64 = 260;
const DOCK_COLLAPSE_ANIMATION_MS: u64 = 260;
const DOCK_ANIMATION_FRAME_MS: u64 = 16;
const FLOATING_WINDOW_RADIUS: i32 = 32;
const COLLAPSED_WINDOW_RADIUS: i32 = 22;
const PEEK_MOUSE_POLL_MS: u64 = 90;
const PEEK_MOUSE_EXIT_DELAY_MS: u64 = 240;
const PEEK_MOUSE_EXIT_MARGIN: f64 = 10.0;

static CREATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_STATE: OnceLock<Mutex<FocusWidgetRuntimeState>> = OnceLock::new();
#[cfg(windows)]
static FOCUS_WIDGET_ORIGINAL_WNDPROC: OnceLock<usize> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct FocusWidget {
    app: AppHandle,
}

#[derive(Debug, Clone, Copy)]
struct FocusWidgetGeometry {
    x: Option<f64>,
    y: Option<f64>,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyModeVisibilitySnapshot {
    study_mode_id: Option<i64>,
    phase: String,
    status: String,
    is_paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FocusWidgetDockMode {
    Floating,
    Collapsed,
    Peek,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FocusWidgetDockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusWidgetDockState {
    mode: FocusWidgetDockMode,
    edge: Option<FocusWidgetDockEdge>,
}

#[derive(Debug, Default)]
struct FocusWidgetRuntimeState {
    handlers_attached: bool,
    current_study_mode_id: Option<i64>,
    manual_hidden_for_study_mode_id: Option<i64>,
    auto_visible_study_mode_id: Option<i64>,
    dock_state: FocusWidgetDockState,
    normal_geometry: Option<FocusWidgetGeometry>,
    geometry_event_generation: u64,
    geometry_suppression_generation: u64,
    window_animation_generation: u64,
    peek_mouse_watch_generation: u64,
    suppress_geometry_events: bool,
}

#[derive(Debug, Clone, Copy)]
struct LogicalWorkArea {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
struct WidgetSize {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
enum DockAnimationKind {
    Expand,
    Collapse,
}

#[derive(Debug, Clone, Copy)]
struct PhysicalWindowBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Default for FocusWidgetDockState {
    fn default() -> Self {
        Self::floating()
    }
}

impl FocusWidgetDockState {
    fn floating() -> Self {
        Self {
            mode: FocusWidgetDockMode::Floating,
            edge: None,
        }
    }

    fn collapsed(edge: FocusWidgetDockEdge) -> Self {
        Self {
            mode: FocusWidgetDockMode::Collapsed,
            edge: Some(edge),
        }
    }

    fn peek(edge: FocusWidgetDockEdge) -> Self {
        Self {
            mode: FocusWidgetDockMode::Peek,
            edge: Some(edge),
        }
    }

    fn is_floating(self) -> bool {
        self.mode == FocusWidgetDockMode::Floating
    }
}

impl FocusWidget {
    pub fn new(app: &AppHandle) -> Self {
        Self { app: app.clone() }
    }

    pub fn window(&self) -> Option<WebviewWindow> {
        self.app.get_webview_window(FOCUS_WIDGET_LABEL)
    }

    pub fn ensure(&self) -> Result<WebviewWindow, String> {
        let settings = get_app_settings(self.app.clone())?;

        if let Some(window) = self.window() {
            configure_focus_widget_window(&window, &settings)?;
            attach_window_handlers(&self.app, &window);
            return Ok(window);
        }

        let _guard = CREATE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|error| error.to_string())?;

        if let Some(window) = self.window() {
            configure_focus_widget_window(&window, &settings)?;
            attach_window_handlers(&self.app, &window);
            return Ok(window);
        }

        let geometry = focus_widget_geometry_from_settings(&settings);
        let window =
            build_focus_widget_window(&self.app, geometry, settings.focus_widget_always_on_top)?;
        attach_window_handlers(&self.app, &window);
        Ok(window)
    }

    pub fn hide(&self) -> Result<(), String> {
        mark_manual_hidden_for_current_study_mode(&self.app);
        self.hide_without_manual_suppression()
    }

    pub fn show(&self) -> Result<(), String> {
        clear_manual_hidden_for_current_study_mode(&self.app);
        let settings = get_app_settings(self.app.clone())?;
        let window = self.ensure()?;
        show_window_without_focus(&window, settings.focus_widget_always_on_top)?;
        emit_focus_widget_dock_state(&self.app);
        schedule_edge_collapse_check(&self.app);
        Ok(())
    }

    pub fn toggle_visibility(&self) -> Result<bool, String> {
        let is_visible = self
            .window()
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);

        if is_visible {
            self.hide()?;
            Ok(false)
        } else {
            self.show()?;
            Ok(true)
        }
    }

    pub fn always_on_top(&self) -> Result<bool, String> {
        Ok(get_app_settings(self.app.clone())?.focus_widget_always_on_top)
    }

    pub fn toggle_always_on_top(&self) -> Result<bool, String> {
        let mut settings = get_app_settings(self.app.clone())?;
        settings.focus_widget_always_on_top = !settings.focus_widget_always_on_top;
        let settings = save_app_settings(self.app.clone(), settings)?;

        if let Some(window) = self.window() {
            window
                .set_always_on_top(settings.focus_widget_always_on_top)
                .map_err(|error| error.to_string())?;
        }

        Ok(settings.focus_widget_always_on_top)
    }

    pub fn bring_to_main(&self) -> Result<(), String> {
        mark_manual_hidden_for_current_study_mode(&self.app);

        if let Some(window) = self.window() {
            let _ = persist_geometry_from_window(&self.app, &window);
            let _ = window.hide();
        }

        let main_window = self
            .app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "Main window not found".to_string())?;
        main_window.show().map_err(|error| error.to_string())?;
        let _ = main_window.unminimize();
        let _ = main_window.set_fullscreen(false);
        main_window.set_focus().map_err(|error| error.to_string())
    }

    pub fn refresh_geometry(&self) -> Result<(), String> {
        let Some(window) = self.window() else {
            return Ok(());
        };

        handle_focus_widget_geometry_event(&self.app, &window)
    }

    pub fn peek_from_edge(&self) -> Result<FocusWidgetDockState, String> {
        let window = self.ensure()?;
        peek_window_from_edge(&self.app, &window)
    }

    pub fn collapse_to_edge(&self) -> Result<FocusWidgetDockState, String> {
        let window = self.ensure()?;
        collapse_window_from_current_state(&self.app, &window)
    }

    fn hide_without_manual_suppression(&self) -> Result<(), String> {
        if let Some(window) = self.window() {
            persist_geometry_from_window(&self.app, &window)?;
            window.hide().map_err(|error| error.to_string())?;
        }

        Ok(())
    }
}

pub fn focus_widget(app: &AppHandle) -> FocusWidget {
    FocusWidget::new(app)
}

pub fn hide(app: &AppHandle) -> Result<(), String> {
    focus_widget(app).hide()
}

pub fn toggle_visibility(app: &AppHandle) -> Result<bool, String> {
    focus_widget(app).toggle_visibility()
}

pub fn get_always_on_top(app: &AppHandle) -> Result<bool, String> {
    focus_widget(app).always_on_top()
}

pub fn toggle_always_on_top(app: &AppHandle) -> Result<bool, String> {
    focus_widget(app).toggle_always_on_top()
}

pub fn bring_to_main(app: &AppHandle) -> Result<(), String> {
    focus_widget(app).bring_to_main()
}

pub fn refresh_geometry(app: &AppHandle) -> Result<(), String> {
    focus_widget(app).refresh_geometry()
}

pub fn get_dock_state(_app: &AppHandle) -> FocusWidgetDockState {
    current_dock_state()
}

pub fn peek_from_edge(app: &AppHandle) -> Result<FocusWidgetDockState, String> {
    focus_widget(app).peek_from_edge()
}

pub fn collapse_to_edge(app: &AppHandle) -> Result<FocusWidgetDockState, String> {
    focus_widget(app).collapse_to_edge()
}

pub fn apply_current_settings(app: &AppHandle) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    if let Some(window) = focus_widget(app).window() {
        configure_focus_widget_window(&window, &settings)?;
    }
    Ok(())
}

pub fn restore_on_app_startup(app: &AppHandle) -> Result<(), String> {
    let _ = crate::commands::focus::sync_study_runtime_state(app);
    sync_visibility_with_background_study_mode(app)
}

pub fn sync_visibility_with_background_study_mode(app: &AppHandle) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    let snapshot = load_background_study_mode_visibility(app)?;
    apply_study_mode_visibility(app, &settings, &snapshot)?;
    emit_focus_widget_study_state(app, &snapshot);
    Ok(())
}

pub fn sync_visibility_with_study_mode_state(
    app: &AppHandle,
    study_state: &StudyModeState,
) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    let snapshot = StudyModeVisibilitySnapshot::from(study_state);
    apply_study_mode_visibility(app, &settings, &snapshot)?;
    emit_focus_widget_study_state(app, study_state);
    Ok(())
}

fn build_focus_widget_window(
    app: &AppHandle,
    geometry: FocusWidgetGeometry,
    always_on_top: bool,
) -> Result<WebviewWindow, String> {
    let configured_window = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == FOCUS_WIDGET_LABEL);

    let mut builder = if let Some(config) = configured_window {
        WebviewWindowBuilder::from_config(app, config).map_err(|error| error.to_string())?
    } else {
        WebviewWindowBuilder::new(
            app,
            FOCUS_WIDGET_LABEL,
            WebviewUrl::App(PathBuf::from("index.html")),
        )
    };

    builder = builder
        .title(FOCUS_WIDGET_TITLE)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(always_on_top)
        .shadow(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .prevent_overflow()
        .min_inner_size(MIN_WINDOW_WIDTH as f64, MIN_WINDOW_HEIGHT as f64)
        .max_inner_size(MAX_NORMAL_WIDTH as f64, MAX_NORMAL_HEIGHT as f64)
        .inner_size(geometry.width, geometry.height)
        .initialization_script(FOCUS_WIDGET_INITIALIZATION_SCRIPT);

    if let (Some(x), Some(y)) = (geometry.x, geometry.y) {
        builder = builder.position(x, y);
    } else {
        builder = builder.center();
    }

    let window = builder.build().map_err(|error| error.to_string())?;
    install_focus_widget_chrome_guard(&window);
    apply_focus_widget_window_shape(&window);
    Ok(window)
}

fn configure_focus_widget_window(
    window: &WebviewWindow,
    settings: &AppSettings,
) -> Result<(), String> {
    let geometry = focus_widget_geometry_from_settings(settings);
    let dock_state = current_dock_state();
    let _ = window.set_title(FOCUS_WIDGET_TITLE);
    let _ = window.set_decorations(false);
    let _ = window.set_resizable(false);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(settings.focus_widget_always_on_top);
    let _ = window.set_focusable(false);
    let _ = window.set_shadow(false);
    install_focus_widget_chrome_guard(window);
    apply_size_constraints(window, dock_state.mode == FocusWidgetDockMode::Collapsed)?;

    if let Some(edge) = dock_state
        .edge
        .filter(|_| dock_state.mode == FocusWidgetDockMode::Collapsed)
    {
        let area = current_work_area(window)?;
        let size = collapsed_size(edge);
        let current_geometry = logical_geometry_from_window(window)?;
        let position = docked_position(edge, &area, current_geometry, size);
        set_focus_widget_geometry_frame(
            window,
            position.x.unwrap_or(area.x),
            position.y.unwrap_or(area.y),
            size.width,
            size.height,
        )?;
    } else if settings.focus_widget_remember_geometry && dock_state.is_floating() {
        window
            .set_size(LogicalSize::new(geometry.width, geometry.height))
            .map_err(|error| error.to_string())?;
        if let (Some(x), Some(y)) = (geometry.x, geometry.y) {
            let _ = window.set_position(LogicalPosition::new(x, y));
        }
    }

    apply_focus_widget_window_shape(window);
    Ok(())
}

fn attach_window_handlers(app: &AppHandle, window: &WebviewWindow) {
    let mut runtime = match runtime_state().lock() {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    if runtime.handlers_attached {
        return;
    }
    runtime.handlers_attached = true;
    drop(runtime);

    let app_for_events = app.clone();
    let window_for_events = window.clone();

    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_)
        | WindowEvent::Resized(_)
        | WindowEvent::ScaleFactorChanged { .. } => {
            apply_focus_widget_window_shape(&window_for_events);
            let _ = handle_focus_widget_geometry_event(&app_for_events, &window_for_events);
        }
        WindowEvent::CloseRequested { api, .. } => {
            mark_manual_hidden_for_current_study_mode(&app_for_events);
            let _ = persist_geometry_from_window(&app_for_events, &window_for_events);
            api.prevent_close();
            let _ = window_for_events.hide();
        }
        WindowEvent::Destroyed => {
            if let Ok(mut runtime) = runtime_state().lock() {
                runtime.handlers_attached = false;
                runtime.auto_visible_study_mode_id = None;
            }
        }
        _ => {}
    });
}

fn apply_study_mode_visibility(
    app: &AppHandle,
    settings: &AppSettings,
    snapshot: &StudyModeVisibilitySnapshot,
) -> Result<(), String> {
    note_current_study_mode(snapshot.study_mode_id);

    if !settings.focus_widget_enabled {
        mark_lifecycle_hidden(snapshot.study_mode_id);
        return focus_widget(app).hide_without_manual_suppression();
    }

    if let Some(study_mode_id) = snapshot.study_mode_id {
        remember_external_hide_if_needed(app, study_mode_id);
        if is_manually_hidden_for_study_mode(study_mode_id) {
            return Ok(());
        }
    }

    if settings.focus_widget_auto_follow && snapshot.should_auto_show_widget() {
        let window = focus_widget(app).ensure()?;
        show_window_without_focus(&window, settings.focus_widget_always_on_top)?;
        emit_focus_widget_dock_state(app);
        schedule_edge_collapse_check(app);
        if let Some(study_mode_id) = snapshot.study_mode_id {
            mark_auto_visible(study_mode_id);
        }
    } else {
        mark_lifecycle_hidden(snapshot.study_mode_id);
    }

    Ok(())
}

fn show_window_without_focus(window: &WebviewWindow, always_on_top: bool) -> Result<(), String> {
    let _ = window.set_focusable(false);
    window.show().map_err(|error| error.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_always_on_top(always_on_top);
    Ok(())
}

fn persist_geometry_from_window(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    if !current_dock_state().is_floating() {
        return Ok(());
    }

    let geometry = logical_geometry_from_window(window)?;
    remember_normal_geometry(geometry);
    persist_normal_geometry(app, geometry)
}

fn persist_normal_geometry(app: &AppHandle, geometry: FocusWidgetGeometry) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    if !settings.focus_widget_remember_geometry {
        return Ok(());
    }

    let normalized = normalize_normal_geometry(geometry);

    save_focus_widget_geometry(
        app,
        round_to_i64(normalized.x.unwrap_or(0.0)),
        round_to_i64(normalized.y.unwrap_or(0.0)),
        round_to_i64(normalized.width),
        round_to_i64(normalized.height),
    )
}

fn focus_widget_geometry_from_settings(settings: &AppSettings) -> FocusWidgetGeometry {
    let width = settings
        .focus_widget_width
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_NORMAL_WIDTH, MAX_NORMAL_WIDTH) as f64;
    let height = settings
        .focus_widget_height
        .unwrap_or(DEFAULT_HEIGHT)
        .clamp(MIN_NORMAL_HEIGHT, MAX_NORMAL_HEIGHT) as f64;

    let position = if settings.focus_widget_remember_geometry {
        settings
            .focus_widget_x
            .zip(settings.focus_widget_y)
            .map(|(x, y)| (x as f64, y as f64))
    } else {
        None
    };

    FocusWidgetGeometry {
        x: position.map(|(x, _)| x),
        y: position.map(|(_, y)| y),
        width,
        height,
    }
}

fn handle_focus_widget_geometry_event(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<(), String> {
    if should_suppress_geometry_events() {
        return Ok(());
    }

    match current_dock_state().mode {
        FocusWidgetDockMode::Floating => {
            persist_geometry_from_window(app, window)?;
            schedule_edge_collapse_check(app);
        }
        FocusWidgetDockMode::Peek => {
            if nearest_dock_edge(window)?.is_none() {
                let geometry = logical_geometry_from_window(window)?;
                remember_normal_geometry(geometry);
                set_dock_state(app, FocusWidgetDockState::floating());
                persist_normal_geometry(app, geometry)?;
                apply_size_constraints(window, false)?;
                apply_focus_widget_window_shape(window);
            }
        }
        FocusWidgetDockMode::Collapsed => {}
    }

    Ok(())
}

fn schedule_edge_collapse_check(app: &AppHandle) {
    let generation = match runtime_state().lock() {
        Ok(mut runtime) => {
            runtime.geometry_event_generation = runtime.geometry_event_generation.wrapping_add(1);
            runtime.geometry_event_generation
        }
        Err(_) => return,
    };
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(GEOMETRY_DEBOUNCE_MS)).await;

        let should_check = runtime_state()
            .lock()
            .map(|runtime| {
                runtime.geometry_event_generation == generation
                    && runtime.dock_state.mode == FocusWidgetDockMode::Floating
            })
            .unwrap_or(false);

        if !should_check {
            return;
        }

        let Some(window) = app.get_webview_window(FOCUS_WIDGET_LABEL) else {
            return;
        };
        let _ = collapse_if_near_edge(&app, &window);
    });
}

fn collapse_if_near_edge(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    if !current_dock_state().is_floating() {
        return Ok(());
    }

    let Ok(true) = window.is_visible() else {
        return Ok(());
    };

    if let Some(edge) = nearest_dock_edge(window)? {
        collapse_window_to_edge(app, window, edge)?;
    }

    Ok(())
}

fn peek_window_from_edge(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<FocusWidgetDockState, String> {
    let state = current_dock_state();
    let Some(edge) = state.edge else {
        return Ok(state);
    };

    if state.mode == FocusWidgetDockMode::Floating {
        return Ok(state);
    }

    expand_window_to_edge(app, window, edge, FocusWidgetDockMode::Peek)
}

fn collapse_window_from_current_state(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<FocusWidgetDockState, String> {
    let state = current_dock_state();
    match (state.mode, state.edge) {
        (FocusWidgetDockMode::Collapsed, Some(_)) => Ok(state),
        (FocusWidgetDockMode::Peek, Some(edge)) => {
            if nearest_dock_edge(window)?.is_some() {
                collapse_window_to_edge(app, window, edge)
            } else {
                let geometry = logical_geometry_from_window(window)?;
                remember_normal_geometry(geometry);
                set_dock_state(app, FocusWidgetDockState::floating());
                persist_normal_geometry(app, geometry)?;
                apply_size_constraints(window, false)?;
                apply_focus_widget_window_shape(window);
                Ok(FocusWidgetDockState::floating())
            }
        }
        (FocusWidgetDockMode::Floating, _) => {
            if let Some(edge) = nearest_dock_edge(window)? {
                collapse_window_to_edge(app, window, edge)
            } else {
                Ok(state)
            }
        }
        (_, None) => Ok(FocusWidgetDockState::floating()),
    }
}

fn collapse_window_to_edge(
    app: &AppHandle,
    window: &WebviewWindow,
    edge: FocusWidgetDockEdge,
) -> Result<FocusWidgetDockState, String> {
    let current_geometry = logical_geometry_from_window(window)?;
    let normal_geometry = if current_dock_state().mode == FocusWidgetDockMode::Floating {
        normalize_normal_geometry(current_geometry)
    } else {
        runtime_normal_geometry().unwrap_or_else(|| normalize_normal_geometry(current_geometry))
    };
    remember_normal_geometry(normal_geometry);
    persist_normal_geometry(app, normal_geometry)?;

    let area = current_work_area(window)?;
    let size = collapsed_size(edge);
    let position = docked_position(edge, &area, current_geometry, size);
    let next_state = FocusWidgetDockState::collapsed(edge);
    let target_geometry = FocusWidgetGeometry {
        x: Some(position.x.unwrap_or(area.x)),
        y: Some(position.y.unwrap_or(area.y)),
        width: size.width,
        height: size.height,
    };

    suppress_geometry_events_briefly();
    apply_size_constraints(window, true)?;
    store_dock_state(next_state);
    if animate_focus_widget_geometry(
        window,
        current_geometry,
        target_geometry,
        DockAnimationKind::Collapse,
    )? {
        apply_focus_widget_window_shape(window);
        emit_dock_state_change(app, next_state);
        Ok(next_state)
    } else {
        set_focus_widget_geometry_frame(
            window,
            target_geometry.x.unwrap_or(area.x),
            target_geometry.y.unwrap_or(area.y),
            target_geometry.width,
            target_geometry.height,
        )?;
        apply_focus_widget_window_shape(window);
        emit_dock_state_change(app, next_state);
        Ok(current_dock_state())
    }
}

fn expand_window_to_edge(
    app: &AppHandle,
    window: &WebviewWindow,
    edge: FocusWidgetDockEdge,
    mode: FocusWidgetDockMode,
) -> Result<FocusWidgetDockState, String> {
    let current_geometry = logical_geometry_from_window(window)?;
    let normal_geometry = runtime_normal_geometry().unwrap_or_else(|| {
        normalize_normal_geometry(focus_widget_geometry_from_settings(
            &get_app_settings(app.clone()).unwrap_or_default(),
        ))
    });
    let area = current_work_area(window)?;
    let expanded_geometry = expanded_position(edge, &area, normal_geometry, current_geometry);
    let next_state = match mode {
        FocusWidgetDockMode::Floating => FocusWidgetDockState::floating(),
        FocusWidgetDockMode::Collapsed => FocusWidgetDockState::collapsed(edge),
        FocusWidgetDockMode::Peek => FocusWidgetDockState::peek(edge),
    };

    set_dock_state(app, next_state);
    suppress_geometry_events_briefly();
    apply_size_constraints(window, false)?;
    apply_focus_widget_window_shape(window);
    if animate_focus_widget_geometry(
        window,
        current_geometry,
        expanded_geometry,
        DockAnimationKind::Expand,
    )? {
        apply_focus_widget_window_shape(window);
        if next_state.mode == FocusWidgetDockMode::Peek {
            schedule_peek_mouse_auto_collapse(app);
        }
        Ok(next_state)
    } else {
        Ok(current_dock_state())
    }
}

fn animate_focus_widget_geometry(
    window: &WebviewWindow,
    from: FocusWidgetGeometry,
    to: FocusWidgetGeometry,
    kind: DockAnimationKind,
) -> Result<bool, String> {
    let generation = next_window_animation_generation();
    let start_x = from.x.unwrap_or_else(|| to.x.unwrap_or(0.0));
    let start_y = from.y.unwrap_or_else(|| to.y.unwrap_or(0.0));
    let end_x = to.x.unwrap_or(start_x);
    let end_y = to.y.unwrap_or(start_y);
    let duration = dock_animation_duration(kind);
    let steps = dock_animation_steps(duration);
    let started_at = Instant::now();

    prepare_focus_widget_window_animation(window, kind);

    for step in 1..=steps {
        if !is_current_window_animation(generation) {
            return Ok(false);
        }

        let frame_interval_count = steps.saturating_sub(1).max(1);
        let target_frame_at = started_at
            + Duration::from_secs_f64(
                duration.as_secs_f64() * (step.saturating_sub(1)) as f64
                    / frame_interval_count as f64,
            );
        let now = Instant::now();
        if target_frame_at > now {
            std::thread::sleep(target_frame_at - now);
        }

        let progress = dock_animation_ease(kind, step as f64 / steps as f64);
        let next_x = lerp(start_x, end_x, progress);
        let next_y = lerp(start_y, end_y, progress);
        let next_width = lerp(from.width, to.width, progress);
        let next_height = lerp(from.height, to.height, progress);

        set_focus_widget_geometry_frame(window, next_x, next_y, next_width, next_height)?;
    }

    if is_current_window_animation(generation) {
        set_focus_widget_geometry_frame(window, end_x, end_y, to.width, to.height)?;
    }

    Ok(is_current_window_animation(generation))
}

#[cfg(windows)]
fn set_focus_widget_geometry_frame(
    window: &WebviewWindow,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let Ok(hwnd) = window.hwnd() else {
        return set_focus_widget_geometry_frame_fallback(window, x, y, width, height);
    };
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let physical_x = (x * scale_factor).round() as i32;
    let physical_y = (y * scale_factor).round() as i32;
    let physical_width = ((width * scale_factor).round() as i32).max(1);
    let physical_height = ((height * scale_factor).round() as i32).max(1);

    unsafe {
        SetWindowPos(
            hwnd,
            None,
            physical_x,
            physical_y,
            physical_width,
            physical_height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn set_focus_widget_geometry_frame(
    window: &WebviewWindow,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    set_focus_widget_geometry_frame_fallback(window, x, y, width, height)
}

fn set_focus_widget_geometry_frame_fallback(
    window: &WebviewWindow,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

fn next_window_animation_generation() -> u64 {
    runtime_state()
        .lock()
        .map(|mut runtime| {
            runtime.window_animation_generation =
                runtime.window_animation_generation.wrapping_add(1);
            runtime.window_animation_generation
        })
        .unwrap_or(0)
}

fn next_peek_mouse_watch_generation() -> u64 {
    runtime_state()
        .lock()
        .map(|mut runtime| {
            runtime.peek_mouse_watch_generation =
                runtime.peek_mouse_watch_generation.wrapping_add(1);
            runtime.peek_mouse_watch_generation
        })
        .unwrap_or(0)
}

fn is_current_window_animation(generation: u64) -> bool {
    runtime_state()
        .lock()
        .map(|runtime| runtime.window_animation_generation == generation)
        .unwrap_or(false)
}

fn lerp(from: f64, to: f64, progress: f64) -> f64 {
    from + (to - from) * progress
}

fn dock_animation_duration(kind: DockAnimationKind) -> Duration {
    Duration::from_millis(match kind {
        DockAnimationKind::Expand => DOCK_EXPAND_ANIMATION_MS,
        DockAnimationKind::Collapse => DOCK_COLLAPSE_ANIMATION_MS,
    })
}

fn dock_animation_steps(duration: Duration) -> u32 {
    ((duration.as_millis() as f64 / DOCK_ANIMATION_FRAME_MS as f64).ceil() as u32).max(1)
}

fn dock_animation_ease(kind: DockAnimationKind, progress: f64) -> f64 {
    match kind {
        DockAnimationKind::Expand => cubic_bezier_y_for_x(progress, 0.16, 1.0, 0.30, 1.0),
        DockAnimationKind::Collapse => cubic_bezier_y_for_x(progress, 0.30, 0.0, 0.20, 1.0),
    }
}

fn cubic_bezier_y_for_x(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let mut t = x;
    for _ in 0..6 {
        let current_x = cubic_bezier_value(t, x1, x2);
        let derivative = cubic_bezier_derivative(t, x1, x2);
        if derivative.abs() < f64::EPSILON {
            break;
        }

        t = (t - (current_x - x) / derivative).clamp(0.0, 1.0);
    }

    cubic_bezier_value(t, y1, y2).clamp(0.0, 1.0)
}

fn cubic_bezier_value(t: f64, p1: f64, p2: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * t * p1 + 3.0 * inverse * t * t * p2 + t * t * t
}

fn cubic_bezier_derivative(t: f64, p1: f64, p2: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * p1 + 6.0 * inverse * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

fn schedule_peek_mouse_auto_collapse(app: &AppHandle) {
    let generation = next_peek_mouse_watch_generation();
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let mut outside_since: Option<Instant> = None;

        loop {
            tokio::time::sleep(Duration::from_millis(PEEK_MOUSE_POLL_MS)).await;

            let should_continue = runtime_state()
                .lock()
                .map(|runtime| {
                    runtime.peek_mouse_watch_generation == generation
                        && runtime.dock_state.mode == FocusWidgetDockMode::Peek
                })
                .unwrap_or(false);

            if !should_continue {
                return;
            }

            let Some(window) = app.get_webview_window(FOCUS_WIDGET_LABEL) else {
                return;
            };
            let Ok(true) = window.is_visible() else {
                return;
            };

            if focus_widget_cursor_is_inside_window(&window, PEEK_MOUSE_EXIT_MARGIN).unwrap_or(true)
            {
                outside_since = None;
                continue;
            }

            let first_outside_at = outside_since.get_or_insert_with(Instant::now);
            if first_outside_at.elapsed() < Duration::from_millis(PEEK_MOUSE_EXIT_DELAY_MS) {
                continue;
            }

            let _ = collapse_window_from_current_state(&app, &window);
            return;
        }
    });
}

fn focus_widget_cursor_is_inside_window(
    window: &WebviewWindow,
    margin: f64,
) -> Result<bool, String> {
    let cursor = window
        .cursor_position()
        .map_err(|error| error.to_string())?;
    let bounds = physical_window_bounds_from_window(window)?;
    Ok(cursor_is_inside_window_bounds(
        cursor.x, cursor.y, bounds, margin,
    ))
}

fn physical_window_bounds_from_window(
    window: &WebviewWindow,
) -> Result<PhysicalWindowBounds, String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;

    Ok(PhysicalWindowBounds {
        x: position.x as f64,
        y: position.y as f64,
        width: size.width as f64,
        height: size.height as f64,
    })
}

fn cursor_is_inside_window_bounds(
    cursor_x: f64,
    cursor_y: f64,
    bounds: PhysicalWindowBounds,
    margin: f64,
) -> bool {
    cursor_x >= bounds.x - margin
        && cursor_x <= bounds.x + bounds.width + margin
        && cursor_y >= bounds.y - margin
        && cursor_y <= bounds.y + bounds.height + margin
}

fn nearest_dock_edge(window: &WebviewWindow) -> Result<Option<FocusWidgetDockEdge>, String> {
    let geometry = logical_geometry_from_window(window)?;
    let area = current_work_area(window)?;
    let Some(x) = geometry.x else {
        return Ok(None);
    };
    let Some(y) = geometry.y else {
        return Ok(None);
    };

    let right = x + geometry.width;
    let bottom = y + geometry.height;
    let area_right = area.x + area.width;
    let area_bottom = area.y + area.height;
    let distances = [
        (FocusWidgetDockEdge::Left, (x - area.x).abs()),
        (FocusWidgetDockEdge::Right, (area_right - right).abs()),
        (FocusWidgetDockEdge::Top, (y - area.y).abs()),
        (FocusWidgetDockEdge::Bottom, (area_bottom - bottom).abs()),
    ];

    Ok(distances
        .into_iter()
        .filter(|(_, distance)| *distance <= EDGE_COLLAPSE_THRESHOLD)
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(edge, _)| edge))
}

fn current_work_area(window: &WebviewWindow) -> Result<LogicalWorkArea, String> {
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No monitor available for focus widget".to_string())?;
    let work_area = monitor.work_area();
    let position = work_area.position.to_logical::<f64>(scale_factor);
    let size = work_area.size.to_logical::<f64>(scale_factor);

    Ok(LogicalWorkArea {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

fn logical_geometry_from_window(window: &WebviewWindow) -> Result<FocusWidgetGeometry, String> {
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let logical_position = position.to_logical::<f64>(scale_factor);
    let logical_size = size.to_logical::<f64>(scale_factor);

    Ok(FocusWidgetGeometry {
        x: Some(logical_position.x),
        y: Some(logical_position.y),
        width: logical_size.width,
        height: logical_size.height,
    })
}

fn collapsed_size(edge: FocusWidgetDockEdge) -> WidgetSize {
    match edge {
        FocusWidgetDockEdge::Left | FocusWidgetDockEdge::Right => WidgetSize {
            width: COLLAPSED_SIDE_WIDTH,
            height: COLLAPSED_SIDE_HEIGHT,
        },
        FocusWidgetDockEdge::Top | FocusWidgetDockEdge::Bottom => WidgetSize {
            width: COLLAPSED_BAR_WIDTH,
            height: COLLAPSED_BAR_HEIGHT,
        },
    }
}

fn docked_position(
    edge: FocusWidgetDockEdge,
    area: &LogicalWorkArea,
    source: FocusWidgetGeometry,
    size: WidgetSize,
) -> FocusWidgetGeometry {
    let source_x = source.x.unwrap_or(area.x);
    let source_y = source.y.unwrap_or(area.y);
    let centered_x = source_x + (source.width - size.width) / 2.0;
    let centered_y = source_y + (source.height - size.height) / 2.0;
    let x_limit =
        (area.x + area.width - size.width - EDGE_SAFE_MARGIN).max(area.x + EDGE_SAFE_MARGIN);
    let y_limit =
        (area.y + area.height - size.height - EDGE_SAFE_MARGIN).max(area.y + EDGE_SAFE_MARGIN);

    match edge {
        FocusWidgetDockEdge::Left => FocusWidgetGeometry {
            x: Some(area.x),
            y: Some(centered_y.clamp(area.y + EDGE_SAFE_MARGIN, y_limit)),
            width: size.width,
            height: size.height,
        },
        FocusWidgetDockEdge::Right => FocusWidgetGeometry {
            x: Some(area.x + area.width - size.width),
            y: Some(centered_y.clamp(area.y + EDGE_SAFE_MARGIN, y_limit)),
            width: size.width,
            height: size.height,
        },
        FocusWidgetDockEdge::Top => FocusWidgetGeometry {
            x: Some(centered_x.clamp(area.x + EDGE_SAFE_MARGIN, x_limit)),
            y: Some(area.y),
            width: size.width,
            height: size.height,
        },
        FocusWidgetDockEdge::Bottom => FocusWidgetGeometry {
            x: Some(centered_x.clamp(area.x + EDGE_SAFE_MARGIN, x_limit)),
            y: Some(area.y + area.height - size.height),
            width: size.width,
            height: size.height,
        },
    }
}

fn expanded_position(
    edge: FocusWidgetDockEdge,
    area: &LogicalWorkArea,
    normal: FocusWidgetGeometry,
    current: FocusWidgetGeometry,
) -> FocusWidgetGeometry {
    let normal = normalize_normal_geometry(normal);
    let current_x = current.x.unwrap_or(area.x);
    let current_y = current.y.unwrap_or(area.y);
    let x_limit =
        (area.x + area.width - normal.width - EDGE_SAFE_MARGIN).max(area.x + EDGE_SAFE_MARGIN);
    let y_limit =
        (area.y + area.height - normal.height - EDGE_SAFE_MARGIN).max(area.y + EDGE_SAFE_MARGIN);
    let centered_x = current_x + (current.width - normal.width) / 2.0;
    let centered_y = current_y + (current.height - normal.height) / 2.0;
    let normal_x = normal.x.unwrap_or(centered_x);
    let normal_y = normal.y.unwrap_or(centered_y);

    match edge {
        FocusWidgetDockEdge::Left => FocusWidgetGeometry {
            x: Some(area.x),
            y: Some(normal_y.clamp(area.y + EDGE_SAFE_MARGIN, y_limit)),
            ..normal
        },
        FocusWidgetDockEdge::Right => FocusWidgetGeometry {
            x: Some(area.x + area.width - normal.width),
            y: Some(normal_y.clamp(area.y + EDGE_SAFE_MARGIN, y_limit)),
            ..normal
        },
        FocusWidgetDockEdge::Top => FocusWidgetGeometry {
            x: Some(normal_x.clamp(area.x + EDGE_SAFE_MARGIN, x_limit)),
            y: Some(area.y),
            ..normal
        },
        FocusWidgetDockEdge::Bottom => FocusWidgetGeometry {
            x: Some(normal_x.clamp(area.x + EDGE_SAFE_MARGIN, x_limit)),
            y: Some(area.y + area.height - normal.height),
            ..normal
        },
    }
}

fn normalize_normal_geometry(geometry: FocusWidgetGeometry) -> FocusWidgetGeometry {
    FocusWidgetGeometry {
        x: geometry.x,
        y: geometry.y,
        width: geometry
            .width
            .clamp(MIN_NORMAL_WIDTH as f64, MAX_NORMAL_WIDTH as f64),
        height: geometry
            .height
            .clamp(MIN_NORMAL_HEIGHT as f64, MAX_NORMAL_HEIGHT as f64),
    }
}

fn apply_size_constraints(window: &WebviewWindow, compact: bool) -> Result<(), String> {
    let min_width = if compact {
        MIN_WINDOW_WIDTH
    } else {
        MIN_NORMAL_WIDTH
    };
    let min_height = if compact {
        MIN_WINDOW_HEIGHT
    } else {
        MIN_NORMAL_HEIGHT
    };
    let _ = window.set_min_size(Some(LogicalSize::new(min_width as f64, min_height as f64)));
    let _ = window.set_max_size(Some(LogicalSize::new(
        MAX_NORMAL_WIDTH as f64,
        MAX_NORMAL_HEIGHT as f64,
    )));
    Ok(())
}

#[cfg(windows)]
fn apply_focus_widget_window_shape(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    enforce_focus_widget_chrome_less(hwnd);

    let mut rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return;
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return;
    }

    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let dock_state = current_dock_state();
    let radius = focus_widget_window_radius(width, height, scale_factor, dock_state);
    let Some(region) = focus_widget_window_region(width, height, radius, dock_state) else {
        return;
    };

    if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
        let _ = unsafe { DeleteObject(HGDIOBJ(region.0)) };
    }
}

#[cfg(not(windows))]
fn apply_focus_widget_window_shape(_window: &WebviewWindow) {}

#[cfg(windows)]
fn install_focus_widget_chrome_guard(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    enforce_focus_widget_chrome_less(hwnd);

    if FOCUS_WIDGET_ORIGINAL_WNDPROC.get().is_some() {
        return;
    }

    let previous = unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWL_WNDPROC,
            focus_widget_wndproc as *const () as isize,
        )
    };
    if previous != 0 {
        let _ = FOCUS_WIDGET_ORIGINAL_WNDPROC.set(previous as usize);
    }
}

#[cfg(not(windows))]
fn install_focus_widget_chrome_guard(_window: &WebviewWindow) {}

#[cfg(windows)]
fn prepare_focus_widget_window_animation(window: &WebviewWindow, kind: DockAnimationKind) {
    if let Ok(hwnd) = window.hwnd() {
        enforce_focus_widget_chrome_less(hwnd);
        if matches!(kind, DockAnimationKind::Expand) {
            let _ = unsafe { SetWindowRgn(hwnd, None, false) };
        }
    }
}

#[cfg(not(windows))]
fn prepare_focus_widget_window_animation(_window: &WebviewWindow, _kind: DockAnimationKind) {}

#[cfg(windows)]
unsafe extern "system" fn focus_widget_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE | WM_NCPAINT => return LRESULT(0),
        // Returning FALSE here can block Windows from activating the main window again.
        WM_NCACTIVATE => return LRESULT(1),
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        _ => {}
    }

    if let Some(previous) = FOCUS_WIDGET_ORIGINAL_WNDPROC.get().copied() {
        let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
        return unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) };
    }

    LRESULT(0)
}

#[cfg(windows)]
fn enforce_focus_widget_chrome_less(hwnd: HWND) {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    let blocked_style_bits =
        (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0 as isize;
    let next_style = style & !blocked_style_bits;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let blocked_ex_style_bits =
        (WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE).0 as isize;
    let required_ex_style_bits = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize;
    let next_ex_style = (ex_style & !blocked_ex_style_bits) | required_ex_style_bits;

    if next_style == style && next_ex_style == ex_style {
        return;
    }

    unsafe {
        if next_style != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, next_style);
        }
        if next_ex_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_ex_style);
        }
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(windows)]
fn focus_widget_window_radius(
    width: i32,
    height: i32,
    scale_factor: f64,
    dock_state: FocusWidgetDockState,
) -> i32 {
    let logical_radius = if dock_state.mode == FocusWidgetDockMode::Collapsed {
        COLLAPSED_WINDOW_RADIUS
    } else {
        FLOATING_WINDOW_RADIUS
    };
    let max_radius = (width.min(height) / 2).max(1);
    ((logical_radius as f64 * scale_factor).round() as i32).clamp(1, max_radius)
}

#[cfg(windows)]
fn focus_widget_window_region(
    width: i32,
    height: i32,
    radius: i32,
    dock_state: FocusWidgetDockState,
) -> Option<windows::Win32::Graphics::Gdi::HRGN> {
    let diameter = radius.saturating_mul(2);
    let region = unsafe { CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter) };
    if region.is_invalid() {
        return None;
    }

    if dock_state.mode != FocusWidgetDockMode::Collapsed {
        return Some(region);
    }

    let Some(edge) = dock_state.edge else {
        return Some(region);
    };

    let fill_region = match edge {
        FocusWidgetDockEdge::Left => unsafe { CreateRectRgn(0, 0, radius + 1, height + 1) },
        FocusWidgetDockEdge::Right => unsafe {
            CreateRectRgn(width - radius, 0, width + 1, height + 1)
        },
        FocusWidgetDockEdge::Top => unsafe { CreateRectRgn(0, 0, width + 1, radius + 1) },
        FocusWidgetDockEdge::Bottom => unsafe {
            CreateRectRgn(0, height - radius, width + 1, height + 1)
        },
    };

    if fill_region.is_invalid() {
        return Some(region);
    }

    let result = unsafe { CombineRgn(Some(region), Some(region), Some(fill_region), RGN_OR) };
    let _ = unsafe { DeleteObject(HGDIOBJ(fill_region.0)) };
    if result == RGN_ERROR {
        return Some(region);
    }

    Some(region)
}

fn load_background_study_mode_visibility(
    app: &AppHandle,
) -> Result<StudyModeVisibilitySnapshot, String> {
    let connection = open_database(&database_path(app)?)?;
    let snapshot = connection
        .query_row(
            "
            SELECT id, phase, status, paused_at
            FROM study_modes
            ORDER BY
              CASE WHEN status = 'active' THEN 0 ELSE 1 END,
              state_revision DESC,
              updated_at DESC,
              id DESC
            LIMIT 1
            ",
            [],
            |row| {
                let paused_at: Option<String> = row.get(3)?;
                Ok(StudyModeVisibilitySnapshot {
                    study_mode_id: row.get(0)?,
                    phase: row.get(1)?,
                    status: row.get(2)?,
                    is_paused: paused_at.is_some(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(snapshot.unwrap_or_else(StudyModeVisibilitySnapshot::idle))
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

impl StudyModeVisibilitySnapshot {
    fn idle() -> Self {
        Self {
            study_mode_id: None,
            phase: "idle".to_string(),
            status: "idle".to_string(),
            is_paused: false,
        }
    }

    fn should_auto_show_widget(&self) -> bool {
        self.status == "active"
    }
}

impl From<&StudyModeState> for StudyModeVisibilitySnapshot {
    fn from(state: &StudyModeState) -> Self {
        Self {
            study_mode_id: state.id,
            phase: state.phase.clone(),
            status: state.status.clone(),
            is_paused: state.is_paused,
        }
    }
}

fn runtime_state() -> &'static Mutex<FocusWidgetRuntimeState> {
    RUNTIME_STATE.get_or_init(|| Mutex::new(FocusWidgetRuntimeState::default()))
}

fn current_dock_state() -> FocusWidgetDockState {
    runtime_state()
        .lock()
        .map(|runtime| runtime.dock_state)
        .unwrap_or_default()
}

fn set_dock_state(app: &AppHandle, next_state: FocusWidgetDockState) {
    if store_dock_state(next_state) {
        emit_dock_state_change(app, next_state);
    }
}

fn store_dock_state(next_state: FocusWidgetDockState) -> bool {
    match runtime_state().lock() {
        Ok(mut runtime) => {
            let changed = runtime.dock_state != next_state;
            runtime.dock_state = next_state;
            runtime.peek_mouse_watch_generation =
                runtime.peek_mouse_watch_generation.wrapping_add(1);
            changed
        }
        Err(_) => false,
    }
}

fn emit_dock_state_change(app: &AppHandle, next_state: FocusWidgetDockState) {
    let _ = app.emit(FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT, next_state);
}

fn emit_focus_widget_dock_state(app: &AppHandle) {
    let _ = app.emit(FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT, current_dock_state());
}

fn remember_normal_geometry(geometry: FocusWidgetGeometry) {
    if let Ok(mut runtime) = runtime_state().lock() {
        runtime.normal_geometry = Some(normalize_normal_geometry(geometry));
    }
}

fn runtime_normal_geometry() -> Option<FocusWidgetGeometry> {
    runtime_state()
        .lock()
        .ok()
        .and_then(|runtime| runtime.normal_geometry)
}

fn should_suppress_geometry_events() -> bool {
    runtime_state()
        .lock()
        .map(|runtime| runtime.suppress_geometry_events)
        .unwrap_or(false)
}

fn suppress_geometry_events_briefly() {
    let generation = match runtime_state().lock() {
        Ok(mut runtime) => {
            runtime.geometry_suppression_generation =
                runtime.geometry_suppression_generation.wrapping_add(1);
            runtime.suppress_geometry_events = true;
            runtime.geometry_suppression_generation
        }
        Err(_) => return,
    };

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(GEOMETRY_DEBOUNCE_MS + 80)).await;
        if let Ok(mut runtime) = runtime_state().lock() {
            if runtime.geometry_suppression_generation == generation {
                runtime.suppress_geometry_events = false;
            }
        }
    });
}

fn note_current_study_mode(study_mode_id: Option<i64>) {
    let Ok(mut runtime) = runtime_state().lock() else {
        return;
    };

    if runtime.current_study_mode_id != study_mode_id {
        runtime.current_study_mode_id = study_mode_id;
        runtime.manual_hidden_for_study_mode_id = None;
        runtime.auto_visible_study_mode_id = None;
    }
}

fn mark_auto_visible(study_mode_id: i64) {
    if let Ok(mut runtime) = runtime_state().lock() {
        runtime.auto_visible_study_mode_id = Some(study_mode_id);
    }
}

fn mark_lifecycle_hidden(study_mode_id: Option<i64>) {
    if let Ok(mut runtime) = runtime_state().lock() {
        if runtime.current_study_mode_id == study_mode_id {
            runtime.auto_visible_study_mode_id = None;
        }
    }
}

fn mark_manual_hidden_for_current_study_mode(app: &AppHandle) {
    let Ok(snapshot) = load_background_study_mode_visibility(app) else {
        return;
    };
    let Some(study_mode_id) = snapshot.study_mode_id else {
        return;
    };

    if let Ok(mut runtime) = runtime_state().lock() {
        runtime.current_study_mode_id = Some(study_mode_id);
        runtime.manual_hidden_for_study_mode_id = Some(study_mode_id);
        runtime.auto_visible_study_mode_id = None;
    }
}

fn clear_manual_hidden_for_current_study_mode(app: &AppHandle) {
    let Ok(snapshot) = load_background_study_mode_visibility(app) else {
        return;
    };

    if let Ok(mut runtime) = runtime_state().lock() {
        runtime.current_study_mode_id = snapshot.study_mode_id;
        if runtime.manual_hidden_for_study_mode_id == snapshot.study_mode_id {
            runtime.manual_hidden_for_study_mode_id = None;
        }
    }
}

fn remember_external_hide_if_needed(app: &AppHandle, study_mode_id: i64) {
    let Some(window) = app.get_webview_window(FOCUS_WIDGET_LABEL) else {
        return;
    };
    let Ok(false) = window.is_visible() else {
        return;
    };

    if let Ok(mut runtime) = runtime_state().lock() {
        if runtime.auto_visible_study_mode_id == Some(study_mode_id) {
            runtime.manual_hidden_for_study_mode_id = Some(study_mode_id);
            runtime.auto_visible_study_mode_id = None;
        }
    }
}

fn is_manually_hidden_for_study_mode(study_mode_id: i64) -> bool {
    runtime_state()
        .lock()
        .map(|runtime| runtime.manual_hidden_for_study_mode_id == Some(study_mode_id))
        .unwrap_or(false)
}

fn emit_focus_widget_study_state<T>(app: &AppHandle, payload: &T)
where
    T: Serialize + Clone,
{
    let _ = app.emit(FOCUS_WIDGET_STUDY_STATE_EVENT, payload.clone());
    let _ = app.emit(FOCUS_WIDGET_STUDY_STATE_CHANGED_EVENT, payload.clone());
}

fn round_to_i64(value: f64) -> i64 {
    value.round() as i64
}

#[cfg(test)]
mod tests {
    use super::{
        cursor_is_inside_window_bounds, PhysicalWindowBounds, StudyModeVisibilitySnapshot,
    };

    fn snapshot(status: &str, is_paused: bool) -> StudyModeVisibilitySnapshot {
        StudyModeVisibilitySnapshot {
            study_mode_id: Some(1),
            phase: "focus".to_string(),
            status: status.to_string(),
            is_paused,
        }
    }

    #[test]
    fn active_study_mode_auto_shows_even_when_paused() {
        assert!(snapshot("active", false).should_auto_show_widget());
        assert!(snapshot("active", true).should_auto_show_widget());
    }

    #[test]
    fn inactive_study_mode_does_not_auto_show() {
        assert!(!snapshot("finished", false).should_auto_show_widget());
        assert!(!snapshot("idle", false).should_auto_show_widget());
    }

    #[test]
    fn cursor_bounds_include_exit_margin() {
        let bounds = PhysicalWindowBounds {
            x: 100.0,
            y: 200.0,
            width: 280.0,
            height: 144.0,
        };

        assert!(cursor_is_inside_window_bounds(96.0, 196.0, bounds, 8.0));
        assert!(cursor_is_inside_window_bounds(388.0, 352.0, bounds, 8.0));
    }

    #[test]
    fn cursor_bounds_exclude_points_beyond_exit_margin() {
        let bounds = PhysicalWindowBounds {
            x: 100.0,
            y: 200.0,
            width: 280.0,
            height: 144.0,
        };

        assert!(!cursor_is_inside_window_bounds(91.0, 250.0, bounds, 8.0));
        assert!(!cursor_is_inside_window_bounds(250.0, 353.0, bounds, 8.0));
    }
}
