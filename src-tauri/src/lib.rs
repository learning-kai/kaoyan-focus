use std::sync::Mutex;

mod commands {
    pub mod focus;
}
mod focus;
mod storage;

pub struct AppState {
    active_session_id: Mutex<Option<i64>>,
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            active_session_id: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            commands::focus::start_focus_session,
            commands::focus::finish_focus_session,
            commands::focus::list_focus_sessions
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
