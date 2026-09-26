//! 命令 1–3：读取 / 保存 AI 排期设置，以及连通性测试。
//!
//! 密钥走 `credential`（Windows DPAPI），**永不回传前端**；读取时只回
//! `api_key_configured: bool`（对齐 `email.rs` 的 `password_configured`）。

use super::client;
use super::models::*;
use crate::{credential, storage::db::open_database};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

/// 非敏感配置以单个 JSON 存放，避免散落十几个 settings 键。
const SETTINGS_KEY: &str = "ai_scheduler_settings";
/// 密钥单独存放并加密；不出现在上面的 JSON 里。
const API_KEY_KEY: &str = "ai_scheduler_api_key";

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn db_error(error: impl std::fmt::Display) -> AiSchedulerError {
    AiSchedulerError::new(ERR_DB_ERROR, format!("本地数据读写失败：{error}"), false)
}

fn get_setting(connection: &Connection, key: &str) -> Result<Option<String>, AiSchedulerError> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), AiSchedulerError> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![key, value, updated_at],
        )
        .map_err(db_error)?;
    Ok(())
}

/// 读取配置。解析失败时回落为默认值，不阻断设置页打开。
fn read_settings(connection: &Connection) -> Result<AiSchedulerSettings, AiSchedulerError> {
    let raw = get_setting(connection, SETTINGS_KEY)?.unwrap_or_default();
    let mut settings: AiSchedulerSettings = if raw.trim().is_empty() {
        AiSchedulerSettings::default()
    } else {
        serde_json::from_str(&raw).unwrap_or_default()
    };
    settings.api_key = String::new();
    settings.api_key_configured = credential::secret_configured(connection, API_KEY_KEY)
        .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false))?;
    Ok(settings)
}

/// 供同模块的排期 / apply 链路复用。密钥字段在这一层已被清空，调用方拿不到明文。
pub(super) fn current_settings(
    connection: &Connection,
) -> Result<AiSchedulerSettings, AiSchedulerError> {
    read_settings(connection)
}

/// 从 DPAPI 取出 API Key 明文，**仅在发起模型请求的瞬间使用**。
///
/// 与 `current_settings` 的分工：配置结构里永远没有明文（`read_settings` 已清空），
/// 明文只在「马上要发请求」这一处按需解出，用完即弃，不进日志、不进快照。
pub(super) fn load_api_key(connection: &Connection) -> Result<String, AiSchedulerError> {
    let api_key = credential::get_secret(connection, API_KEY_KEY)
        .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false))?;
    if api_key.trim().is_empty() {
        return Err(AiSchedulerError::new(
            ERR_MISSING_API_KEY,
            "请先在 设置 → 集成 → AI 排期 中填写 API Key",
            false,
        ));
    }
    Ok(api_key)
}

fn align_to_five(minutes: i64) -> i64 {
    (minutes / 5) * 5
}

/// 规范化配置：钳制数值区间、丢弃非法时段、补齐预设默认值。
fn normalize_settings(
    mut settings: AiSchedulerSettings,
) -> Result<AiSchedulerSettings, AiSchedulerError> {
    let preset_key = settings.provider_preset.trim().to_lowercase();
    let preset = provider_preset(&preset_key);
    settings.provider_preset = preset
        .map(|preset| preset.key.to_string())
        .unwrap_or_else(|| DEFAULT_PROVIDER_PRESET.to_string());

    settings.base_url = settings.base_url.trim().to_string();
    if settings.base_url.is_empty() {
        if let Some(preset) = preset {
            settings.base_url = preset.base_url.to_string();
        }
    }
    // 地址非空时才校验；允许先保存半成品，连通性测试与排期时会再拦截。
    if !settings.base_url.is_empty() {
        settings.base_url = client::normalize_base_url(&settings.base_url)?.clone();
    }

    settings.model = settings.model.trim().to_string();
    if settings.model.is_empty() {
        if let Some(preset) = preset {
            settings.model = preset.model.to_string();
        }
    }

    settings.structured_output_mode = match settings.structured_output_mode.trim() {
        "json_schema" => "json_schema".to_string(),
        "json_object" => "json_object".to_string(),
        _ => DEFAULT_STRUCTURED_OUTPUT_MODE.to_string(),
    };

    settings.timeout_seconds = settings
        .timeout_seconds
        .clamp(MIN_TIMEOUT_SECONDS, MAX_TIMEOUT_SECONDS);
    settings.max_retries = settings.max_retries.clamp(0, MAX_RETRIES_CEILING);
    settings.max_tokens = settings
        .max_tokens
        .clamp(MIN_MAX_TOKENS, MAX_TOKENS_CEILING);
    settings.temperature = if settings.temperature.is_finite() {
        settings.temperature.clamp(0.0, 2.0)
    } else {
        DEFAULT_TEMPERATURE
    };
    settings.default_block_minutes =
        align_to_five(settings.default_block_minutes.clamp(5, 480)).max(5);
    settings.min_break_minutes = settings.min_break_minutes.clamp(0, 120);
    settings.max_daily_minutes = settings.max_daily_minutes.clamp(30, 1440);
    settings.available_windows = normalize_windows(settings.available_windows);
    if settings.available_windows.is_empty() {
        settings.available_windows = AiSchedulerSettings::default().available_windows;
    }
    settings.peak_windows = normalize_windows(settings.peak_windows);

    // 密钥不落在这个 JSON 里，避免任何形式的明文持久化。
    settings.api_key = String::new();
    Ok(settings)
}

