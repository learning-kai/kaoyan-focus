use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
};

unsafe extern "system" fn enum_window_proc(hwnd: HWND, _lparam: LPARAM) -> isize {
    if IsWindowVisible(hwnd).as_bool() {
        let mut class_name = [0u16; 256];
        let mut window_text = [0u16; 256];

        let class_len = GetClassNameW(hwnd, &mut class_name);
        let text_len = GetWindowTextW(hwnd, &mut window_text);

        if class_len > 0 || text_len > 0 {
            let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);
            let text_str = String::from_utf16_lossy(&window_text[..text_len as usize]);

            if text_str.to_lowercase().contains("dock")
                || text_str.to_lowercase().contains("finder")
                || class_str.to_lowercase().contains("dock")
                || class_str.to_lowercase().contains("finder")
            {
                println!("Class: {}, Title: {}", class_str, text_str);
            }
        }
    }
    1
}

fn main() {
    unsafe {
        let callback = enum_window_proc as unsafe extern "system" fn(HWND, LPARAM) -> isize;
        let _ = EnumWindows(Some(std::mem::transmute(callback)), LPARAM(0));
    }
}
