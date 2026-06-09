use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::{
    commands::{
        focus::StudyModeState,
        settings::{get_app_settings, save_focus_widget_geometry, AppSettings},
    },
    storage::db::open_database,
};

pub const FOCUS_WIDGET_LABEL: &str = "focus-widget";

const MAIN_WINDOW_LABEL: &str = "main";
const FOCUS_WIDGET_TITLE: &str = "Focus Widget";
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
const DEFAULT_HEIGHT: i64 = 144;
const MIN_NORMAL_WIDTH: i64 = 240;
const MAX_NORMAL_WIDTH: i64 = 420;
const MIN_NORMAL_HEIGHT: i64 = 112;
const MAX_NORMAL_HEIGHT: i64 = 240;
const MIN_WINDOW_WIDTH: i64 = 36;
const MIN_WINDOW_HEIGHT: i64 = 36;
const COLLAPSED_SIDE_WIDTH: f64 = 36.0;
const COLLAPSED_SIDE_HEIGHT: f64 = 132.0;
const COLLAPSED_BAR_WIDTH: f64 = 280.0;
const COLLAPSED_BAR_HEIGHT: f64 = 36.0;
const EDGE_COLLAPSE_THRESHOLD: f64 = 18.0;
const EDGE_SAFE_MARGIN: f64 = 8.0;
const GEOMETRY_DEBOUNCE_MS: u64 = 250;

static CREATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_STATE: OnceLock<Mutex<FocusWidgetRuntimeState>> = OnceLock::new();

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
        let window = build_focus_widget_window(&self.app, geometry, settings.focus_widget_always_on_top)?;
        attach_window_handlers(&self.app, &window);
        Ok(window)
    }

    pub fn hide(&self) -> Result<(), String> {
        mark_manual_hidden_for_current_study_mode(&self.app);
        self.hide_without_manual_suppression()
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
        .resizable(true)
        .skip_taskbar(true)
        .always_on_top(always_on_top)
        .shadow(false)
        .visible(false)
        .focused(false)
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

    builder.build().map_err(|error| error.to_string())
}

fn configure_focus_widget_window(
    window: &WebviewWindow,
    settings: &AppSettings,
) -> Result<(), String> {
    let geometry = focus_widget_geometry_from_settings(settings);
    let dock_state = current_dock_state();
    let _ = window.set_title(FOCUS_WIDGET_TITLE);
    let _ = window.set_decorations(false);
    let _ = window.set_resizable(true);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(settings.focus_widget_always_on_top);
    let _ = window.set_shadow(false);
    apply_size_constraints(window, dock_state.mode == FocusWidgetDockMode::Collapsed)?;

    if settings.focus_widget_remember_geometry && dock_state.is_floating() {
        window
            .set_size(LogicalSize::new(geometry.width, geometry.height))
            .map_err(|error| error.to_string())?;
        if let (Some(x), Some(y)) = (geometry.x, geometry.y) {
            let _ = window.set_position(LogicalPosition::new(x, y));
        }
    }

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

    let should_auto_show = settings.focus_widget_enabled
        && settings.focus_widget_auto_follow
        && snapshot.should_show_widget();

    if !should_auto_show {
        mark_lifecycle_hidden(snapshot.study_mode_id);
        return focus_widget(app).hide_without_manual_suppression();
    }

    if let Some(study_mode_id) = snapshot.study_mode_id {
        remember_external_hide_if_needed(app, study_mode_id);
        if is_manually_hidden_for_study_mode(study_mode_id) {
            return Ok(());
        }
    }

    let window = focus_widget(app).ensure()?;
    show_window_without_focus(&window, settings.focus_widget_always_on_top)?;
    emit_focus_widget_dock_state(app);
    schedule_edge_collapse_check(app);
    if let Some(study_mode_id) = snapshot.study_mode_id {
        mark_auto_visible(study_mode_id);
    }

    Ok(())
}

fn show_window_without_focus(window: &WebviewWindow, always_on_top: bool) -> Result<(), String> {
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

    set_dock_state(app, next_state);
    suppress_geometry_events_briefly();
    apply_size_constraints(window, true)?;
    window
        .set_size(LogicalSize::new(size.width, size.height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(LogicalPosition::new(position.x.unwrap_or(area.x), position.y.unwrap_or(area.y)))
        .map_err(|error| error.to_string())?;
    Ok(next_state)
}

fn expand_window_to_edge(
    app: &AppHandle,
    window: &WebviewWindow,
    edge: FocusWidgetDockEdge,
    mode: FocusWidgetDockMode,
) -> Result<FocusWidgetDockState, String> {
    let current_geometry = logical_geometry_from_window(window)?;
    let normal_geometry = runtime_normal_geometry()
        .unwrap_or_else(|| normalize_normal_geometry(focus_widget_geometry_from_settings(&get_app_settings(app.clone()).unwrap_or_default())));
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
    window
        .set_size(LogicalSize::new(expanded_geometry.width, expanded_geometry.height))
        .map_err(|error| error.to_string())?;
    if let (Some(x), Some(y)) = (expanded_geometry.x, expanded_geometry.y) {
        window
            .set_position(LogicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
    }

    Ok(next_state)
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
            left.partial_cmp(right)
                .unwrap_or(std::cmp::Ordering::Equal)
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
    let x_limit = (area.x + area.width - size.width - EDGE_SAFE_MARGIN).max(area.x + EDGE_SAFE_MARGIN);
    let y_limit = (area.y + area.height - size.height - EDGE_SAFE_MARGIN).max(area.y + EDGE_SAFE_MARGIN);

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
    let x_limit = (area.x + area.width - normal.width - EDGE_SAFE_MARGIN).max(area.x + EDGE_SAFE_MARGIN);
    let y_limit = (area.y + area.height - normal.height - EDGE_SAFE_MARGIN).max(area.y + EDGE_SAFE_MARGIN);
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
    let _ = window.set_resizable(!compact);
    Ok(())
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

    fn should_show_widget(&self) -> bool {
        self.status == "active" && !self.is_paused
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
    let changed = match runtime_state().lock() {
        Ok(mut runtime) => {
            let changed = runtime.dock_state != next_state;
            runtime.dock_state = next_state;
            changed
        }
        Err(_) => false,
    };

    if changed {
        let _ = app.emit(FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT, next_state);
    }
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