fn normalize_windows(windows: Vec<AiTimeWindow>) -> Vec<AiTimeWindow> {
    let mut cleaned: Vec<AiTimeWindow> = windows
        .into_iter()
        .filter_map(|window| {
            if !(1..=7).contains(&window.weekday) {
                return None;
            }
            let start = align_to_five(window.start_minute.clamp(0, 1440));
            let end = align_to_five(window.end_minute.clamp(0, 1440));
            if start >= end {
                return None;
            }
            Some(AiTimeWindow {
                weekday: window.weekday,
                start_minute: start,
                end_minute: end,
            })
        })
        .collect();
    cleaned.sort_by_key(|window| (window.weekday, window.start_minute, window.end_minute));
    cleaned.dedup();
    cleaned
}

/// 落盘非敏感配置。
///
/// `pub(super)`：`apply` 链路与测试需要把「当前生效的可用时段」真正写进 settings，
/// 因为 `apply_proposal` 重读的是库里的当前配置，而不是调用方手里那份。
pub(super) fn persist_settings(
    connection: &Connection,
    settings: &AiSchedulerSettings,
    now: &str,
) -> Result<(), AiSchedulerError> {
    let serialized = serde_json::to_string(settings).map_err(|error| {
        AiSchedulerError::new(ERR_DB_ERROR, format!("配置序列化失败：{error}"), false)
    })?;
    set_setting(connection, SETTINGS_KEY, &serialized, now)
}

/// 落地 API Key，并返回落盘后的「是否已配置」。
///
/// **调用方必须在 `normalize_settings` 之前取出明文**：`normalize_settings` 会把
/// `api_key` 清空以杜绝明文持久化，若在其后再读就只能拿到空串，密钥将永不生效。
///
/// 空串表示「不修改」——`credential::set_secret_if_changed` 在空值时只负责把历史
/// 明文升级为 DPAPI 密文，不会覆盖已保存的密钥。因此留空保存是安全的。
fn persist_api_key(
    connection: &Connection,
    raw_api_key: &str,
    now: &str,
) -> Result<bool, AiSchedulerError> {
    let credential_error = |error: String| AiSchedulerError::new(ERR_DB_ERROR, error, false);

    if raw_api_key.is_empty() {
        credential::set_secret_if_changed(connection, API_KEY_KEY, "", now)
            .map_err(credential_error)?;
    } else {
        credential::set_secret(connection, API_KEY_KEY, raw_api_key, now)
            .map_err(credential_error)?;
    }

    credential::secret_configured(connection, API_KEY_KEY).map_err(credential_error)
}

#[tauri::command]
pub fn get_ai_scheduler_settings(app: AppHandle) -> Result<AiSchedulerSettings, String> {
    let connection =
        open_database(&database_path(&app)?).map_err(|error| db_error(error).to_envelope())?;
    read_settings(&connection).map_err(|error| error.to_envelope())
}

#[tauri::command]
pub fn save_ai_scheduler_settings(
    app: AppHandle,
    settings: AiSchedulerSettings,
) -> Result<AiSchedulerSettings, String> {
    let connection =
        open_database(&database_path(&app)?).map_err(|error| db_error(error).to_envelope())?;

    // 密钥必须在 `normalize_settings` **之前**取出。normalize 会清空 `api_key`
    // 以杜绝明文落盘，若在其之后读取，密钥会静默变成空串（表现为「填了密钥仍提示未配置」）。
    let raw_api_key = settings.api_key.trim().to_string();

    let normalized = normalize_settings(settings).map_err(|error| error.to_envelope())?;
    let now = Utc::now().to_rfc3339();

    persist_api_key(&connection, &raw_api_key, &now).map_err(|error| error.to_envelope())?;
    persist_settings(&connection, &normalized, &now).map_err(|error| error.to_envelope())?;

    let mut response = normalized;
    response.api_key = String::new();
    response.api_key_configured = credential::secret_configured(&connection, API_KEY_KEY)
        .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false).to_envelope())?;
    Ok(response)
}

