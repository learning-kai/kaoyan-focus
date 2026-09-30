//! OpenAI 兼容的 `POST /chat/completions` 客户端。
//!
//! 只依赖三件事：`POST {base_url}/chat/completions`、`Authorization: Bearer <key>`、
//! 标准 `choices[0].message.content`。任何满足这三点的网关都能接入。

use super::models::*;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::{StatusCode, Url};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// 探测结构化输出档位时用的最小 schema。
const PROBE_SCHEMA_NAME: &str = "ai_scheduler_probe";

/// 构造 HTTP 客户端。总超时取用户配置，连接超时固定 10s（与 caldav 一致）。
pub fn build_client(timeout_seconds: i64) -> Result<Client, AiSchedulerError> {
    let timeout = timeout_seconds.clamp(MIN_TIMEOUT_SECONDS, MAX_TIMEOUT_SECONDS) as u64;
    Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| {
            AiSchedulerError::new(ERR_NETWORK, format!("无法初始化网络组件：{error}"), false)
        })
}

/// 规范化并校验 `base_url`，返回去掉尾部 `/` 的地址。
///
/// 强制 HTTPS；`http` 仅允许回环地址，避免密钥在明文链路上传输（见 §7）。
pub fn normalize_base_url(raw: &str) -> Result<String, AiSchedulerError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AiSchedulerError::new(
            ERR_BAD_REQUEST,
            "请先填写接口地址（Base URL）",
            false,
        ));
    }

    let url = Url::parse(trimmed).map_err(|error| {
        AiSchedulerError::new(
            ERR_BAD_REQUEST,
            format!("接口地址不是合法的 URL：{error}"),
            false,
        )
    })?;

    match url.scheme() {
        "https" => {}
        "http" => {
            let is_loopback = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
            );
            if !is_loopback {
                return Err(AiSchedulerError::new(
                    ERR_BAD_REQUEST,
                    "出于安全考虑，接口地址必须使用 https（仅本机地址允许 http）",
                    false,
                ));
            }
        }
        other => {
            return Err(AiSchedulerError::new(
                ERR_BAD_REQUEST,
                format!("不支持的协议 {other}，请使用 https"),
                false,
            ));
        }
    }

    Ok(trimmed.to_string())
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn bearer_request(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    api_key: &str,
) -> RequestBuilder {
    client
        .request(method, url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .header(reqwest::header::ACCEPT, "application/json")
}

/// 把 HTTP 状态码 + 响应体映射为结构化错误。
fn classify_status(status: StatusCode, body: &str, retry_after: Option<f64>) -> AiSchedulerError {
    let detail = extract_error_message(body);
    match status.as_u16() {
        401 => AiSchedulerError::new(ERR_UNAUTHORIZED, "API Key 无效或被拒绝，请重新填写", false),
        402 => AiSchedulerError::new(ERR_QUOTA_EXCEEDED, "账户额度不足，请检查账单与配额", false),
        403 => AiSchedulerError::new(
            ERR_FORBIDDEN,
            format!(
                "当前 Key 无权访问该模型，请更换模型{}",
                detail_suffix(&detail)
            ),
            false,
        ),
        404 => AiSchedulerError::new(
            ERR_BAD_REQUEST,
            format!(
                "接口地址或模型名不存在，请检查接口地址与模型名{}",
                detail_suffix(&detail)
            ),
            false,
        ),
        429 => {
            let mut error =
                AiSchedulerError::new(ERR_RATE_LIMITED, "请求过于频繁，稍后会自动重试", true);
            if let Some(seconds) = retry_after {
                error = error.with_retry_after(seconds);
            }
            error
        }
        400 => AiSchedulerError::new(
            ERR_BAD_REQUEST,
            format!(
                "请求被拒绝，请检查接口地址与模型名{}",
                detail_suffix(&detail)
            ),
            false,
        ),
        code if (500..600).contains(&code) => AiSchedulerError::new(
            ERR_SERVER_ERROR,
            format!("服务商异常（HTTP {code}），稍后重试"),
            true,
        ),
        code => AiSchedulerError::new(
            ERR_SERVER_ERROR,
            format!("服务商返回了未预期的状态码 HTTP {code}"),
            false,
        ),
    }
}

/// 400 且错误指向 `response_format` 时，说明该网关不支持当前结构化输出档位。
pub fn is_response_format_rejection(error: &AiSchedulerError) -> bool {
    error.code == ERR_BAD_REQUEST && error.message.contains("response_format")
}

fn detail_suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!("（{detail}）")
    }
}

