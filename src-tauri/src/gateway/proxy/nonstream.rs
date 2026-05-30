// 非流式响应处理：解码上游 EventStream、聚合、token 估算、Prompt Cache 模拟、
// 构建协议响应、写响应缓存与请求日志。从 proxy_handler 抽出，保持行为不变。
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_nonstream_response(
    state: RouterState,
    format: ResponseFormat,
    upstream_resp: reqwest::Response,
    request: &NormalizedRequest,
    response_id: String,
    message_id: String,
    created_at: i64,
    upstream_payload_log_context: RequestLogContext<'_>,
    cache_session_id: String,
    messages_hash: String,
    cache_message_count: usize,
    cache_total_chars: usize,
) -> Response {
    // 非流式响应也是 EventStream 格式，需要解码
    let raw_bytes = match upstream_resp.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = sanitize_error(&format!("读取上游响应失败: {error}"));
            return gateway_error_with_log(
                &state,
                format,
                &upstream_payload_log_context,
                GatewayErrorDetails {
                    status: StatusCode::BAD_GATEWAY,
                    error_type: "api_error",
                    message: &message,
                    response_body: None,
                },
            )
            .await;
        }
    };

    // 添加调试日志：记录原始响应体大小和前几个字节
    log::debug!(
        "[非流式响应] 原始字节大小: {} 字节",
        raw_bytes.len(),
    );

    // 解码 EventStream 消息并提取所有 JSON payload
    let mut buffer = raw_bytes.to_vec();
    let mut json_payloads = Vec::new();
    let mut message_count = 0;

    loop {
        match decode_message(&buffer) {
            Ok(Some((msg, consumed_bytes))) => {
                message_count += 1;
                let message_type = msg.headers.get(":message-type").map(String::as_str);
                let event_type = msg.headers.get(":event-type").map(String::as_str);

                log::info!(
                    "[非流式响应] 消息 #{}: type={:?}, event={:?}, payload_size={} 字节",
                    message_count,
                    message_type,
                    event_type,
                    msg.payload.len()
                );

                // 检查错误消息
                if matches!(message_type, Some("error") | Some("exception")) {
                    let error_text = String::from_utf8_lossy(&msg.payload);
                    log::error!(
                        "EventStream 上游错误: message_type={:?}, event_type={:?}, payload={}",
                        message_type,
                        event_type,
                        error_text
                    );

                    if let Some((status, error_type, message)) = detect_upstream_error_body(&error_text) {
                        return gateway_error_with_log(
                            &state,
                            format,
                            &upstream_payload_log_context,
                            GatewayErrorDetails {
                                status,
                                error_type,
                                message: &message,
                                response_body: Some(&error_text),
                            },
                        )
                        .await;
                    }
                }

                // 只处理事件类型的消息
                if matches!(message_type, Some("event")) {
                    let json_text = String::from_utf8_lossy(&msg.payload);
                    log::info!(
                        "[Non-Stream Response] Event payload: {}",
                        json_text.chars().take(500).collect::<String>()
                    );
                    json_payloads.push(json_text.to_string());
                }

                buffer.drain(..consumed_bytes);
            }
            Ok(None) => {
                // 缓冲区数据不足，已处理完所有消息
                log::info!(
                    "[非流式响应] EventStream 解码完成，剩余缓冲区: {} 字节", buffer.len());
                break;
            }
            Err(e) => {
                log::error!("EventStream 解码失败: {}, 剩余缓冲区: {} 字节", e, buffer.len());
                break;
            }
        }
    }

    // 用于调试日志的拼接字符串
    let body = json_payloads.join("");

    // 添加调试日志：记录解码后的 JSON 数量和预览
    log::info!(
        "[非流式响应] 解码了 {} 条 EventStream 消息, 总 body 长度: {} 字符, body 预览: {}",
        json_payloads.len(),
        body.len(),
        body.chars().take(1000).collect::<String>()
    );

    let mut aggregated = stream::aggregate_kiro_response_from_payloads(&json_payloads);

    // 直接使用本地估算 token（不依赖响应中的 token 信息）
    log::info!("[非流式响应] 使用本地 token 估算");

    // 估算输入 tokens（从请求消息中）
    let request_text = serde_json::to_string(&request.messages).unwrap_or_default();
    aggregated.input_tokens = crate::gateway::token_estimator::estimate_tokens(&request_text, &request.model);

    // 估算输出 tokens（从响应文本中）
    let response_text = format!("{}{}", aggregated.text, aggregated.thinking);
    aggregated.output_tokens = crate::gateway::token_estimator::estimate_tokens(&response_text, &request.model);

    log::info!(
        "[非流式响应] 估算的 tokens: input={}, output={} (model={})",
        aggregated.input_tokens,
        aggregated.output_tokens,
        request.model
    );

    // 调试：记录 aggregated 的详细信息
    log::info!(
        "[非流式响应] 聚合详情: text_len={}, thinking_len={}, tool_calls={}, citations={}",
        aggregated.text.len(),
        aggregated.thinking.len(),
        aggregated.tool_calls.len(),
        aggregated.citations.len()
    );

    // Prompt Cache 模拟：如果响应中没有缓存信息，用模拟器填充
    if aggregated.cache_read_input_tokens.is_none() && aggregated.cache_creation_input_tokens.is_none() {
        let tracker = crate::gateway::prompt_cache::global_prompt_cache_tracker();
        let messages_json: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        }).collect();
        let tools_json: Option<Vec<serde_json::Value>> = request.tools.as_ref().map(|tools| {
            tools.iter().map(|t| serde_json::to_value(t).unwrap_or_default()).collect()
        });

        if let Some(profile) = tracker.build_profile(
            None,
            &messages_json,
            tools_json.as_deref(),
            aggregated.input_tokens as usize,
            &request.model,
        ) {
            let cache_usage = tracker.compute(&request.model, &profile);
            tracker.update(&request.model, &profile);

            if cache_usage.cache_read_input_tokens > 0 {
                aggregated.cache_read_input_tokens = Some(cache_usage.cache_read_input_tokens as i32);
            }
            if cache_usage.cache_creation_input_tokens > 0 {
                aggregated.cache_creation_input_tokens = Some(cache_usage.cache_creation_input_tokens as i32);
            }

            log::info!(
                "[非流式] Prompt Cache 模拟: read={}, creation={}",
                cache_usage.cache_read_input_tokens,
                cache_usage.cache_creation_input_tokens
            );
        }
    }

    // 还原工具名称（sanitized -> original）
    for (_, name, _) in &mut aggregated.tool_calls {
        if let Some(original) = request.tool_name_map.get(name.as_str()) {
            *name = original.clone();
        }
    }

    let response = match format {
        ResponseFormat::Anthropic => build_anthropic_response(&request.model, &aggregated),
        ResponseFormat::Responses => build_responses_response_with_ids(
            &request.model,
            &aggregated,
            &response_id,
            &message_id,
            created_at,
            request.previous_response_id.as_deref(),
        ),
        ResponseFormat::OpenAI => {
            serde_json::to_value(stream::build_openai_response(&request.model, &aggregated))
                .unwrap_or_else(|_| json!({}))
        }
    };
    if matches!(format, ResponseFormat::Responses) {
        persist_responses_session_entry(
            &state,
            &response_id,
            request.messages.clone(),
            request.tools.clone(),
            request.tool_choice.clone(),
            request.previous_response_id.clone(),
            &aggregated,
        )
        .await;
    }
    // ===== 响应缓存：写入（仅非流式成功响应） =====
    if state.config.response_cache_enabled {
        let response_json = serde_json::to_string(&response).unwrap_or_default();
        let mut cache_guard = state.response_cache.lock().await;
        cache_guard.put(
            &cache_session_id,
            &messages_hash,
            response_json,
            aggregated.input_tokens,
            aggregated.output_tokens,
            cache_message_count,
            cache_total_chars,
        );
        drop(cache_guard);
        log::debug!(
            "[响应缓存] 已写入: session={}, hash={}",
            &cache_session_id[..cache_session_id.len().min(16)],
            &messages_hash[..16]
        );
    }

    write_request_log(
        &upstream_payload_log_context,
        StatusCode::OK,
        "success",
        None,
        None, // error_type
        Some(body.as_str()),
        Some(aggregated.input_tokens),
        Some(aggregated.output_tokens),
        aggregated.cache_read_input_tokens,
        aggregated.cache_creation_input_tokens,
        &state,
    );
    Json(response).into_response()
}