/// 连通性测试。S2 阶段的硬闸门：验证可用 + 探测结构化输出档位 + 拉取模型列表。
///
/// 网络命令必须 `async fn` + `spawn_blocking`，否则会在主线程执行并冻结界面。
#[tauri::command]
pub async fn test_ai_scheduler_connection(
    app: AppHandle,
) -> Result<AiConnectionTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_connection_test(app))
        .await
        .map_err(|error| {
            AiSchedulerError::new(
                ERR_NETWORK,
                format!("连通性测试后台任务失败：{error}"),
                true,
            )
            .to_envelope()
        })?
        .map_err(|error: AiSchedulerError| error.to_envelope())
}

fn run_connection_test(app: AppHandle) -> Result<AiConnectionTestResult, AiSchedulerError> {
    let connection = open_database(&database_path(&app).map_err(db_error)?).map_err(db_error)?;
    let mut settings = read_settings(&connection)?;
    let now = Utc::now().to_rfc3339();

    if settings.base_url.is_empty() {
        return Err(AiSchedulerError::new(
            ERR_BAD_REQUEST,
            "请先在 设置 → 集成 → AI 排期 中填写接口地址（Base URL）",
            false,
        ));
    }
    if settings.model.is_empty() {
        return Err(AiSchedulerError::new(
            ERR_BAD_REQUEST,
            "请先在 设置 → 集成 → AI 排期 中选择或填写模型名",
            false,
        ));
    }
    let base_url = client::normalize_base_url(&settings.base_url)?;

    let api_key = credential::get_secret(&connection, API_KEY_KEY)
        .map_err(|error| AiSchedulerError::new(ERR_DB_ERROR, error, false))?;
    if api_key.trim().is_empty() {
        return Err(AiSchedulerError::new(
            ERR_MISSING_API_KEY,
            "请先在 设置 → 集成 → AI 排期 中填写 API Key",
            false,
        ));
    }

    let http = client::build_client(settings.timeout_seconds)?;

    // 1. 拉取模型列表（网关未实现 /models 时返回空列表，不算失败）。
    let available_models = client::list_models(&http, &base_url, &api_key)?;

    // 2. 验证 key / base_url / model 可用，并顺带探测结构化输出档位。
    let (mode, latency_ms, _content) = client::probe_structured_output_mode(
        &http,
        &base_url,
        &api_key,
        &settings.model,
        &settings.structured_output_mode,
        settings.disable_thinking,
        settings.temperature,
    )?;

    // 3. 回写缓存，正式排期时不再浪费一次失败的往返。
    let mut persisted = settings.clone();
    persisted.structured_output_mode = mode.clone();
    persisted.available_models = available_models.clone();
    settings.structured_output_mode = mode.clone();
    settings.available_models = available_models.clone();
    persist_settings(&connection, &persisted_redacted(&persisted), &now)?;

    let model_note = if available_models
        .iter()
        .any(|entry| entry == &settings.model)
    {
        String::new()
    } else if available_models.is_empty() {
        "（该网关未提供模型列表，无法核对模型名）".to_string()
    } else {
        format!(
            "（注意：账号可见模型列表中没有 {}，请确认模型名）",
            settings.model
        )
    };

    Ok(AiConnectionTestResult {
        ok: true,
        model: settings.model.clone(),
        latency_ms,
        message: format!(
            "连接正常，模型 {} 可用，往返 {} ms{}",
            settings.model, latency_ms, model_note
        ),
        structured_output_mode: mode,
        available_models,
    })
}

