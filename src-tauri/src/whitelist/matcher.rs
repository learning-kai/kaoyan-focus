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
    pub matched_subject_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ProcessWhitelistRule {
    pub process_name: String,
    pub subject_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct WebsiteWhitelistRule {
    pub domain: String,
    pub launch_url: Option<String>,
    pub subject_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct BrowserUrlParts {
    domain: String,
    path: String,
}

pub fn is_foreground_app_allowed(
    app: &ForegroundApp,
    whitelist_processes: &[ProcessWhitelistRule],
    whitelist_websites: &[WebsiteWhitelistRule],
) -> WhitelistMatchResult {
    let process_name = app.process_name.to_ascii_lowercase();
    let supported_browser = is_supported_browser(&process_name);

    if DEFAULT_ALLOWED_PROCESS_NAMES
        .iter()
        .any(|allowed_name| process_name == allowed_name.to_ascii_lowercase())
    {
        return WhitelistMatchResult {
            allowed: true,
            reason: "默认系统放行".to_string(),
            matched_process_name: Some(app.process_name.clone()),
            detected_domain: None,
            matched_subject_id: None,
        };
    }

    if supported_browser && !whitelist_websites.is_empty() {
        let browser_url = detect_browser_url_parts(app);
        let detected_domain = browser_url
            .as_ref()
            .map(|parts| parts.domain.clone())
            .or_else(|| extract_domain_from_text(&app.window_title));

        if let Some(domain) = detected_domain.as_deref() {
            if let Some(matched_rule) = whitelist_websites
                .iter()
                .find(|rule| website_rule_matches(domain, browser_url.as_ref(), rule))
            {
                return WhitelistMatchResult {
                    allowed: true,
                    reason: "命中网站白名单".to_string(),
                    matched_process_name: Some(rule_label(matched_rule)),
                    detected_domain,
                    matched_subject_id: matched_rule.subject_id,
                };
            }

            return WhitelistMatchResult {
                allowed: false,
                reason: format!("浏览器网站 {domain} 不在白名单"),
                matched_process_name: None,
                detected_domain,
                matched_subject_id: None,
            };
        }

        return WhitelistMatchResult {
            allowed: false,
            reason: "无法识别浏览器当前网址".to_string(),
            matched_process_name: None,
            detected_domain: None,
            matched_subject_id: None,
        };
    }

    if let Some(matched_rule) = whitelist_processes
        .iter()
        .find(|candidate| process_name == candidate.process_name.to_ascii_lowercase())
    {
        return WhitelistMatchResult {
            allowed: true,
            reason: "命中软件白名单".to_string(),
            matched_process_name: Some(matched_rule.process_name.clone()),
            detected_domain: None,
            matched_subject_id: matched_rule.subject_id,
        };
    }

    let browser_url = detect_browser_url_parts(app);
    let detected_domain = browser_url
        .as_ref()
        .map(|parts| parts.domain.clone())
        .or_else(|| extract_domain_from_text(&app.window_title));
    if let Some(domain) = detected_domain.as_deref() {
        if let Some(matched_rule) = whitelist_websites
            .iter()
            .find(|rule| website_rule_matches(domain, browser_url.as_ref(), rule))
        {
            return WhitelistMatchResult {
                allowed: true,
                reason: "命中网站白名单".to_string(),
                matched_process_name: Some(rule_label(matched_rule)),
                detected_domain,
                matched_subject_id: matched_rule.subject_id,
            };
        }

        return WhitelistMatchResult {
            allowed: false,
            reason: format!("浏览器网站 {domain} 不在白名单"),
            matched_process_name: None,
            detected_domain,
            matched_subject_id: None,
        };
    }

    WhitelistMatchResult {
        allowed: false,
        reason: "不在白名单".to_string(),
        matched_process_name: None,
        detected_domain: None,
        matched_subject_id: None,
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

fn detect_browser_url_parts(app: &ForegroundApp) -> Option<BrowserUrlParts> {
    let process_name = app.process_name.to_ascii_lowercase();
    if !is_supported_browser(&process_name) {
        return None;
    }

    app.browser_url.as_deref().and_then(parse_browser_url)
}

fn extract_domain_from_text(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let separators: &[char] = &[
        ' ', '\t', '\r', '\n', '/', '\\', '|', '—', '-', '(', ')', '[', ']', '<', '>', '"', '\'',
    ];

    lower
        .split(separators)
        .filter_map(|token| {
            let trimmed = token
                .trim_matches(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '.' && character != ':'
                })
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_start_matches("www.");
            normalize_domain_candidate(trimmed)
        })
        .next()
}

fn parse_browser_url(value: &str) -> Option<BrowserUrlParts> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let scheme_length = if lower.starts_with("http://") {
        "http://".len()
    } else if lower.starts_with("https://") {
        "https://".len()
    } else {
        0
    };
    let without_scheme = &trimmed[scheme_length..];
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    let domain = normalize_domain_candidate(host)?;
    let path_start = without_scheme.find(['/', '?', '#']);
    let raw_path = path_start
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    let path = raw_path
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_end_matches('/');

    Some(BrowserUrlParts {
        domain,
        path: if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        },
    })
}

fn website_rule_matches(
    detected_domain: &str,
    browser_url: Option<&BrowserUrlParts>,
    rule: &WebsiteWhitelistRule,
) -> bool {
    let allowed_domain = normalize_domain(&rule.domain);
    if !domain_matches(detected_domain, &allowed_domain) {
        return false;
    }

    let Some(rule_url) = rule.launch_url.as_deref().and_then(parse_browser_url) else {
        return true;
    };

    if !is_specific_url_path(&rule_url.path) {
        return true;
    }

    let Some(browser_url) = browser_url else {
        return false;
    };

    domain_matches(&browser_url.domain, &allowed_domain) && browser_url.path == rule_url.path
}

fn is_specific_url_path(path: &str) -> bool {
    let normalized = path.trim().trim_end_matches('/');
    !normalized.is_empty() && normalized != "/"
}

fn rule_label(rule: &WebsiteWhitelistRule) -> String {
    rule.launch_url
        .as_ref()
        .filter(|url| parse_browser_url(url).is_some_and(|parts| is_specific_url_path(&parts.path)))
        .cloned()
        .unwrap_or_else(|| normalize_domain(&rule.domain))
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
        .trim_start_matches("//")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .trim_start_matches("*.")
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn domain_matches(detected_domain: &str, allowed_domain: &str) -> bool {
    detected_domain == allowed_domain
        || detected_domain
            .strip_suffix(allowed_domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}
