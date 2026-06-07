fn parse_feishu_response(response: Response) -> Result<Value, String> {
    let status = response.status();
    let value: Value = response.json().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(feishu_value_error_message(status, &value));
    }
    if value.get("access_token").is_some() || value.get("user_access_token").is_some() {
        return Ok(value);
    }
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code == 0 {
        return Ok(value.get("data").cloned().unwrap_or(value));
    }
    Err(feishu_value_error_message(status, &value))
}

fn feishu_value_error_message(status: StatusCode, value: &Value) -> String {
    let message = value
        .get("msg")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .or_else(|| value.get("error_description").and_then(Value::as_str))
        .or_else(|| value.get("error").and_then(Value::as_str))
        .unwrap_or("未知错误");
    let code = value
        .get("code")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    format!(
        "飞书返回状态 {} / code {}：{}",
        status.as_u16(),
        code,
        message
    )
}

fn is_feishu_deleted_event_error(error: &str) -> bool {
    error.contains("193003")
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={}", encode_form_component(value))
}

fn body_with_marker(note: Option<&str>, entity_type: &str, sync_id: &str) -> String {
    let mut content = note.unwrap_or("").trim().to_string();
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(&format!("{MARKER_PREFIX}{entity_type}:{sync_id}]"));
    content
}

fn marker_json(entity_type: &str, sync_id: &str) -> String {
    json!({
        "source": "kaoyan-focus",
        "entity_type": entity_type,
        "sync_id": sync_id
    })
    .to_string()
}

fn extract_marker(raw: &str) -> Option<(String, String)> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if value.get("source").and_then(Value::as_str) == Some("kaoyan-focus") {
            let entity_type = value.get("entity_type").and_then(Value::as_str)?;
            let sync_id = value.get("sync_id").and_then(Value::as_str)?;
            return Some((entity_type.to_string(), sync_id.to_string()));
        }
    }
    raw.lines().find_map(|line| {
        let trimmed = line.trim();
        let body = trimmed.strip_prefix(MARKER_PREFIX)?.strip_suffix(']')?;
        let (entity_type, sync_id) = body.split_once(':')?;
        Some((entity_type.to_string(), sync_id.to_string()))
    })
}

fn strip_marker(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.trim_start().starts_with(MARKER_PREFIX))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn calendar_sync_range() -> (i64, i64) {
    let today = Local::now().date_naive();
    let start = today - Duration::days(30);
    let end = today + Duration::days(180);
    (
        local_date_minute_to_timestamp(&start.format("%Y-%m-%d").to_string(), 0),
        local_date_minute_to_timestamp(&end.format("%Y-%m-%d").to_string(), 1439),
    )
}

fn date_in_sync_range(date: &str) -> bool {
    let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return false;
    };
    let today = Local::now().date_naive();
    date >= today - Duration::days(30) && date <= today + Duration::days(180)
}

fn today_date_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn due_date_to_millis(date: &str) -> i64 {
    local_date_minute_to_timestamp(date, 0) * 1000
}

fn minute_to_timestamp(date: &str, minute: i64) -> i64 {
    local_date_minute_to_timestamp(date, minute.clamp(0, 1440))
}

fn local_date_minute_to_timestamp(date: &str, minute: i64) -> i64 {
    let date =
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive());
    let clamped = minute.clamp(0, 1440);
    let hour = (clamped / 60).min(23) as u32;
    let minute = if clamped >= 1440 {
        59
    } else {
        (clamped % 60) as u32
    };
    let naive = date
        .and_hms_opt(hour, minute, if clamped >= 1440 { 59 } else { 0 })
        .unwrap_or_else(|| Local::now().naive_local());
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .unwrap_or_else(Local::now)
        .timestamp()
}

fn timestamp_to_local_date_minute(timestamp: i64) -> Option<(String, i64)> {
    let timestamp = normalize_timestamp_seconds(timestamp);
    let value = Local.timestamp_opt(timestamp, 0).single()?;
    Some((
        value.date_naive().format("%Y-%m-%d").to_string(),
        i64::from(value.hour()) * 60 + i64::from(value.minute()),
    ))
}

fn millis_to_local_date_string(value: i64) -> String {
    let seconds = normalize_timestamp_seconds(value);
    Local
        .timestamp_opt(seconds, 0)
        .single()
        .unwrap_or_else(Local::now)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn normalize_timestamp_seconds(value: i64) -> i64 {
    if value > 9_999_999_999 {
        value / 1000
    } else {
        value
    }
}

fn normalize_timestamp_millis(value: i64) -> i64 {
    if value < 9_999_999_999 {
        value * 1000
    } else {
        value
    }
}

fn parse_link_millis(value: &str) -> Option<i64> {
    value
        .parse::<i64>()
        .ok()
        .map(normalize_timestamp_millis)
        .or_else(|| parse_rfc3339_millis(value).ok())
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn parse_rfc3339_millis(value: &str) -> Result<i64, String> {
    parse_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .ok_or_else(|| format!("时间格式不正确：{value}"))
}

fn millis_to_rfc3339(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|item| item.parse::<i64>().ok()))
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn encode_form_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