/// 从各家格式不一的错误响应里尽量抠出一句可读描述。
fn extract_error_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return truncate_for_display(body);
    };

    if let Some(message) = value.get("error").and_then(|error| {
        error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
    }) {
        return truncate_for_display(message);
    }

    if let Some(message) = value.get("message").and_then(Value::as_str) {
        return truncate_for_display(message);
    }

    truncate_for_display(body)
}

fn truncate_for_display(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 200 {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(200).collect();
    format!("{truncated}…")
}

/// 区分 DNS / TLS / 连接超时 / 连接被拒，给出不同中文文案，帮用户判断该改哪一项。
pub fn classify_transport_error(error: &reqwest::Error) -> AiSchedulerError {
    if error.is_timeout() {
        return AiSchedulerError::new(ERR_TIMEOUT, "请求超时，正在重试或请检查网络与代理", true);
    }
    if error.is_connect() {
        let chain = error_chain_text(error);
        if chain.contains("dns")
            || chain.contains("lookup address")
            || chain.contains("Name or service not known")
        {
            return AiSchedulerError::new(
                ERR_NETWORK,
                "域名解析失败：接口地址可能写错，或本机 DNS 不可用",
                true,
            );
        }
        if chain.contains("certificate")
            || chain.contains("tls")
            || chain.contains("handshake")
            || chain.contains("SSL")
        {
            return AiSchedulerError::new(
                ERR_NETWORK,
                "TLS 握手失败：可能是代理拦截或证书问题，请检查代理设置",
                true,
            );
        }
        if chain.contains("refused") {
            return AiSchedulerError::new(
                ERR_NETWORK,
                "连接被拒绝：接口地址的端口不通，或需要先开启代理",
                true,
            );
        }
        return AiSchedulerError::new(ERR_NETWORK, "无法连接到服务商，请检查网络与接口地址", true);
    }
    if error.is_body() || error.is_decode() {
        return AiSchedulerError::new(ERR_INVALID_RESPONSE, "服务商返回的内容无法解析", false);
    }
    AiSchedulerError::new(ERR_NETWORK, format!("网络请求失败：{error}"), true)
}

fn error_chain_text(error: &reqwest::Error) -> String {
    let mut text = format!("{error:?}");
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(current) = source {
        text.push_str(" | ");
        text.push_str(&format!("{current:?}"));
        source = current.source();
    }
    text
}

fn parse_retry_after(response: &reqwest::blocking::Response) -> Option<f64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|seconds| *seconds >= 0.0)
}

/// `GET /models`：拉取账号可见模型列表。网关未实现该端点时返回空列表而非失败。
pub fn list_models(
    client: &Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, AiSchedulerError> {
    let url = endpoint(base_url, "models");
    let response = bearer_request(client, reqwest::Method::GET, &url, api_key)
        .send()
        .map_err(|error| classify_transport_error(&error))?;

    let status = response.status();
    let retry_after = parse_retry_after(&response);
    let body = response.text().unwrap_or_default();

    if status == StatusCode::NOT_FOUND {
        // 部分中转网关没有 /models，不视为失败。
        return Ok(Vec::new());
    }
    if !status.is_success() {
        return Err(classify_status(status, &body, retry_after));
    }

    let Ok(value) = serde_json::from_str::<Value>(&body) else {
        return Ok(Vec::new());
    };
    let mut models: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models.dedup();
    Ok(models)
}

fn response_format_for(mode: &str) -> Option<Value> {
    match mode {
        "json_schema" => Some(json!({
            "type": "json_schema",
            "json_schema": {
                "name": PROBE_SCHEMA_NAME,
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": { "ok": { "type": "boolean" } },
                    "required": ["ok"],
                    "additionalProperties": false
                }
            }
        })),
        "json_object" => Some(json!({ "type": "json_object" })),
        _ => None,
    }
}

