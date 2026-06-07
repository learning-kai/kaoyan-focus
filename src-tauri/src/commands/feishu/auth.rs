fn handle_oauth_code(app: &AppHandle, code: &str) -> Result<String, String> {
    let connection = open_database(&database_path(app)?)?;
    let settings = read_feishu_settings_for_api(&connection)?;
    let data = exchange_user_token(&settings, code)?;
    persist_token_response(&connection, &data)?;
    Ok("飞书登录成功，已保存本机 Token。".to_string())
}

fn ensure_access_token(connection: &Connection) -> Result<TokenSet, String> {
    let access_token = credential::get_secret(connection, FEISHU_ACCESS_TOKEN_KEY)?;
    let refresh_token = credential::get_secret(connection, FEISHU_REFRESH_TOKEN_KEY)?;
    let expires_at_raw = get_setting(connection, FEISHU_TOKEN_EXPIRES_AT_KEY, "")?;
    let expires_at = parse_rfc3339(&expires_at_raw).unwrap_or_else(Utc::now);
    if !access_token.is_empty() && expires_at > Utc::now() + Duration::seconds(60) {
        return Ok(TokenSet { access_token });
    }
    if refresh_token.is_empty() {
        return Err("飞书尚未登录，请先完成浏览器授权。".to_string());
    }
    let settings = read_feishu_settings_for_api(connection)?;
    let data = refresh_user_token(&settings, &refresh_token)?;
    persist_token_response(connection, &data)?;
    Ok(TokenSet {
        access_token: credential::get_secret(connection, FEISHU_ACCESS_TOKEN_KEY)?,
    })
}

fn exchange_user_token(settings: &FeishuSyncSettings, code: &str) -> Result<Value, String> {
    let client = http_client()?;
    let v2 = client
        .post(format!("{FEISHU_BASE}/open-apis/authen/v2/oauth/token"))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "grant_type": "authorization_code",
            "client_id": settings.app_id,
            "client_secret": settings.app_secret,
            "code": code,
            "redirect_uri": settings.redirect_uri,
        }))
        .send()
        .map_err(|error| format!("交换飞书 user_access_token 失败：{error}"))
        .and_then(parse_feishu_response);
    match v2 {
        Ok(value) => Ok(value),
        Err(v2_error) => {
            let app_access_token = get_app_access_token(settings)?;
            client
                .post(format!("{FEISHU_BASE}/open-apis/authen/v1/access_token"))
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {app_access_token}"))
                .json(&json!({
                    "grant_type": "authorization_code",
                    "code": code
                }))
                .send()
                .map_err(|error| format!("交换飞书 user_access_token 失败：{error}"))
                .and_then(parse_feishu_response)
                .map_err(|v1_error| {
                    format!("飞书 OAuth v2 交换失败：{v2_error}；v1 兼容交换也失败：{v1_error}")
                })
        }
    }
}

fn refresh_user_token(settings: &FeishuSyncSettings, refresh_token: &str) -> Result<Value, String> {
    let client = http_client()?;
    let v2 = client
        .post(format!("{FEISHU_BASE}/open-apis/authen/v2/oauth/token"))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "grant_type": "refresh_token",
            "client_id": settings.app_id,
            "client_secret": settings.app_secret,
            "refresh_token": refresh_token
        }))
        .send()
        .map_err(|error| format!("刷新飞书 Token 失败：{error}"))
        .and_then(parse_feishu_response);
    match v2 {
        Ok(value) => Ok(value),
        Err(v2_error) => {
            let app_access_token = get_app_access_token(settings)?;
            client
                .post(format!(
                    "{FEISHU_BASE}/open-apis/authen/v1/refresh_access_token"
                ))
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {app_access_token}"))
                .json(&json!({
                    "grant_type": "refresh_token",
                    "refresh_token": refresh_token
                }))
                .send()
                .map_err(|error| format!("刷新飞书 Token 失败：{error}"))
                .and_then(parse_feishu_response)
                .map_err(|v1_error| {
                    format!("飞书 OAuth v2 刷新失败：{v2_error}；v1 兼容刷新也失败：{v1_error}")
                })
        }
    }
}

fn get_app_access_token(settings: &FeishuSyncSettings) -> Result<String, String> {
    let response = http_client()?
        .post(format!(
            "{FEISHU_BASE}/open-apis/auth/v3/app_access_token/internal"
        ))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "app_id": settings.app_id,
            "app_secret": settings.app_secret,
        }))
        .send()
        .map_err(|error| format!("获取飞书 app_access_token 失败：{error}"))?;
    let data = parse_feishu_response(response)?;
    data.get("app_access_token")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "飞书未返回 app_access_token。".to_string())
}

fn persist_token_response(connection: &Connection, value: &Value) -> Result<(), String> {
    let access_token = value
        .get("access_token")
        .or_else(|| value.get("user_access_token"))
        .and_then(Value::as_str)
        .ok_or_else(|| "飞书未返回 user_access_token。".to_string())?;
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let expires_in = value
        .get("expires_in")
        .or_else(|| value.get("expires_in_sec"))
        .and_then(value_to_i64)
        .unwrap_or(7200);
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    let now = Utc::now().to_rfc3339();
    credential::set_secret(connection, FEISHU_ACCESS_TOKEN_KEY, access_token, &now)?;
    if !refresh_token.is_empty() {
        credential::set_secret(connection, FEISHU_REFRESH_TOKEN_KEY, refresh_token, &now)?;
    }
    set_setting(
        connection,
        FEISHU_TOKEN_EXPIRES_AT_KEY,
        &expires_at.to_rfc3339(),
        &now,
    )?;
    Ok(())
}

