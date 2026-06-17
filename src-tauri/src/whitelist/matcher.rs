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
    pub potplayer_media_path: Option<String>,
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
pub struct PotPlayerWhitelistRule {
    pub process_name: String,
    pub media_path: String,
    pub match_type: String,
    pub subject_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct BrowserUrlParts {
    domain: String,
    path: String,
    /// 原始完整网址（含 scheme 与 query），用于「包含模式」子串匹配。
    full: String,
}

pub fn is_foreground_app_allowed(
    app: &ForegroundApp,
    whitelist_processes: &[ProcessWhitelistRule],
    whitelist_websites: &[WebsiteWhitelistRule],
    whitelist_potplayer_media: &[PotPlayerWhitelistRule],
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
            potplayer_media_path: app.potplayer_media_path.clone(),
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
                    potplayer_media_path: None,
                };
            }

            return WhitelistMatchResult {
                allowed: false,
                reason: format!("浏览器网站 {domain} 不在白名单"),
                matched_process_name: None,
                detected_domain,
                matched_subject_id: None,
                potplayer_media_path: None,
            };
        }

        return WhitelistMatchResult {
            allowed: false,
            reason: "无法识别浏览器当前网址".to_string(),
            matched_process_name: None,
            detected_domain: None,
            matched_subject_id: None,
            potplayer_media_path: None,
        };
    }

    if is_supported_potplayer(&process_name) {
        return match_potplayer_media(app, whitelist_potplayer_media);
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
            potplayer_media_path: app.potplayer_media_path.clone(),
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
                potplayer_media_path: None,
            };
        }

        return WhitelistMatchResult {
            allowed: false,
            reason: format!("浏览器网站 {domain} 不在白名单"),
            matched_process_name: None,
            detected_domain,
            matched_subject_id: None,
            potplayer_media_path: None,
        };
    }

    WhitelistMatchResult {
        allowed: false,
        reason: "不在白名单".to_string(),
        matched_process_name: None,
        detected_domain: None,
        matched_subject_id: None,
        potplayer_media_path: app.potplayer_media_path.clone(),
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

fn is_supported_potplayer(process_name: &str) -> bool {
    matches!(process_name, "potplayermini64.exe" | "potplayermini.exe")
}

fn match_potplayer_media(
    app: &ForegroundApp,
    whitelist_potplayer_media: &[PotPlayerWhitelistRule],
) -> WhitelistMatchResult {
    let Some(media_path) = app.potplayer_media_path.as_deref() else {
        return WhitelistMatchResult {
            allowed: false,
            reason: "PotPlayer 未识别到当前播放视频，默认拦截".to_string(),
            matched_process_name: None,
            detected_domain: None,
            matched_subject_id: None,
            potplayer_media_path: None,
        };
    };

    if let Some(matched_rule) = whitelist_potplayer_media.iter().find(|rule| {
        rule.process_name.eq_ignore_ascii_case(&app.process_name)
            && potplayer_rule_matches(media_path, rule)
    }) {
        return WhitelistMatchResult {
            allowed: true,
            reason: "命中 PotPlayer 视频白名单".to_string(),
            matched_process_name: Some(matched_rule.media_path.clone()),
            detected_domain: None,
            matched_subject_id: matched_rule.subject_id,
            potplayer_media_path: Some(media_path.to_string()),
        };
    }

    WhitelistMatchResult {
        allowed: false,
        reason: format!("PotPlayer 当前视频不在白名单：{media_path}"),
        matched_process_name: None,
        detected_domain: None,
        matched_subject_id: None,
        potplayer_media_path: Some(media_path.to_string()),
    }
}

fn potplayer_rule_matches(media_path: &str, rule: &PotPlayerWhitelistRule) -> bool {
    match rule.match_type.as_str() {
        "potplayer_video_file" => paths_equal_ignore_ascii_case(media_path, &rule.media_path),
        "potplayer_video_directory" => path_is_within_directory(media_path, &rule.media_path),
        _ => false,
    }
}

fn paths_equal_ignore_ascii_case(left: &str, right: &str) -> bool {
    normalize_file_path(left).eq_ignore_ascii_case(&normalize_file_path(right))
}

fn path_is_within_directory(media_path: &str, directory_path: &str) -> bool {
    let media = normalize_file_path(media_path).to_ascii_lowercase();
    let directory = normalize_file_path(directory_path).to_ascii_lowercase();

    if media == directory {
        return false;
    }

    media
        .strip_prefix(&directory)
        .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn normalize_file_path(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches(['\\', '/'])
        .to_string()
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
        ' ', '\t', '\r', '\n', '/', '\\', '|', '–', '-', '(', ')', '[', ']', '<', '>', '"', '\'',
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
        full: trimmed.to_string(),
    })
}