const PLAN_SCHEMA_NAME: &str = "ai_schedule_plan";

/// 正式排期用的 `response_format`。
///
/// **不能复用 `response_format_for`**：那份是连通性探测用的 `{"ok": boolean}` strict schema。
/// 旧实现在 `json_schema` 档（OpenAI）下把它发给了真实排期请求，约束解码只会吐出
/// `{"ok": true}`，解析后条目为空——草案永远是空的。
///
/// strict 模式要求所有字段都在 `required` 里、对象都禁 `additionalProperties`，
/// 且不支持 `maxLength`，长度约束交给校验器。
fn plan_response_format(mode: &str) -> Option<Value> {
    match mode {
        "json_schema" => Some(json!({
            "type": "json_schema",
            "json_schema": {
                "name": PLAN_SCHEMA_NAME,
                "strict": true,
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["summary", "items", "unscheduled"],
                    "properties": {
                        "summary": { "type": "string" },
                        "items": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["item_id", "date", "start_minute", "end_minute", "rationale"],
                                "properties": {
                                    "item_id": { "type": "integer" },
                                    "date": { "type": "string" },
                                    "start_minute": { "type": "integer" },
                                    "end_minute": { "type": "integer" },
                                    "rationale": { "type": "string" }
                                }
                            }
                        },
                        "unscheduled": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["item_id", "reason"],
                                "properties": {
                                    "item_id": { "type": "integer" },
                                    "reason": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        })),
        other => response_format_for(other),
    }
}

fn probe_messages() -> Value {
    json!([
        {
            "role": "system",
            // 注意：`json_object` 档要求提示词里必须出现 "json" 字样，勿删。
            "content": "你是日程排期助手。只输出 json，不要任何解释文字。"
        },
        {
            "role": "user",
            "content": "请输出符合此结构的 json：{\"ok\": true}"
        }
    ])
}

/// 发一条最小请求验证 key / base_url / model 可用，返回往返延迟。
///
/// `mode` 为 `json_schema` 且被网关以 `response_format` 为由拒绝时，调用方应改成
/// `json_object` 后重试（见 `probe_structured_output_mode`）。
pub fn probe_chat(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    mode: &str,
    disable_thinking: bool,
    temperature: f64,
) -> Result<(i64, String), AiSchedulerError> {
    let url = endpoint(base_url, "chat/completions");
    let mut body = json!({
        "model": model,
        "messages": probe_messages(),
        "temperature": temperature,
        "max_tokens": 64,
        "stream": false,
    });
    if let Some(response_format) = response_format_for(mode) {
        body["response_format"] = response_format;
    }
    if disable_thinking {
        body["thinking"] = json!({ "type": "disabled" });
    }

    let started = Instant::now();
    let response = bearer_request(client, reqwest::Method::POST, &url, api_key)
        .json(&body)
        .send()
        .map_err(|error| classify_transport_error(&error))?;
    let latency_ms = started.elapsed().as_millis() as i64;

    let status = response.status();
    let retry_after = parse_retry_after(&response);
    let raw = response.text().unwrap_or_default();

    if !status.is_success() {
        let mut error = classify_status(status, &raw, retry_after);
        // 把原始 server 文案拼进去，前端才能识别 `response_format` 关键字。
        if error.code == ERR_BAD_REQUEST {
            let detail = extract_error_message(&raw);
            if detail.contains("response_format") {
                error.message = format!("请求被拒绝（{detail}）");
            }
        }
        return Err(error);
    }

    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Err(AiSchedulerError::new(
            ERR_INVALID_RESPONSE,
            "服务商返回的内容不是合法 JSON",
            false,
        ));
    };
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if content.trim().is_empty() {
        return Err(AiSchedulerError::new(
            ERR_INVALID_RESPONSE,
            "服务商返回了空内容，请稍后重试或更换模型",
            true,
        ));
    }

    Ok((latency_ms, content))
}