fn read_feishu_settings(connection: &Connection) -> Result<FeishuSyncSettings, String> {
    Ok(normalize_settings(FeishuSyncSettings {
        enabled: get_bool_setting(connection, FEISHU_SYNC_ENABLED_KEY, false)?,
        app_id: get_setting(connection, FEISHU_APP_ID_KEY, "")?,
        app_secret: String::new(),
        app_secret_configured: credential::secret_configured(connection, FEISHU_APP_SECRET_KEY)?,
        redirect_uri: get_setting(connection, FEISHU_REDIRECT_URI_KEY, DEFAULT_REDIRECT_URI)?,
    }))
}

fn normalize_settings(settings: FeishuSyncSettings) -> FeishuSyncSettings {
    FeishuSyncSettings {
        enabled: settings.enabled,
        app_id: settings.app_id.trim().to_string(),
        app_secret: settings.app_secret.trim().to_string(),
        app_secret_configured: !settings.app_secret.trim().is_empty()
            || settings.app_secret_configured,
        redirect_uri: settings
            .redirect_uri
            .trim()
            .to_string()
            .if_empty(DEFAULT_REDIRECT_URI),
    }
}

fn read_feishu_settings_for_api(connection: &Connection) -> Result<FeishuSyncSettings, String> {
    let mut settings = read_feishu_settings(connection)?;
    settings.app_secret = credential::get_secret(connection, FEISHU_APP_SECRET_KEY)?;
    settings.app_secret_configured = !settings.app_secret.is_empty();
    Ok(settings)
}

fn resolve_feishu_secret(
    connection: &Connection,
    mut settings: FeishuSyncSettings,
) -> Result<FeishuSyncSettings, String> {
    if settings.app_secret.is_empty() {
        settings.app_secret = credential::get_secret(connection, FEISHU_APP_SECRET_KEY)?;
    }
    settings.app_secret_configured = !settings.app_secret.is_empty();
    Ok(settings)
}

fn redact_feishu_settings(mut settings: FeishuSyncSettings) -> FeishuSyncSettings {
    settings.app_secret_configured =
        !settings.app_secret.is_empty() || settings.app_secret_configured;
    settings.app_secret.clear();
    settings
}

fn clear_feishu_tokens(connection: &Connection) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    credential::set_secret(connection, FEISHU_ACCESS_TOKEN_KEY, "", &now)?;
    credential::set_secret(connection, FEISHU_REFRESH_TOKEN_KEY, "", &now)?;
    for key in [
        FEISHU_TOKEN_EXPIRES_AT_KEY,
        FEISHU_TASKLIST_GUID_KEY,
        FEISHU_LEGACY_TASKLIST_GUID_KEY,
        FEISHU_CALENDAR_ID_KEY,
        &feishu_tasklist_setting_key(TASKLIST_KEY_POLITICS),
        &feishu_tasklist_setting_key(TASKLIST_KEY_ENGLISH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MATH),
        &feishu_tasklist_setting_key(TASKLIST_KEY_MAJOR),
        &feishu_tasklist_setting_key(TASKLIST_KEY_GENERAL),
        &feishu_tasklist_setting_key(TASKLIST_KEY_TODAY),
        FEISHU_OAUTH_STATE_KEY,
        FEISHU_OAUTH_URL_KEY,
        FEISHU_OAUTH_MESSAGE_KEY,
    ] {
        set_setting(connection, key, "", &now)?;
    }
    Ok(())
}

fn is_feishu_access_token_usable(access_token: &str, expires_at: Option<&str>) -> bool {
    if access_token.is_empty() {
        return false;
    }

    expires_at
        .and_then(parse_rfc3339)
        .map(|value| value > Utc::now() + Duration::seconds(60))
        .unwrap_or(true)
}

fn is_app_tasklist_name(name: &str) -> bool {
    name == BRIDGE_CONTAINER_NAME || name.starts_with("考研专注 - ")
}

fn receive_oauth_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|error| format!("接收飞书登录回调失败：{error}"))?;
    let mut buffer = [0_u8; 8192];
    let size = stream
        .read(&mut buffer)
        .map_err(|error| format!("读取飞书登录回调失败：{error}"))?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let first_line = request.lines().next().unwrap_or_default();
    let target = first_line.split_whitespace().nth(1).unwrap_or_default();
    let query = target
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let params = parse_query_params(query);
    let result = if let Some(error) = params.get("error") {
        Err(format!("飞书授权失败：{error}"))
    } else if params.get("state").map(String::as_str) != Some(expected_state) {
        Err("飞书授权 state 不匹配，请重新登录。".to_string())
    } else {
        params
            .get("code")
            .filter(|value| !value.is_empty())
            .cloned()
            .ok_or_else(|| "飞书回调未包含授权 code。".to_string())
    };
    let body = match &result {
        Ok(_) => "Feishu login received. You can return to Kaoyan Focus.",
        Err(_) => "Feishu login failed. Please return to Kaoyan Focus and retry.",
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    result
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn callback_bind_addr(redirect_uri: &str) -> Result<String, String> {
    let url = Url::parse(redirect_uri).map_err(|error| format!("飞书回调地址不正确：{error}"))?;
    let host = url.host_str().unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" {
        return Err("飞书本地回调地址必须使用 127.0.0.1 或 localhost。".to_string());
    }
    let port = url.port().unwrap_or(80);
    Ok(format!("127.0.0.1:{port}"))
}

fn get_setting(connection: &Connection, key: &str, fallback: &str) -> Result<String, String> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn get_bool_setting(connection: &Connection, key: &str, fallback: bool) -> Result<bool, String> {
    let raw = get_setting(connection, key, if fallback { "true" } else { "false" })?;
    Ok(matches!(raw.as_str(), "true" | "1" | "yes" | "on"))
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
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
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn non_empty_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    let value = get_setting(connection, key, "")?.trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

