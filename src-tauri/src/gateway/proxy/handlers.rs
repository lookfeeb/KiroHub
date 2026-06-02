use super::*;

pub(crate) async fn guarded_local_response(
    state: RouterState,
    client_addr: SocketAddr,
    headers: HeaderMap,
    endpoint: &'static str,
    request_body: Option<&str>,
    response_body: Value,
) -> Response {
    let request_index = state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let started_at = Instant::now();
    let log_context = RequestLogContext {
        request_index,
        endpoint,
        client_addr,
        request: None,
        upstream: None,
        started_at,
        request_body,
        model_hint: None,
        is_stream: None,
    };

    if let Some(blocked) =
        guard_client_request(&state, ResponseFormat::Responses, &log_context, &headers, client_addr)
            .await
    {
        return blocked;
    }

    write_request_log(
        &log_context,
        StatusCode::OK,
        "success",
        None,
        None, // error_type
        None, // response_body（轻量端点默认不记录）
        None, // input_tokens
        None, // output_tokens
        None, // cache_read_input_tokens
        None, // cache_creation_input_tokens
        &state,
    );
    Json(response_body).into_response()
}


pub async fn health_handler(
    state: RouterState,
    client_addr: SocketAddr,
    headers: HeaderMap,
) -> Response {
    guarded_local_response(
        state,
        client_addr,
        headers,
        "health",
        None,
        build_health_response(),
    )
    .await
}


pub async fn models_handler(
    state: RouterState,
    client_addr: SocketAddr,
    headers: HeaderMap,
) -> Response {
    guarded_local_response(
        state,
        client_addr,
        headers,
        "models",
        None,
        build_models_response(),
    )
    .await
}


pub async fn count_tokens_handler(
    state: RouterState,
    client_addr: SocketAddr,
    headers: HeaderMap,
    payload: Value,
) -> Response {
    // count_tokens 为轻量工具端点，默认不记录请求体
    guarded_local_response(
        state,
        client_addr,
        headers,
        "count_tokens",
        None,
        build_count_tokens_response(&payload),
    )
    .await
}


