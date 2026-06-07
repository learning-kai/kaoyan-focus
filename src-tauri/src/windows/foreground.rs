use serde::Serialize;
use windows::Win32::{
    Foundation::{CloseHandle, HWND},
    System::{
        ProcessStatus::K32GetModuleFileNameExW,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ},
    },
    UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    },
};

use super::{
    browser_url::read_browser_url,
    potplayer::{detect_potplayer_media_for_process, is_supported_potplayer_process},
};

#[derive(Debug, Clone, Serialize)]
pub struct ForegroundApp {
    #[serde(skip)]
    window: HWND,
    pub process_id: u32,
    pub process_name: String,
    pub process_path: Option<String>,
    pub window_title: String,
    pub browser_url: Option<String>,
    pub potplayer_media_path: Option<String>,
}

impl ForegroundApp {
    pub fn window(&self) -> HWND {
        self.window
    }

    #[cfg(test)]
    pub fn for_test(
        process_id: u32,
        process_name: String,
        process_path: Option<String>,
        window_title: String,
        browser_url: Option<String>,
        potplayer_media_path: Option<String>,
    ) -> Self {
        Self {
            window: HWND::default(),
            process_id,
            process_name,
            process_path,
            window_title,
            browser_url,
            potplayer_media_path,
        }
    }
}

pub fn get_foreground_app() -> Result<ForegroundApp, String> {
    let window = unsafe { GetForegroundWindow() };
    if window == HWND::default() {
        return Err("当前没有可识别的前台窗口".to_string());
    }

    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process_id));
    }

    if process_id == 0 {
        return Err("无法获取前台窗口进程 ID".to_string());
    }

    let window_title = read_window_title(window);
    let process_path = read_process_path(process_id);
    let process_name = process_path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("pid-{process_id}"));
    let browser_url = if is_supported_browser_process(&process_name) {
        read_browser_url(window)
    } else {
        None
    };
    let potplayer_media_path = if is_supported_potplayer_process(&process_name) {
        detect_potplayer_media_for_process(
            &process_name,
            process_path.as_deref(),
            Some(&window_title),
        )
        .and_then(|media| media.media_path)
    } else {
        None
    };

    Ok(ForegroundApp {
        window,
        process_id,
        process_name,
        process_path,
        window_title,
        browser_url,
        potplayer_media_path,
    })
}

fn is_supported_browser_process(process_name: &str) -> bool {
    matches!(
        process_name.to_ascii_lowercase().as_str(),
        "chrome.exe"
            | "msedge.exe"
            | "firefox.exe"
            | "brave.exe"
            | "opera.exe"
            | "vivaldi.exe"
            | "iexplore.exe"
    )
}

fn read_window_title(window: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return String::new();
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn read_process_path(process_id: u32) -> Option<String> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        )
    }
    .ok()?;

    let mut buffer = vec![0u16; 32768];
    let copied = unsafe { K32GetModuleFileNameExW(Some(handle), None, &mut buffer) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    if copied == 0 {
        return None;
    }

    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}
