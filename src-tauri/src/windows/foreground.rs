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

#[derive(Debug, Clone, Serialize)]
pub struct ForegroundApp {
    #[serde(skip)]
    window: HWND,
    pub process_id: u32,
    pub process_name: String,
    pub process_path: Option<String>,
    pub window_title: String,
}

impl ForegroundApp {
    pub fn window(&self) -> HWND {
        self.window
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

    Ok(ForegroundApp {
        window,
        process_id,
        process_name,
        process_path,
        window_title,
    })
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