/// 探测结构化输出档位：先试 `json_schema`，被拒则退回 `json_object`。
///
/// 返回 `(最终档位, 延迟毫秒, 模型回显)`。
pub fn probe_structured_output_mode(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    preferred_mode: &str,
    disable_thinking: bool,
    temperature: f64,
) -> Result<(String, i64, String), AiSchedulerError> {
    let first_mode = match preferred_mode {
        "json_schema" => "json_schema",
        // `auto` 与 `json_object` 都从最宽松的可用档位开始试：先 schema，失败再退回 object。
        _ => "json_schema",
    };

    match probe_chat(
        client,
        base_url,
        api_key,
        model,
        first_mode,
        disable_thinking,
        temperature,
    ) {
        Ok((latency_ms, content)) => Ok((first_mode.to_string(), latency_ms, content)),
        Err(error) => {
            // 该服务商不支持 json_schema —— 静默降级为 json_object，不打扰用户。
            if is_response_format_rejection(&error) {
                let (latency_ms, content) = probe_chat(
                    client,
                    base_url,
                    api_key,
                    model,
                    "json_object",
                    disable_thinking,
                    temperature,
                )?;
                return Ok(("json_object".to_string(), latency_ms, content));
            }
            // 显式选了 json_object 却失败，或者网关不认 thinking 字段 —— 退掉思考开关再试一次。
            if disable_thinking && error.code == ERR_BAD_REQUEST {
                let retry_mode = if preferred_mode == "json_object" {
                    "json_object"
                } else {
                    "json_schema"
                };
                if let Ok((latency_ms, content)) = probe_chat(
                    client,
                    base_url,
                    api_key,
                    model,
                    retry_mode,
                    false,
                    temperature,
                ) {
                    return Ok((retry_mode.to_string(), latency_ms, content));
                }
            }
            Err(error)
        }
    }
}

/// 排期请求的模型返回，以及解析阶段产生的警告（截断 / 结构修补）。
pub struct ChatOutcome {
    pub response: RawPlanResponse,
    pub warnings: Vec<AiPlanWarning>,
}

/// 去掉模型自作主张包裹的 Markdown 代码围栏。
fn strip_code_fences(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    // 允许 ```json / ```JSON 之类的语言标注。
    let rest = rest.trim_start_matches(|ch: char| ch.is_ascii_alphanumeric());
    let rest = rest.trim_start_matches('\n');
    let rest = rest.strip_suffix("```").unwrap_or(rest).trim();
    rest
}

/// 字符串感知地把悬空的 `"key":` 与截断的结构补齐到可解析。
fn close_unclosed_json(text: &str) -> String {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    let mut repaired = text.to_string();
    // 截断发生在字符串中间时先补引号。
    if in_string {
        repaired.push('"');
    }
    while let Some(open) = stack.pop() {
        repaired.push(if open == '{' { '}' } else { ']' });
    }
    repaired
}

/// 去掉 `}]` / `]}` 前的悬挂逗号（截断补齐后最常见的残留）。
fn remove_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in chars.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                out.push(ch);
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            out.push(ch);
            continue;
        }
        if ch == ',' {
            let mut look = index + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && (chars[look] == '}' || chars[look] == ']') {
                continue;
            }
        }
        if ch == '"' {
            in_string = true;
        }
        out.push(ch);
    }
    out
}

/// 尽力把「有点坏但能救」的模型输出修回合法 JSON。救不回来就原样返回，由调用方报错。
fn repair_json_text(text: &str) -> String {
    remove_trailing_commas(&close_unclosed_json(strip_code_fences(text)))
}

fn backoff_duration(attempt: u32, retry_after_seconds: Option<f64>) -> Duration {
    if let Some(seconds) = retry_after_seconds {
        if seconds.is_finite() && seconds > 0.0 {
            return Duration::from_millis((seconds * 1000.0).ceil() as u64);
        }
    }
    // 0.6s → 1.2s → 2.4s …，上限 8s，避免把用户锁在等待里。
    Duration::from_millis(600u64.saturating_mul(1 << attempt.min(4))).min(Duration::from_secs(8))
}

