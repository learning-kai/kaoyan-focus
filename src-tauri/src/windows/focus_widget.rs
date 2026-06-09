use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
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
const MIN_WIDTH: i64 = 240;
const MAX_WIDTH: i64 = 420;
const MIN_HEIGHT: i64 = 112;
const MAX_HEIGHT: i64 = 240;

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

#[derive(Debug, Default)]
struct FocusWidgetRuntimeState {
    handlers_attached: bool,
    current_study_mode_id: Option<i64>,
    manual_hidden_for_study_mode_id: Option<i64>,
    auto_visible_study_mode_id: Option<i64>,
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
        let window = build_focus_widget_window(&self.app, geometry)?;
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

        persist_geometry_from_window(&self.app, &window)
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
        .resizable(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .prevent_overflow()
        .min_inner_size(MIN_WIDTH as f64, MIN_HEIGHT as f64)
        .max_inner_size(MAX_WIDTH as f64, MAX_HEIGHT as f64)
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
    let _ = window.set_title(FOCUS_WIDGET_TITLE);
    let _ = window.set_decorations(false);
    let _ = window.set_resizable(true);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
    let _ = window.set_shadow(false);
    let _ = window.set_min_size(Some(LogicalSize::new(MIN_WIDTH as f64, MIN_HEIGHT as f64)));
    let _ = window.set_max_size(Some(LogicalSize::new(MAX_WIDTH as f64, MAX_HEIGHT as f64)));

    if settings.focus_widget_remember_geometry {
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
            let _ = persist_geometry_from_window(&app_for_events, &window_for_events);
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
    show_window_without_focus(&window)?;
    if let Some(study_mode_id) = snapshot.study_mode_id {
        mark_auto_visible(study_mode_id);
    }

    Ok(())
}

fn show_window_without_focus(window: &WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_always_on_top(true);
    Ok(())
}

fn persist_geometry_from_window(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let settings = get_app_settings(app.clone())?;
    if !settings.focus_widget_remember_geometry {
        return Ok(());
    }

    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let logical_position = position.to_logical::<f64>(scale_factor);
    let logical_size = size.to_logical::<f64>(scale_factor);

    save_focus_widget_geometry(
        app,
        round_to_i64(logical_position.x),
        round_to_i64(logical_position.y),
        round_to_i64(logical_size.width),
        round_to_i64(logical_size.height),
    )
}

fn focus_widget_geometry_from_settings(settings: &AppSettings) -> FocusWidgetGeometry {
    let width = settings
        .focus_widget_width
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH) as f64;
    let height = settings
        .focus_widget_height
        .unwrap_or(DEFAULT_HEIGHT)
        .clamp(MIN_HEIGHT, MAX_HEIGHT) as f64;

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