fn website_rule_matches(
    detected_domain: &str,
    browser_url: Option<&BrowserUrlParts>,
    rule: &WebsiteWhitelistRule,
) -> bool {
    // 「包含模式」：当规则配置了具体网址/关键词时，要求当前网址同时包含全部片段。
    let patterns = rule_url_patterns(rule.launch_url.as_deref());
    if !patterns.is_empty() {
        // 用户明确要求按片段过滤；读不到完整网址时一律拦截，避免被整站放行绕过。
        let Some(browser_url) = browser_url else {
            return false;
        };
        // 地址栏常隐藏 scheme（如 chrome 显示 www.bilibili.com/...），
        // 因此对网址与片段都做归一化（去掉 http(s):// 与 www.）后再做子串匹配。
        let haystack = normalize_for_contains(&browser_url.full);
        return patterns
            .iter()
            .all(|pattern| haystack.contains(&normalize_for_contains(pattern)));
    }

    // 未配置具体片段时，沿用原有的纯域名匹配。
    domain_matches(detected_domain, &normalize_domain(&rule.domain))
}

/// 把网址 / 片段归一化为便于子串比较的形式：转小写、去掉 scheme 和 `www.`。
/// 这样 `https://www.bilibili.com/video` 与地址栏里的 `www.bilibili.com/video` 等价。
fn normalize_for_contains(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme)
        .to_string()
}

/// 把规则里存储的网址拆成需要「同时命中」的子串片段：
/// 以空白（空格 / 换行 / 制表符）分隔，纯域名（无路径、无 query）不视为片段。
fn rule_url_patterns(launch_url: Option<&str>) -> Vec<String> {
    let Some(launch_url) = launch_url else {
        return Vec::new();
    };

    launch_url
        .split_whitespace()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .filter(|token| pattern_is_specific(token))
        .map(|token| token.to_string())
        .collect()
}

/// 判断单个片段是否「足够具体」——带路径 / query / 等号关键词才算，纯 `bilibili.com` 不算。
fn pattern_is_specific(token: &str) -> bool {
    if token.contains('=') {
        return true;
    }
    parse_browser_url(token).is_some_and(|parts| is_specific_url_path(&parts.path))
}

fn is_specific_url_path(path: &str) -> bool {
    let normalized = path.trim().trim_end_matches('/');
    !normalized.is_empty() && normalized != "/"
}

