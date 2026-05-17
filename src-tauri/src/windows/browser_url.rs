use windows::Win32::{
    Foundation::HWND,
    System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    },
    UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
        UIA_EditControlTypeId, UIA_ValuePatternId,
    },
};

pub fn read_browser_url(window: HWND) -> Option<String> {
    if window == HWND::default() {
        return None;
    }

    unsafe { read_browser_url_inner(window).ok().flatten() }
}

unsafe fn read_browser_url_inner(window: HWND) -> windows::core::Result<Option<String>> {
    let coinit = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let should_uninitialize = coinit.is_ok();
    let result = unsafe { read_browser_url_with_com(window) };
    if should_uninitialize {
        unsafe { CoUninitialize() };
    }
    result
}

unsafe fn read_browser_url_with_com(window: HWND) -> windows::core::Result<Option<String>> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root = unsafe { automation.ElementFromHandle(window)? };
    let condition = unsafe { automation.CreateTrueCondition()? };
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
    let length = unsafe { elements.Length()? }.min(80);

    for index in 0..length {
        let Ok(element) = (unsafe { elements.GetElement(index) }) else {
            continue;
        };
        let Ok(control_type) = (unsafe { element.CurrentControlType() }) else {
            continue;
        };
        if control_type != UIA_EditControlTypeId {
            continue;
        }

        let Ok(pattern) = (unsafe {
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        }) else {
            continue;
        };
        let Ok(value) = (unsafe { pattern.CurrentValue() }) else {
            continue;
        };
        let text = value.to_string();
        if looks_like_url(&text) {
            return Ok(Some(text));
        }
    }

    Ok(None)
}

fn looks_like_url(value: &str) -> bool {
    let text = value.trim().to_ascii_lowercase();
    text.starts_with("http://")
        || text.starts_with("https://")
        || text.starts_with("www.")
        || (text.contains('.') && !text.contains(' ') && !text.contains('\\'))
}
