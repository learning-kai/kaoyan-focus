struct FeishuClient {
    client: Client,
    token: String,
}

impl FeishuClient {
    fn new(token: String) -> Result<Self, String> {
        Ok(Self {
            client: http_client()?,
            token,
        })
    }

    fn get_paged(&self, path_or_url: &str) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut next_url = Some(feishu_url(path_or_url));
        while let Some(url) = next_url {
            let data = self
                .client
                .get(&url)
                .headers(self.auth_headers()?)
                .send()
                .map_err(|error| format!("飞书分页读取失败：{error}"))
                .and_then(parse_feishu_response)?;
            if let Some(values) = data.get("items").and_then(Value::as_array) {
                items.extend(values.iter().cloned());
            }
            let has_more = data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let token = data
                .get("page_token")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            next_url = if has_more {
                token.map(|value| append_query_param(&url, "page_token", value))
            } else {
                None
            };
        }
        Ok(items)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.client
            .post(feishu_url(path))
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .map_err(|error| format!("飞书 POST 失败：{error}"))
            .and_then(parse_feishu_response)
    }

    fn patch(&self, path: &str, body: Value) -> Result<Value, String> {
        self.client
            .patch(feishu_url(path))
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .map_err(|error| format!("飞书 PATCH 失败：{error}"))
            .and_then(parse_feishu_response)
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        let response = self
            .client
            .delete(feishu_url(path))
            .headers(self.auth_headers()?)
            .send()
            .map_err(|error| format!("飞书 DELETE 失败：{error}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        parse_feishu_response(response).map(|_| ())
    }

    fn auth_headers(&self) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|error| error.to_string())?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }
}