fn rule_label(rule: &WebsiteWhitelistRule) -> String {
    rule.launch_url
        .as_ref()
        .filter(|url| !rule_url_patterns(Some(url)).is_empty())
        .map(|url| url.split_whitespace().collect::<Vec<_>>().join(" "))
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

#[cfg(test)]
mod tests {
    use super::{
        is_foreground_app_allowed, path_is_within_directory, PotPlayerWhitelistRule,
        ProcessWhitelistRule, WebsiteWhitelistRule,
    };
    use crate::windows::foreground::ForegroundApp;

    fn foreground_app(process_name: &str, media_path: Option<&str>) -> ForegroundApp {
        ForegroundApp::for_test(
            1,
            process_name.to_string(),
            Some(format!(r"D:\Apps\{process_name}")),
            "PotPlayer".to_string(),
            None,
            media_path.map(|value| value.to_string()),
        )
    }

    #[test]
    fn file_rule_matches_case_insensitively() {
        let app = foreground_app("PotPlayerMini64.exe", Some(r"d:\videos\course\lesson1.mkv"));
        let result = is_foreground_app_allowed(
            &app,
            &[],
            &[],
            &[PotPlayerWhitelistRule {
                process_name: "PotPlayerMini64.exe".to_string(),
                media_path: r"D:\Videos\Course\Lesson1.mkv".to_string(),
                match_type: "potplayer_video_file".to_string(),
                subject_id: Some(2),
            }],
        );

        assert!(result.allowed);
        assert_eq!(result.matched_subject_id, Some(2));
    }

    #[test]
    fn directory_rule_matches_child_file() {
        assert!(path_is_within_directory(
            r"D:\Videos\Course\Lesson1.mkv",
            r"D:\Videos\Course"
        ));
    }

    #[test]
    fn directory_rule_rejects_outside_file() {
        assert!(!path_is_within_directory(
            r"D:\Videos\Other\Lesson1.mkv",
            r"D:\Videos\Course"
        ));
    }

    #[test]
    fn unidentified_potplayer_media_is_blocked() {
        let app = foreground_app("PotPlayerMini64.exe", None);
        let result = is_foreground_app_allowed(&app, &[], &[], &[]);

        assert!(!result.allowed);
        assert!(result.reason.contains("未识别"));
        assert!(result.reason.contains("拦截"));
    }

    #[test]
    fn unidentified_potplayer_media_ignores_process_whitelist() {
        let app = foreground_app("PotPlayerMini64.exe", None);
        let result = is_foreground_app_allowed(
            &app,
            &[ProcessWhitelistRule {
                process_name: "PotPlayerMini64.exe".to_string(),
                subject_id: Some(1),
            }],
            &[],
            &[],
        );

        assert!(!result.allowed);
        assert_eq!(result.matched_subject_id, None);
    }

    #[test]
    fn potplayer_media_rules_override_process_whitelist() {
        let app = foreground_app("PotPlayerMini64.exe", Some(r"D:\Videos\Course\Lesson2.mkv"));
        let result = is_foreground_app_allowed(
            &app,
            &[ProcessWhitelistRule {
                process_name: "PotPlayerMini64.exe".to_string(),
                subject_id: None,
            }],
            &[],
            &[PotPlayerWhitelistRule {
                process_name: "PotPlayerMini64.exe".to_string(),
                media_path: r"D:\Videos\Course\Lesson1.mkv".to_string(),
                match_type: "potplayer_video_file".to_string(),
                subject_id: None,
            }],
        );

        assert!(!result.allowed);
    }

    #[test]
    fn browser_rules_still_work() {
        let app = ForegroundApp::for_test(
            2,
            "chrome.exe".to_string(),
            Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe".to_string()),
            "课程 - www.icourse163.org".to_string(),
            Some("https://www.icourse163.org/learn".to_string()),
            None,
        );
        let result = is_foreground_app_allowed(
            &app,
            &[],
            &[WebsiteWhitelistRule {
                domain: "icourse163.org".to_string(),
                launch_url: None,
                subject_id: Some(1),
            }],
            &[],
        );

        assert!(result.allowed);
        assert_eq!(result.matched_subject_id, Some(1));
    }

    #[test]
    fn pattern_rule_requires_all_fragments() {
        let rule = WebsiteWhitelistRule {
            domain: "bilibili.com".to_string(),
            launch_url: Some(
                "https://www.bilibili.com/video vd_source=cdb62f207df23b5659fe0577a320ce87"
                    .to_string(),
            ),
            subject_id: Some(7),
        };

        // 同时包含两个片段 → 放行。
        let hit = ForegroundApp::for_test(
            3,
            "chrome.exe".to_string(),
            None,
            "哔哩哔哩".to_string(),
            Some(
                "https://www.bilibili.com/video/BV1ufLu6UEEM/?vd_source=cdb62f207df23b5659fe0577a320ce87&spm_id_from=333.788"
                    .to_string(),
            ),
            None,
        );
        let result = is_foreground_app_allowed(&hit, &[], std::slice::from_ref(&rule), &[]);
        assert!(result.allowed);
        assert_eq!(result.matched_subject_id, Some(7));

        // 同域名但缺少 vd_source 片段 → 拦截。
        let miss = ForegroundApp::for_test(
            3,
            "chrome.exe".to_string(),
            None,
            "哔哩哔哩".to_string(),
            Some("https://www.bilibili.com/video/BV1other/?vd_source=somethingelse".to_string()),
            None,
        );
        let result = is_foreground_app_allowed(&miss, &[], std::slice::from_ref(&rule), &[]);
        assert!(!result.allowed);

        // 地址栏隐藏了 scheme（chrome 实际表现）→ 仍应正确命中。
        let no_scheme = ForegroundApp::for_test(
            3,
            "chrome.exe".to_string(),
            None,
            "哔哩哔哩".to_string(),
            Some(
                "www.bilibili.com/video/BV1ufLu6UEEM/?vd_source=cdb62f207df23b5659fe0577a320ce87"
                    .to_string(),
            ),
            None,
        );
        let result = is_foreground_app_allowed(&no_scheme, &[], &[rule], &[]);
        assert!(result.allowed);
    }
}