/// 正式排期调用：带重试与结构化输出档位，返回解析后的草案候选。
///
/// 与 `probe_chat` 的差别：这里承载真实提示词、真实 `max_tokens`，并容忍模型输出
/// 「差一点能用」（截断 / 尾逗号）——能修补就修补并给 `schema_repaired` 警告，
/// 完全救不回来才算失败，让上层在重试预算内再试。
pub fn chat_json(
    http: &Client,
    base_url: &str,
    api_key: &str,
    settings: &AiSchedulerSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<ChatOutcome, AiSchedulerError> {
    let url = endpoint(base_url, "chat/completions");
    let attempts = settings.max_retries.clamp(0, MAX_RETRIES_CEILING) as u32 + 1;
    let response_format = plan_response_format(&settings.structured_output_mode);

    let mut last_error = AiSchedulerError::new(ERR_NETWORK, "未发起请求", true);
    for attempt in 0..attempts {
        if attempt > 0 {
            std::thread::sleep(backoff_duration(
                attempt - 1,
                last_error.retry_after_seconds,
            ));
        }

        let mut body = json!({
            "model": settings.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt },
            ],
            "temperature": settings.temperature,
            "max_tokens": settings.max_tokens,
            "stream": false,
        });
        if let Some(format) = response_format.clone() {
            body["response_format"] = format;
        }
        if settings.disable_thinking {
            body["thinking"] = json!({ "type": "disabled" });
        }

        let response = match bearer_request(http, reqwest::Method::POST, &url, api_key)
            .json(&body)
            .send()
        {
            Ok(response) => response,
            Err(error) => {
                let classified = classify_transport_error(&error);
                if classified.retryable && attempt + 1 < attempts {
                    last_error = classified;
                    continue;
                }
                return Err(classified);
            }
        };

        let status = response.status();
        let retry_after = parse_retry_after(&response);
        let raw = response.text().unwrap_or_default();
        if !status.is_success() {
            let classified = classify_status(status, &raw, retry_after);
            if classified.retryable && attempt + 1 < attempts {
                last_error = classified;
                continue;
            }
            return Err(classified);
        }

        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            last_error =
                AiSchedulerError::new(ERR_INVALID_RESPONSE, "服务商返回的内容不是合法 JSON", true);
            continue;
        };

        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .cloned()
            .unwrap_or(Value::Null);
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let content = choice
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if content.trim().is_empty() {
            last_error = AiSchedulerError::new(
                ERR_INVALID_RESPONSE,
                "服务商返回了空内容，请稍后重试或更换模型",
                true,
            );
            continue;
        }

        let mut warnings: Vec<AiPlanWarning> = Vec::new();
        if finish_reason == "length" {
            warnings.push(AiPlanWarning::new(
                WARN_TRUNCATED,
                "模型回答被截断（可尝试调大 max_tokens），已尽量解析",
            ));
        }

        let direct = serde_json::from_str::<RawPlanResponse>(strip_code_fences(&content));
        let response = match direct {
            Ok(response) => response,
            Err(_) => {
                let repaired = repair_json_text(&content);
                match serde_json::from_str::<RawPlanResponse>(&repaired) {
                    Ok(response) => {
                        warnings.push(AiPlanWarning::new(
                            WARN_SCHEMA_REPAIRED,
                            "模型输出的结构有偏差，已自动修补后再解析",
                        ));
                        response
                    }
                    Err(error) => {
                        last_error = AiSchedulerError::new(
                            ERR_INVALID_RESPONSE,
                            format!("模型没有按要求返回 JSON（{error}），正在重试"),
                            true,
                        );
                        continue;
                    }
                }
            }
        };

        // 一条都没给（既没排也没说排不下）通常是模型没理解结构。调用方保证队列非空，
        // 所以这不是「没东西可排」，重试一次比拿本地补排冒充 AI 结果更诚实。
        if response.items.is_empty() && response.unscheduled.is_empty() {
            last_error = AiSchedulerError::new(
                ERR_INVALID_RESPONSE,
                "模型返回了空的排期结果，请重试或更换模型",
                true,
            );
            continue;
        }

        return Ok(ChatOutcome { response, warnings });
    }

    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://api.deepseek.com/").unwrap(),
            "https://api.deepseek.com"
        );
        assert_eq!(
            normalize_base_url("  https://api.openai.com/v1  ").unwrap(),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn normalize_base_url_rejects_insecure_public_http() {
        let error = normalize_base_url("http://api.example.com").unwrap_err();
        assert_eq!(error.code, ERR_BAD_REQUEST);
        assert!(error.message.contains("https"));
    }

    #[test]
    fn normalize_base_url_allows_loopback_http_for_local_gateway() {
        assert!(normalize_base_url("http://127.0.0.1:8080/v1").is_ok());
        assert!(normalize_base_url("http://localhost:11434/v1").is_ok());
    }

    #[test]
    fn normalize_base_url_rejects_empty_and_broken_values() {
        assert_eq!(normalize_base_url("   ").unwrap_err().code, ERR_BAD_REQUEST);
        assert_eq!(
            normalize_base_url("not a url").unwrap_err().code,
            ERR_BAD_REQUEST
        );
    }

    #[test]
    fn endpoint_joins_without_double_slash() {
        assert_eq!(
            endpoint("https://api.deepseek.com", "chat/completions"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            endpoint("https://api.openai.com/v1/", "/models"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn classify_status_maps_known_codes() {
        assert_eq!(
            classify_status(StatusCode::UNAUTHORIZED, "{}", None).code,
            ERR_UNAUTHORIZED
        );
        assert_eq!(
            classify_status(StatusCode::FORBIDDEN, "{}", None).code,
            ERR_FORBIDDEN
        );
        assert_eq!(
            classify_status(StatusCode::PAYMENT_REQUIRED, "{}", None).code,
            ERR_QUOTA_EXCEEDED
        );
        assert_eq!(
            classify_status(StatusCode::NOT_FOUND, "{}", None).code,
            ERR_BAD_REQUEST
        );

        let rate_limited = classify_status(StatusCode::TOO_MANY_REQUESTS, "{}", Some(2.0));
        assert_eq!(rate_limited.code, ERR_RATE_LIMITED);
        assert!(rate_limited.retryable);
        assert_eq!(rate_limited.retry_after_seconds, Some(2.0));

        assert_eq!(
            classify_status(StatusCode::INTERNAL_SERVER_ERROR, "{}", None).code,
            ERR_SERVER_ERROR
        );
        assert!(classify_status(StatusCode::BAD_GATEWAY, "{}", None).retryable);
    }

    #[test]
    fn classify_status_surfaces_provider_message() {
        let body =
            r#"{"error":{"message":"model `gpt-5` not found","type":"invalid_request_error"}}"#;
        let error = classify_status(StatusCode::NOT_FOUND, body, None);
        assert!(error.message.contains("gpt-5"), "got {}", error.message);
    }

    #[test]
    fn response_format_rejection_is_detected_for_fallback() {
        let body = r#"{"error":{"message":"response_format type is unavailable now"}}"#;
        let detail = extract_error_message(body);
        assert!(detail.contains("response_format"));

        // 探测路径会把 server 文案拼进 message，据此识别「该档位不支持」。
        let enriched =
            AiSchedulerError::new(ERR_BAD_REQUEST, format!("请求被拒绝（{detail}）"), false);
        assert!(is_response_format_rejection(&enriched));

        // 与之无关的 400 不应被误判为档位不支持，否则会白白降级。
        let unrelated = AiSchedulerError::new(ERR_BAD_REQUEST, "model not found", false);
        assert!(!is_response_format_rejection(&unrelated));
        // 非 400 的错误同样不参与档位降级。
        let unauthorized = AiSchedulerError::new(ERR_UNAUTHORIZED, "response_format", false);
        assert!(!is_response_format_rejection(&unauthorized));
    }

    #[test]
    fn extract_error_message_handles_common_shapes() {
        assert_eq!(
            extract_error_message(r#"{"error":{"message":"boom"}}"#),
            "boom"
        );
        assert_eq!(extract_error_message(r#"{"message":"boom"}"#), "boom");
        assert_eq!(extract_error_message(r#"{"error":"boom"}"#), "boom");
        assert_eq!(extract_error_message("plain text"), "plain text");
    }

    #[test]
    fn response_format_payload_matches_mode() {
        let schema = response_format_for("json_schema").expect("schema mode payload");
        assert_eq!(schema["type"], "json_schema");
        assert_eq!(schema["json_schema"]["strict"], true);

        let object = response_format_for("json_object").expect("object mode payload");
        assert_eq!(object["type"], "json_object");

        assert!(response_format_for("auto").is_none());
    }

    /// 回归：`json_schema` 档的正式排期请求曾经带着探测用的 `{"ok": boolean}` schema，
    /// 约束解码下模型只能输出 `{"ok": true}`，草案恒为空。
    #[test]
    fn plan_request_uses_plan_schema_not_probe_schema() {
        let schema = plan_response_format("json_schema").expect("schema mode payload");
        let body = &schema["json_schema"]["schema"];
        assert_eq!(schema["json_schema"]["name"], PLAN_SCHEMA_NAME);
        assert!(body["properties"].get("ok").is_none());
        assert_eq!(body["required"], json!(["summary", "items", "unscheduled"]));
        assert_eq!(
            body["properties"]["items"]["items"]["required"],
            json!(["item_id", "date", "start_minute", "end_minute", "rationale"])
        );

        // 模型按这份 schema 输出的内容必须能被解析成草案候选。
        let sample = r#"{"summary":"s","items":[{"item_id":1,"date":"2026-09-30","start_minute":480,"end_minute":540,"rationale":""}],"unscheduled":[]}"#;
        let parsed: RawPlanResponse = serde_json::from_str(sample).expect("parse");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.summary.as_deref(), Some("s"));

        assert_eq!(
            plan_response_format("json_object").expect("object")["type"],
            "json_object"
        );
        assert!(plan_response_format("auto").is_none());
    }

    #[test]
    fn probe_system_prompt_keeps_json_keyword() {
        // `json_object` 档的硬性要求：提示词里必须出现 "json" 字样。
        let messages = probe_messages();
        let joined = messages.to_string();
        assert!(joined.to_lowercase().contains("json"));
    }

    #[test]
    fn code_fences_are_stripped() {
        assert_eq!(strip_code_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("  {\"a\":1}  "), "{\"a\":1}");
    }

    #[test]
    fn truncated_json_is_closed_back_into_shape() {
        let truncated = r#"{"items":[{"item_id":41,"date":"2026-09-25","start_minute":480,"end_minute":570,"rationale":"上午"#;
        let repaired = repair_json_text(truncated);
        let parsed: Result<RawPlanResponse, _> = serde_json::from_str(&repaired);
        assert!(parsed.is_ok(), "repaired = {repaired}");
        assert_eq!(parsed.unwrap().items.len(), 1);
    }

    #[test]
    fn trailing_commas_are_removed() {
        let raw = r#"{"items":[{"item_id":41,"date":"2026-09-25","start_minute":480,"end_minute":570,}],"unscheduled":[],}"#;
        let parsed: Result<RawPlanResponse, _> = serde_json::from_str(&repair_json_text(raw));
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().items.len(), 1);
    }

    #[test]
    fn repair_does_not_break_already_valid_json() {
        let raw = r#"{"items":[{"item_id":1,"date":"2026-09-25","start_minute":480,"end_minute":525,"rationale":null}],"unscheduled":[]}"#;
        let repaired = repair_json_text(raw);
        let parsed: RawPlanResponse = serde_json::from_str(&repaired).expect("still valid");
        assert_eq!(parsed.items.len(), 1);
        assert!(parsed.unscheduled.is_empty());
    }

    #[test]
    fn backoff_is_capped_and_honors_retry_after() {
        assert_eq!(backoff_duration(0, None), Duration::from_millis(600));
        assert_eq!(
            backoff_duration(9, None),
            Duration::from_millis(8_000),
            "指数退避封顶 8 秒"
        );
        assert_eq!(
            backoff_duration(0, Some(3.0)),
            Duration::from_millis(3_000),
            "服务端 Retry-After 优先"
        );
    }
}
