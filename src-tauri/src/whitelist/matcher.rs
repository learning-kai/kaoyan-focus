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
    pub detected_domain: Option<String>,
}

pub fn is_foreground_app_allowed(
    app: &ForegroundApp,
    whitelist_process_names: &[String],
    whitelist_domains: &[String],
) -> WhitelistMatchResult {
    let process_name = app.process_name.to_ascii_lowercase();

    if DEFAULT_ALLOWED_PROCESS_NAMES
        .iter()
        .any(|allowed_name| process_name == allowed_name.to_ascii_lowercase())
    {
        return WhitelistMatchResult {
            allowed: true,
            reason: "默认系统放行".to_string(),
            matched_process_name: Some(app.process_name.clone()),
            detected_domain: None,
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
            detected_domain: None,
        };
    }

    let detected_domain = detect_browser_domain(app);
    if let Some(domain) = detected_domain.as_deref() {
        if let Some(matched_domain) = whitelist_domains
            .iter()
            .map(|candidate| normalize_domain(candidate))
            .find(|candidate| domain_matches(domain, candidate))
        {
            return WhitelistMatchResult {
                allowed: true,
                reason: "命中网站白名单".to_string(),
                matched_process_name: Some(matched_domain),
                detected_domain,
            };
        }

        return WhitelistMatchResult {
            allowed: false,
            reason: format!("浏览器网站 {domain} 不在白名单"),
            matched_process_name: None,
            detected_domain,
        };
    }

    WhitelistMatchResult {
        allowed: false,
        reason: "不在白名单".to_string(),
        matched_process_name: None,
        detected_domain: None,
    }
}

fn is_supported_browser(process_name: &str) -> bool {
    matches!(
        process_name,
        "chrome.exe"
            | "msedge.exe"
            | "firefox.exe"
            | "brave.exe"
            | "opera.exe"
            | "vivaldi.exe"
            | "iexplore.exe"
    )
}

fn detect_browser_domain(app: &ForegroundApp) -> Option<String> {
    let process_name = app.process_name.to_ascii_lowercase();
    if !is_supported_browser(&process_name) {
        return None;
    }

    extract_domain_from_text(&app.window_title)
}

fn extract_domain_from_text(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let separators: &[char] = &[' ', '\t', '\r', '\n', '/', '\\', '|', '—', '-', '(', ')', '[', ']', '<', '>', '"', '\''];

    lower
        .split(separators)
        .filter_map(|token| {
            let trimmed = token
                .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.' && character != ':')
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_start_matches("www.");
            normalize_domain_candidate(trimmed)
        })
        .next()
}

fn normalize_domain_candidate(value: &str) -> Option<String> {
    let domain = normalize_domain(value);
    if !domain.contains('.') {
        return None;
    }

    if domain
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '.')
    {
        Some(domain)
    } else {
        None
    }
}

fn normalize_domain(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn domain_matches(detected_domain: &str, allowed_domain: &str) -> bool {
    detected_domain == allowed_domain
        || detected_domain
            .strip_suffix(allowed_domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}