pub async fn proxy_handler(
    state: RouterState,
    client_addr: SocketAddr,
    headers: HeaderMap,
    payload: Value,
    format: ResponseFormat,
) -> Response {
    let request_index = state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let endpoint = request_endpoint(format);
    let started_at = Instant::now();
    let raw_request_body = payload.to_string();

    let model_hint = extract_model_from_payload(&raw_request_body);
    let base_log_context = RequestLogContext {
        request_index,
        endpoint,
        client_addr,
        request: None,
        upstream: None,
        started_at,
        request_body: Some(raw_request_body.as_str()),
        model_hint,
        is_stream: None,
    };

    if let Some(blocked) =
        guard_client_request(&state, format, &base_log_context, &headers, client_addr).await
    {
        return blocked;
    }

    let mut request = match normalize_request(format, &payload) {
        Ok(request) => request,
        Err(message) => {
            let sanitized = sanitize_error(&message);
            return gateway_error_with_log(
                &state,
                format,
                &base_log_context,
                GatewayErrorDetails {
                    status: StatusCode::BAD_REQUEST,
                    error_type: "invalid_request_error",
                    message: &sanitized,
                    response_body: None,
                },
            )
            .await;
        }
    };

    // 模型映射：根据规则替换请求的模型名
    let original_model = request.model.clone();
    request.model = crate::gateway::resolve_model_mapping(&state.config, &request.model);
    if request.model != original_model {
        log::info!("[模型映射] {} → {}", original_model, request.model);
    }

    // 添加详细的请求日志（参考 Kiro-account-manager 的日志设计）
    let messages_count = request.messages.len();
    let tools_count = request.tools.as_ref().map(|t| t.len()).unwrap_or(0);
    let has_tool_choice = request.tool_choice.is_some();
    let content_length: usize = request.messages.iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.to_string().len())
        .sum();

    log::info!(
        "[请求详情] 请求 #{} | 模型={} | 流式={} | 消息数={} | 工具数={} | 工具选择={} | 内容长度={}",
        request_index,
        request.model,
        request.stream,
        messages_count,
        tools_count,
        has_tool_choice,
        content_length
    );
    let mut request = if matches!(format, ResponseFormat::Responses) {
        let mut resumed = request.clone();
        resumed.messages = restore_responses_session_messages(&state, &request).await;
        // 如果当前请求没有 tools/tool_choice，从历史 session 继承
        if resumed.tools.is_none() || resumed.tool_choice.is_none() {
            let (inherited_tools, inherited_tool_choice) =
                restore_responses_session_request_options(&state, &request).await;
            if resumed.tools.is_none() {
                resumed.tools = inherited_tools;
            }
            if resumed.tool_choice.is_none() {
                resumed.tool_choice = inherited_tool_choice;
            }
        }
        resumed
    } else {
        request
    };

    // Token 估算和裁剪（在创建 log context 之前）
    // 应用系统提示过滤
    let has_filters = state.config.filter_claude_code
        || state.config.filter_strip_boundaries
        || state.config.filter_env_noise
        || !state.config.prompt_filter_rules.is_empty();
    if has_filters {
        for msg in &mut request.messages {
            if msg.role == "system" {
                if let Some(serde_json::Value::String(text)) = &msg.content {
                    let filtered = crate::gateway::prompt_filter::apply_prompt_filters(&state.config, text);
                    msg.content = Some(serde_json::Value::String(filtered));
                }
            }
        }
    }

    // ===== 响应缓存：查找 =====
    // 仅对非流式请求尝试缓存命中
    let cache_session_id = extract_session_id_from_request(&request).unwrap_or_default();
    let mut cache_enabled_for_request = !request.stream && state.config.response_cache_enabled;
    let messages_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(request.model.as_bytes());
        for msg in &request.messages {
            hasher.update(msg.role.as_bytes());
            if let Some(content) = &msg.content {
                hasher.update(content.to_string().as_bytes());
            }
        }
        if let Some(tools) = &request.tools {
            match serde_json::to_string(tools) {
                Ok(serialized) => hasher.update(serialized.as_bytes()),
                Err(error) => {
                    log::warn!("[响应缓存] 工具列表序列化失败，跳过本次缓存: {error}");
                    cache_enabled_for_request = false;
                }
            }
        }
        format!("{:x}", hasher.finalize())
    };
    let cache_message_count = request.messages.len();
    let cache_total_chars: usize = request.messages.iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.to_string().len())
        .sum();

    if cache_enabled_for_request {
        let mut cache_guard = state.response_cache.lock().await;
        if let Some(cached) = cache_guard.get(
            &cache_session_id,
            &messages_hash,
            cache_message_count,
            cache_total_chars,
        ) {
            drop(cache_guard);
            log::info!(
                "[响应缓存] 命中! session={}, hash={}, 响应长度={}",
                &cache_session_id[..cache_session_id.len().min(16)],
                &messages_hash[..16],
                cached.response.len()
            );

            // 从缓存构建响应
            if let Ok(cached_response) = serde_json::from_str::<Value>(&cached.response) {
                // 记录缓存命中日志
                let cache_log_context = RequestLogContext {
                    request: Some(&request),
                    ..base_log_context.clone()
                };
                write_request_log(
                    &cache_log_context,
                    StatusCode::OK,
                    "cached",
                    None,
                    None,
                    Some(&cached.response),
                    Some(cached.input_tokens),
                    Some(cached.output_tokens),
                    None,
                    None,
                    &state,
                );
                return Json(cached_response).into_response();
            }
            // 缓存内容解析失败，继续正常流程
            log::warn!("[响应缓存] 缓存内容解析失败，走正常请求流程");
        } else {
            drop(cache_guard);
        }
    }

    // 创建 log context
    let request_log_context = RequestLogContext {
        request: Some(&request),
        ..base_log_context.clone()
    };

    let upstream = match resolve_upstream_credentials(&state.config, &state).await {
        Ok(creds) => creds,
        Err(message) => {
            let sanitized = sanitize_error(&message);
                return gateway_error_with_log(
                    &state,
                    format,
                    &request_log_context,
                    GatewayErrorDetails {
                        status: StatusCode::UNAUTHORIZED,
                        error_type: "authentication_error",
                        message: &sanitized,
                        response_body: None,
                    },
                )
                .await;
        }
    };
    let response_id = format!("resp_{}", short_uuid());
    let message_id = format!("msg_{}", short_uuid());
    let created_at = chrono::Utc::now().timestamp();

    let upstream_log_context = RequestLogContext {
        upstream: Some(&upstream),
        ..request_log_context.clone()
    };

    // 获取账号可用模型列表（用于模型降级）
    let available_models = match get_available_models_for_upstream(&upstream).await {
        Ok(models) => {
            log::debug!(
                "[Gateway] 账号 {} 可用模型: {:?}",
                upstream.source_label,
                models
            );
            Some(models)
        }
        Err(e) => {
            log::warn!(
                "[Gateway] 无法获取账号 {} 的可用模型列表: {}，将不进行模型降级",
                upstream.source_label,
                e
            );
            None
        }
    };

    let upstream_payload = match build_kiro_payload(
        &state.http,
        &request,
        upstream.profile_arn.clone(),
        available_models.as_deref(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(message) => {
            let sanitized = sanitize_error(&message);
            return gateway_error_with_log(
                &state,
                format,
                &upstream_log_context,
                GatewayErrorDetails {
                    status: StatusCode::BAD_REQUEST,
                    error_type: "invalid_request_error",
                    message: &sanitized,
                    response_body: None,
                },
            )
            .await;
        }
    };
    
    // 【第二层防护】Payload 大小裁剪（硬限制 - 615KB）
    // 如果 payload 超过 Kiro API 的 HTTP 请求大小限制，自动裁剪历史记录
    let mut payload_value = serde_json::to_value(&upstream_payload)
        .unwrap_or_else(|_| json!({}));

    let original_size = check_payload_size(&payload_value);
    if original_size > MAX_KIRO_PAYLOAD_SIZE {
        log::info!(
            "[网关] Payload 大小 {} 字节超过限制 {} 字节。裁剪历史记录...",
            original_size,
            MAX_KIRO_PAYLOAD_SIZE
        );
        let trimmed = trim_kiro_payload_history(&mut payload_value, MAX_KIRO_PAYLOAD_SIZE);
        if trimmed {
            let final_size = check_payload_size(&payload_value);
            log::info!(
                "[网关] Payload 从 {} 字节裁剪到 {} 字节",
                original_size,
                final_size
            );
        }
    }

    // 方案 3：二次检查 payload 大小，确保裁剪后仍然符合限制
    let mut payload_json = serde_json::to_string(&payload_value)
        .unwrap_or_else(|_| String::new());
    let mut payload_size = payload_json.len();

    if payload_size > MAX_KIRO_PAYLOAD_SIZE {
        log::warn!(
            "[网关] 裁剪后 payload 大小 {} 字节仍超过限制 {} 字节，继续裁剪...",
            payload_size,
            MAX_KIRO_PAYLOAD_SIZE
        );

        // 继续裁剪，直到满足大小限制
        let mut retry_count = 0;
        const MAX_TRIM_RETRIES: u32 = 5;

        while payload_size > MAX_KIRO_PAYLOAD_SIZE && retry_count < MAX_TRIM_RETRIES {
            retry_count += 1;
            let trimmed = trim_kiro_payload_history(&mut payload_value, MAX_KIRO_PAYLOAD_SIZE);

            if !trimmed {
                log::error!(
                    "[网关] 无法继续裁剪 payload（第 {} 次尝试），可能历史记录已为空",
                    retry_count
                );
                break;
            }

            payload_json = serde_json::to_string(&payload_value)
                .unwrap_or_else(|_| String::new());
            let new_size = payload_json.len();

            log::info!(
                "[网关] 第 {} 次裁剪：payload 从 {} 字节减少到 {} 字节",
                retry_count,
                payload_size,
                new_size
            );

            if new_size >= payload_size {
                log::error!(
                    "[网关] 裁剪无效，payload 大小未减少（{} -> {} 字节）",
                    payload_size,
                    new_size
                );
                break;
            }

            payload_size = new_size;
        }

        let final_payload_size = check_payload_size(&payload_value);
        if final_payload_size > MAX_KIRO_PAYLOAD_SIZE {
            log::error!(
                "[网关] 多次裁剪后 payload 大小 {} 字节仍超过限制 {} 字节，拒绝请求",
                final_payload_size,
                MAX_KIRO_PAYLOAD_SIZE
            );
            let message = format!(
                "请求体过大：裁剪后仍有 {final_payload_size} 字节，超过上限 {MAX_KIRO_PAYLOAD_SIZE} 字节"
            );
            return gateway_error_with_log(
                &state,
                format,
                &upstream_log_context,
                GatewayErrorDetails {
                    status: StatusCode::BAD_REQUEST,
                    error_type: "invalid_request_error",
                    message: &message,
                    response_body: None,
                },
            )
            .await;
        }
        log::info!(
            "[网关] 多次裁剪成功，最终 payload 大小 {} 字节",
            final_payload_size
        );
    }

    let upstream_request_body = serde_json::to_string_pretty(&payload_value)
        .unwrap_or_else(|_| "[failed to serialize upstream payload]".to_string());
    let upstream_payload_log_context = RequestLogContext {
        request_body: Some(upstream_request_body.as_str()),
        ..upstream_log_context.clone()
    };

    // 账号重试循环：遇到 429 错误时切换账号
    const MAX_ACCOUNT_RETRIES: u32 = 3;
    let mut account_attempt = 0;
    let mut tried_account_ids: HashSet<String> = HashSet::new();
    
    let (upstream_resp, _conn_lease) = loop {
        account_attempt += 1;
        
        if account_attempt > MAX_ACCOUNT_RETRIES {
            // 所有账号都尝试过了，返回最后一个错误
            return gateway_error_with_log(
                &state,
                format,
                &upstream_payload_log_context,
                GatewayErrorDetails {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    error_type: "rate_limit_error",
                    message: "所有账号均达到速率限制，请稍后再试",
                    response_body: None,
                },
            )
            .await;
        }
        
        // 如果不是第一次尝试，需要重新选择账号
        let current_upstream = if account_attempt > 1 {
            match resolve_upstream_credentials(&state.config, &state).await {
                Ok(creds) => {
                    // 检查是否已经尝试过这个账号
                    let account_id = creds.account_id.clone();
                    if tried_account_ids.contains(&account_id) {
                        log::warn!(
                            "[Gateway] 账号 {} 已尝试过，继续尝试下一个 (尝试: {}/{})",
                            creds.source_label,
                            account_attempt,
                            MAX_ACCOUNT_RETRIES
                        );
                        continue;
                    }
                    tried_account_ids.insert(account_id);
                    creds
                }
                Err(message) => {
                    let sanitized = sanitize_error(&message);
                    log::warn!(
                        "[Gateway] 重新选择账号失败 (尝试: {}/{}): {}",
                        account_attempt,
                        MAX_ACCOUNT_RETRIES,
                        sanitized
                    );
                    // 如果是账号不可用，继续尝试
                    if message.contains("未找到符合反代配置的可用账号") {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        continue;
                    }
                    // 其他错误直接返回
                    return gateway_error_with_log(
                        &state,
                        format,
                        &upstream_payload_log_context,
                        GatewayErrorDetails {
                            status: StatusCode::UNAUTHORIZED,
                            error_type: "authentication_error",
                            message: &sanitized,
                            response_body: None,
                        },
                    )
                    .await;
                }
            }
        } else {
            // 第一次尝试，使用已选择的账号
            tried_account_ids.insert(upstream.account_id.clone());
            upstream.clone()
        };

        // 占用连接租约：真正发送上游请求时才计数，drop 时(成功/失败/流式结束)自动释放
        let conn_lease = state
            .load_balancer
            .lease_connection(&current_upstream.account_id)
            .await;

        // 发送请求
        match send_generate_request(&state.http, &current_upstream, &payload_value).await {
            Ok(resp) => break (resp, conn_lease),
            Err((status, error_type, message, upstream_response_body)) => {
                // 检查是否是 429 错误
                if status == StatusCode::TOO_MANY_REQUESTS {
                    let account_id = current_upstream.account_id.clone();
                    
                    // 标记账号为速率限制
                    state.load_balancer.mark_rate_limited(&account_id).await;
                    state.load_balancer.record_failure(&account_id).await;
                    
                    log::warn!(
                        "[Gateway] 账号 {} 返回 429 错误，标记为速率限制并切换账号 (尝试: {}/{})",
                        current_upstream.source_label,
                        account_attempt,
                        MAX_ACCOUNT_RETRIES
                    );
                    
                    // 继续尝试下一个账号（conn_lease 在此处 drop，释放连接计数）
                    continue;
                }
                
                // 其他错误直接返回
                return gateway_error_with_log(
                    &state,
                    format,
                    &upstream_payload_log_context,
                    GatewayErrorDetails {
                        status,
                        error_type,
                        message: &message,
                        response_body: upstream_response_body.as_deref(),
                    },
                )
                .await;
            }
        }
    };

    if request.stream {
        // 流式开始时不记录日志，等流式结束后再记录完整的 tokens
        // 将 log_context 转换为 'static 生命周期
        let static_log_context = RequestLogContext {
            request_index: upstream_payload_log_context.request_index,
            endpoint: Box::leak(upstream_payload_log_context.endpoint.to_string().into_boxed_str()),
            client_addr: upstream_payload_log_context.client_addr,
            request: None, // 不持有引用
            upstream: None, // 不持有引用
            started_at: upstream_payload_log_context.started_at,
            request_body: None,
            model_hint: upstream_payload_log_context.model_hint.clone(),
            is_stream: Some(true),
        };

        return stream_proxy_response(
            state.clone(),
            upstream_resp,
            format,
            request.model.clone(),
            request.messages.clone(),
            request.tools.clone(),
            request.tool_choice.clone(),
            request.previous_response_id.clone(),
            request.tool_name_map.clone(),
            static_log_context,
            _conn_lease,
        );
    }

    process_nonstream_response(
        state,
        format,
        upstream_resp,
        &request,
        response_id,
        message_id,
        created_at,
        upstream_payload_log_context,
        cache_session_id,
        messages_hash,
        cache_message_count,
        cache_total_chars,
        cache_enabled_for_request,
    )
    .await
}