/// 持久化前把密钥字段清空，保证任何路径都不会把它写进 JSON。
fn persisted_redacted(settings: &AiSchedulerSettings) -> AiSchedulerSettings {
    let mut redacted = settings.clone();
    redacted.api_key = String::new();
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open in-memory sqlite");
        connection
            .execute(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL)",
                [],
            )
            .expect("create settings table");
        connection
    }

    #[test]
    fn read_settings_falls_back_to_default_when_missing_or_broken() {
        let connection = memory_connection();
        let settings = read_settings(&connection).expect("read default settings");
        assert_eq!(settings.provider_preset, DEFAULT_PROVIDER_PRESET);
        assert!(!settings.api_key_configured);
        assert!(settings.api_key.is_empty());

        set_setting(&connection, SETTINGS_KEY, "{ not json", "now").expect("write broken json");
        let recovered = read_settings(&connection).expect("read after broken json");
        assert_eq!(recovered.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn read_settings_never_returns_plaintext_api_key() {
        let connection = memory_connection();
        let raw = r#"{"enabled":true,"base_url":"https://api.deepseek.com","model":"deepseek-v4-flash","api_key":"sk-should-never-leak","api_key_configured":true}"#;
        set_setting(&connection, SETTINGS_KEY, raw, "now").expect("write settings");
        let settings = read_settings(&connection).expect("read settings");
        assert!(
            settings.api_key.is_empty(),
            "api_key must never be echoed back"
        );
    }

    #[test]
    fn read_settings_tolerates_legacy_json_without_new_fields() {
        let connection = memory_connection();
        // 只有 enabled 字段的旧配置也必须能读出来，其余走默认值。
        set_setting(&connection, SETTINGS_KEY, r#"{"enabled":true}"#, "now").expect("write legacy");
        let settings = read_settings(&connection).expect("read legacy settings");
        assert!(settings.enabled);
        assert_eq!(settings.model, DEFAULT_MODEL);
        assert_eq!(settings.max_tokens, DEFAULT_MAX_TOKENS);
        assert_eq!(settings.available_windows.len(), 7);
    }

    #[test]
    fn normalize_clamps_numeric_ranges() {
        let raw = AiSchedulerSettings {
            timeout_seconds: 9999,
            max_retries: 99,
            max_tokens: 999_999,
            temperature: 7.5,
            default_block_minutes: 37,
            min_break_minutes: -5,
            max_daily_minutes: 5,
            ..AiSchedulerSettings::default()
        };

        let settings = normalize_settings(raw).expect("normalize settings");
        assert_eq!(settings.timeout_seconds, MAX_TIMEOUT_SECONDS);
        assert_eq!(settings.max_retries, MAX_RETRIES_CEILING);
        assert_eq!(settings.max_tokens, MAX_TOKENS_CEILING);
        assert_eq!(settings.temperature, 2.0);
        assert_eq!(settings.default_block_minutes, 35, "应向下对齐到 5 分钟");
        assert_eq!(settings.min_break_minutes, 0);
        assert_eq!(settings.max_daily_minutes, 30);
    }

    #[test]
    fn normalize_adopts_preset_defaults_for_empty_fields() {
        let raw = AiSchedulerSettings {
            provider_preset: "openai".to_string(),
            base_url: String::new(),
            model: String::new(),
            ..AiSchedulerSettings::default()
        };

        let settings = normalize_settings(raw).expect("normalize settings");
        assert_eq!(settings.base_url, "https://api.openai.com/v1");
        assert_eq!(settings.model, "gpt-4o-mini");
    }

    #[test]
    fn normalize_rejects_insecure_public_http_base_url() {
        let raw = AiSchedulerSettings {
            base_url: "http://evil.example.com/v1".to_string(),
            ..AiSchedulerSettings::default()
        };
        let error = normalize_settings(raw).unwrap_err();
        assert_eq!(error.code, ERR_BAD_REQUEST);
    }

    #[test]
    fn normalize_drops_invalid_windows_and_keeps_valid_ones() {
        let raw = AiSchedulerSettings {
            available_windows: vec![
                AiTimeWindow {
                    weekday: 0,
                    start_minute: 480,
                    end_minute: 600,
                },
                AiTimeWindow {
                    weekday: 1,
                    start_minute: 600,
                    end_minute: 480,
                },
                AiTimeWindow {
                    weekday: 2,
                    start_minute: 482,
                    end_minute: 601,
                },
                AiTimeWindow {
                    weekday: 2,
                    start_minute: 482,
                    end_minute: 601,
                },
            ],
            ..AiSchedulerSettings::default()
        };

        let settings = normalize_settings(raw).expect("normalize settings");
        assert_eq!(
            settings.available_windows.len(),
            1,
            "非法与重复时段应被丢弃"
        );
        assert_eq!(settings.available_windows[0].weekday, 2);
        assert_eq!(settings.available_windows[0].start_minute, 480);
        assert_eq!(settings.available_windows[0].end_minute, 600);
    }

    #[test]
    fn normalize_restores_default_windows_when_all_invalid() {
        let raw = AiSchedulerSettings {
            available_windows: vec![AiTimeWindow {
                weekday: 9,
                start_minute: 0,
                end_minute: 10,
            }],
            ..AiSchedulerSettings::default()
        };
        let settings = normalize_settings(raw).expect("normalize settings");
        assert_eq!(settings.available_windows.len(), 7);
    }

    #[test]
    fn normalize_always_clears_api_key_before_persisting() {
        let raw = AiSchedulerSettings {
            api_key: "sk-secret".to_string(),
            ..AiSchedulerSettings::default()
        };
        let settings = normalize_settings(raw).expect("normalize settings");
        assert!(settings.api_key.is_empty());
    }

    #[test]
    fn persist_writes_json_without_api_key() {
        let connection = memory_connection();
        let settings = AiSchedulerSettings {
            api_key: "sk-secret".to_string(),
            ..AiSchedulerSettings::default()
        };
        persist_settings(&connection, &persisted_redacted(&settings), "now").expect("persist");

        let stored = get_setting(&connection, SETTINGS_KEY)
            .expect("read raw")
            .unwrap();
        assert!(!stored.contains("sk-secret"), "明文密钥不得落盘：{stored}");
        assert!(stored.contains(DEFAULT_MODEL));
    }

    fn raw_api_key_row(connection: &Connection) -> Option<String> {
        connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![API_KEY_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .expect("read api key row")
    }

    /// 回归：填入密钥后必须真的写进凭据存储，并且是 DPAPI 密文而非明文。
    ///
    /// 此前的缺陷是保存路径从 `normalized.api_key` 读取密钥，而 `normalize_settings`
    /// 已把它清空，于是 `set_secret` 永远收到空串 —— 用户表现为「填了密钥仍提示未配置」。
    #[test]
    fn save_persists_typed_api_key_and_reports_configured() {
        let connection = memory_connection();

        let configured = persist_api_key(&connection, "sk-typed-by-user", "2026-09-25T00:00:00Z")
            .expect("persist");

        assert!(configured, "写入密钥后必须回显「已配置」");
        assert_eq!(
            credential::get_secret(&connection, API_KEY_KEY).expect("read back"),
            "sk-typed-by-user",
            "密钥必须可原样读回（DPAPI 解密后一致）"
        );

        let stored = raw_api_key_row(&connection).expect("api key row exists");
        assert!(
            stored.starts_with("dpapi:v1:"),
            "落盘必须是 DPAPI 密文，实际为：{stored}"
        );
        assert!(!stored.contains("sk-typed-by-user"), "明文不得落盘");
    }

    /// 回归：留空保存等价于「不修改」，不能把已保存的密钥清空。
    #[test]
    fn blank_api_key_keeps_existing_secret() {
        let connection = memory_connection();
        persist_api_key(&connection, "sk-first", "t1").expect("persist first");

        let configured = persist_api_key(&connection, "", "t2").expect("blank means keep");

        assert!(configured, "留空保存后仍应是已配置");
        assert_eq!(
            credential::get_secret(&connection, API_KEY_KEY).expect("read back"),
            "sk-first"
        );
    }

    /// 回归：`normalize_settings` 会清空 `api_key`，取值顺序错位即导致密钥永不落盘。
    /// 这个测试把「必须先取明文、再 normalize」这一约束固化下来。
    #[test]
    fn api_key_must_be_read_before_normalize_clears_it() {
        let connection = memory_connection();
        let input = AiSchedulerSettings {
            api_key: "sk-order-matters".to_string(),
            ..AiSchedulerSettings::default()
        };

        // normalize 之后字段必然为空 —— 这正是不能从它取密钥的原因。
        let normalized = normalize_settings(input.clone()).expect("normalize");
        assert!(normalized.api_key.is_empty());
        assert!(
            !persist_api_key(&connection, normalized.api_key.trim(), "now").expect("persist"),
            "从 normalize 结果取密钥会得到空串，只会走「不修改」分支"
        );

        // 正确顺序：先取明文再 normalize。
        let connection = memory_connection();
        let raw_api_key = input.api_key.trim().to_string();
        let _ = normalize_settings(input).expect("normalize");
        assert!(
            persist_api_key(&connection, &raw_api_key, "now").expect("persist"),
            "先取明文的顺序才能让密钥生效"
        );
        assert_eq!(
            credential::get_secret(&connection, API_KEY_KEY).expect("read back"),
            "sk-order-matters"
        );
    }

    #[test]
    fn preset_backfill_prevents_silent_empty_base_url() {
        let raw = AiSchedulerSettings {
            base_url: String::new(),
            ..AiSchedulerSettings::default()
        };
        let normalized = normalize_settings(raw).expect("preset fills base_url back");
        assert!(
            !normalized.base_url.is_empty(),
            "空地址会被预设补回，而不是静默失败"
        );
        assert!(!normalized.model.is_empty());
    }
}
