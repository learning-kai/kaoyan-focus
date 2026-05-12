use serde::Serialize;

use crate::windows::foreground::ForegroundApp;

const DEFAULT_ALLOWED_PROCESS_NAMES: &[&str] = &[
    "explorer.exe",
    "sihost.exe",
    "ctfmon.exe",
    "textinputhost.exe",
    "applicationframehost.exe",
    "kaoyan-focus.exe",
];

#[derive(Debug, Clone, Serialize)]
pub struct WhitelistMatchResult {
    pub allowed: bool,
    pub reason: String,
    pub matched_process_name: Option<String>,
}

pub fn is_foreground_app_allowed(app: &ForegroundApp, whitelist_process_names: &[String]) -> WhitelistMatchResult {
    let process_name = app.process_name.to_ascii_lowercase();

    if DEFAULT_ALLOWED_PROCESS_NAMES
        .iter()
        .any(|allowed_name| process_name == allowed_name.to_ascii_lowercase())
    {
        return WhitelistMatchResult {
            allowed: true,
            reason: "默认系统放行".to_string(),
            matched_process_name: Some(app.process_name.clone()),
        };
    }

    if let Some(matched_name) = whitelist_process_names
        .iter()
        .find(|candidate| process_name == candidate.to_ascii_lowercase())
    {
        return WhitelistMatchResult {
            allowed: true,
            reason: "命中软件白名单".to_string(),
            matched_process_name: Some(matched_name.clone()),
        };
    }

    WhitelistMatchResult {
        allowed: false,
        reason: "不在白名单".to_string(),
        matched_process_name: None,
    }
}
