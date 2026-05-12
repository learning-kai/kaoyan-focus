use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE},
};

pub fn close_window(window: HWND) -> Result<(), String> {
    if window == HWND::default() {
        return Err("No window to close".to_string());
    }

    unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }
        .map_err(|error| format!("Failed to post WM_CLOSE: {error}"))?;

    Ok(())
}
