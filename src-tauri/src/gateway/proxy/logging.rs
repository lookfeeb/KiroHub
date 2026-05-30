use super::*;

pub(crate) fn serialize_logged_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}


/// 从原始请求体 JSON 中提取 model 字段（用于错误日志）
pub(crate) fn extract_model_from_payload(payload_str: &str) -> Option<String> {
    serde_json::from_str::<Value>(payload_str)
        .ok()?
        .get("model")?
        .as_str()
        .map(String::from)
}


pub(crate) fn write_request_log(
    context: &RequestLogContext<'_>,
    status: StatusCode,
    outcome: &str,
    error: Option<&str>,
    error_type: Option<&str>,
    _response_body: Option<&str>,
    input_tokens: Option<i32>,
    output_tokens: Option<i32>,
    cache_read_input_tokens: Option<i32>,
    cache_creation_input_tokens: Option<i32>,
    state: &RouterState,
) {
    let duration_ms = context
        .started_at
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;

    // 只在出错时记录日志
    if !status.is_success() {
        log::error!(
            "请求失败 #{} | {} | {} | {}ms | {}",
            context.request_index,
            context.endpoint,
            status.as_u16(),
            duration_ms,
            error.unwrap_or("未知错误")
        );
    }

    let entry = GatewayRequestLogEntry {
        occurred_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        request_index: context.request_index,
        endpoint: context.endpoint.to_string(),
        client_ip: context.client_addr.ip().to_string(),
        model: context
            .request
            .map(|item| item.model.clone())
            .or_else(|| context.model_hint.clone()),
        stream: context.is_stream
            .or_else(|| context.request.map(|item| item.stream))
            .unwrap_or(false),
        upstream_source: context.upstream.map(|item| item.source_label.clone()),
        region: context.upstream.map(|item| item.region.clone()),
        status_code: status.as_u16(),
        outcome: outcome.to_string(),
        duration_ms,
        error: error.map(str::to_string),
        request_body: context.request_body.map(str::to_string),
        response_body: _response_body.map(str::to_string),
        input_tokens,
        output_tokens,
        cache_read_input_tokens,
        cache_creation_input_tokens,
        error_type: error_type.map(str::to_string),
    };

    // 如果关闭了日志记录，跳过
    if !state.config.log_requests {
        return;
    }

    // 写入文件日志
    let _ = append_gateway_request_log(&entry);

    // 保存到内存日志存储（异步）
    let log_store = state.log_store.clone();
    let entry_clone = entry.clone();
    tokio::spawn(async move {
        log_store.add(entry_clone).await;
    });
}


pub(crate) fn build_gateway_error_body(
    format: ResponseFormat,
    status: StatusCode,
    error_type: &str,
    message: &str,
) -> Value {
    match format {
        ResponseFormat::Anthropic => json!({
            "type": "error",
            "error": {
                "type": error_type,
                "message": message
            }
        }),
        ResponseFormat::Responses => json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": status.as_u16()
            }
        }),
        ResponseFormat::OpenAI => json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": status.as_u16()
            }
        }),
    }
}


pub(crate) async fn gateway_error_with_log(
    state: &RouterState,
    format: ResponseFormat,
    context: &RequestLogContext<'_>,
    error: GatewayErrorDetails<'_>,
) -> Response {
    *state.last_error.lock().await = Some(error.message.to_string());

    // 尝试从错误响应体中提取token信息
    let (input_tokens, output_tokens, cache_read, cache_creation) = error.response_body
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|json| {
            let usage = json.get("usage")?;
            Some((
                usage.get("input_tokens").and_then(|v| v.as_i64()).map(|v| v as i32),
                usage.get("output_tokens").and_then(|v| v.as_i64()).map(|v| v as i32),
                usage.get("cache_read_input_tokens").and_then(|v| v.as_i64()).map(|v| v as i32),
                usage.get("cache_creation_input_tokens").and_then(|v| v.as_i64()).map(|v| v as i32),
            ))
        })
        .unwrap_or((None, None, None, None));

    let logged_response_body = error.response_body.map(str::to_string).or_else(|| {
        Some(serialize_logged_value(&build_gateway_error_body(
            format,
            error.status,
            error.error_type,
            error.message,
        )))
    });
    write_request_log(
        context,
        error.status,
        "error",
        Some(error.message),
        Some(error.error_type),
        logged_response_body.as_deref(),
        input_tokens,
        output_tokens,
        cache_read,
        cache_creation,
        state,
    );
    gateway_error_response(format, error.status, error.error_type, error.message)
}


pub(crate) fn gateway_error_response(
    format: ResponseFormat,
    status: StatusCode,
    error_type: &str,
    message: &str,
) -> Response {
    let body = build_gateway_error_body(format, status, error_type, message);
    (status, Json(body)).into_response()
}

