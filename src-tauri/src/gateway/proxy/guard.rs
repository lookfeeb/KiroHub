// 网关请求准入与鉴权：localOnly / 白名单 / 客户端 API Key 校验，以及端点映射。
use super::*;

/// 统一客户端请求准入校验：localOnly、白名单、API Key。
/// 命中拦截时返回 `Some(error_response)`，通过则返回 `None`。
pub(crate) async fn guard_client_request(
    state: &RouterState,
    format: ResponseFormat,
    log_context: &RequestLogContext<'_>,
    headers: &HeaderMap,
    client_addr: SocketAddr,
) -> Option<Response> {
    if state.config.local_only && !client_addr.ip().is_loopback() {
        let message = format!("已拒绝来自非本机地址的访问: {}", client_addr.ip());
        return Some(
            gateway_error_with_log(
                state,
                format,
                log_context,
                GatewayErrorDetails {
                    status: StatusCode::FORBIDDEN,
                    error_type: "permission_error",
                    message: &message,
                    response_body: None,
                },
            )
            .await,
        );
    }
    if !state.config.local_only
        && !state.config.allowed_ips.is_empty()
        && !ip_matches_allowlist(client_addr.ip(), &state.config.allowed_ips)
    {
        let message = format!("访问地址 {} 不在反代白名单中", client_addr.ip());
        return Some(
            gateway_error_with_log(
                state,
                format,
                log_context,
                GatewayErrorDetails {
                    status: StatusCode::FORBIDDEN,
                    error_type: "permission_error",
                    message: &message,
                    response_body: None,
                },
            )
            .await,
        );
    }
    if let Err(message) = verify_client_auth(headers, &state.config) {
        let sanitized = sanitize_error(&message);
        return Some(
            gateway_error_with_log(
                state,
                format,
                log_context,
                GatewayErrorDetails {
                    status: StatusCode::UNAUTHORIZED,
                    error_type: "authentication_error",
                    message: &sanitized,
                    response_body: None,
                },
            )
            .await,
        );
    }
    None
}

pub(crate) fn request_endpoint(format: ResponseFormat) -> &'static str {
    match format {
        ResponseFormat::Anthropic => "messages",
        ResponseFormat::Responses => "responses",
        ResponseFormat::OpenAI => "chat_completions",
    }
}

pub(crate) fn verify_client_auth(headers: &HeaderMap, config: &GatewayConfig) -> Result<(), String> {
    let expected_keys = effective_client_api_keys(config);
    if expected_keys.is_empty() {
        return Err("客户端 API Key 未配置".to_string());
    }

    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok());

    if expected_keys
        .iter()
        .any(|expected| authorization == Some(expected.as_str()) || api_key == Some(expected.as_str()))
    {
        Ok(())
    } else {
        Err("客户端 API Key 无效".to_string())
    }
}
