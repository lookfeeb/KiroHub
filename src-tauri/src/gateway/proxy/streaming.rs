use super::*;

pub(crate) fn stream_proxy_response(
    state: RouterState,
    upstream_resp: reqwest::Response,
    format: ResponseFormat,
    model: String,
    request_messages: Vec<NormalizedMessage>,
    request_tools: Option<Vec<Tool>>,
    request_tool_choice: Option<Value>,
    previous_response_id: Option<String>,
    tool_name_map: std::collections::HashMap<String, String>,
    log_context: RequestLogContext<'static>,
    conn_lease: crate::gateway::load_balancer::ConnectionLease,
) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(2048);
    tokio::spawn(async move {
        // 持有连接租约直到流结束，drop 时自动释放连接计数
        let _conn_lease = conn_lease;
        // 辅助函数：还原工具名称（sanitized -> original）
        let restore_tool_name = |sanitized: &str| -> String {
            tool_name_map.get(sanitized).cloned().unwrap_or_else(|| sanitized.to_string())
        };
        let mut upstream_stream = upstream_resp.bytes_stream();
        let mut raw_buffer = Vec::new();
        let mut parser = ThinkingParser::new();
        let mut aggregated = stream::AggregatedKiroResponse::default();
        let mut tool_accumulators: HashMap<String, (String, String)> = HashMap::new();
        let mut input_tokens = 0i32;
        let mut output_tokens = 0i32;
        let mut message_started = false;
        let mut next_block_index = 0usize;
        let mut text_block_index: Option<usize> = None;
        let mut thinking_block_index: Option<usize> = None;
        let mut tool_block_indexes: HashMap<String, usize> = HashMap::new();
        let mut openai_tool_call_indexes: HashMap<String, i32> = HashMap::new();
        let mut openai_next_tool_index = 0i32;
        let mut saw_tool_calls = false;
        let anthropic_id = format!("msg_{}", short_uuid());
        let response_id = format!("resp_{}", short_uuid());
        let message_id = format!("msg_{}", short_uuid());
        let created_at = chrono::Utc::now().timestamp();
        let completion_id = format!("chatcmpl-{}", short_uuid());
        let mut responses_sequence_number = 0usize;
        let mut responses_next_output_index = 1usize;
        let mut responses_tool_output_indexes: HashMap<String, usize> = HashMap::new();

        if matches!(format, ResponseFormat::Responses) {
            let created = json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "created_at": created_at,
                    "status": "in_progress",
                    "model": model,
                    "output": []
                }
            });
            if !send_data(&tx, &created.to_string()).await {
                return;
            }

            let output_item_added = json!({
                "type": "response.output_item.added",
                "response_id": response_id,
                "output_index": 0,
                "item": {
                    "id": message_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": []
                }
            });
            if !send_data(&tx, &output_item_added.to_string()).await {
                return;
            }
        } else if matches!(format, ResponseFormat::OpenAI) {
            let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
            let created = chrono::Utc::now().timestamp();
            let delta = crate::gateway::models::OpenAIChatDelta {
                role: Some("assistant".to_string()),
                content: Some("".to_string()),
                tool_calls: None,
            };
            let chunk = stream::build_openai_chunk(
                &completion_id,
                created,
                &model,
                delta,
                None,
                None,
            );
            if let Ok(chunk_json) = serde_json::to_string(&chunk) {
                if !send_data(&tx, &chunk_json).await {
                    return;
                }
            }
        }

        const STALLED_STREAM_TIMEOUT: tokio::time::Duration =
            tokio::time::Duration::from_secs(300);

        loop {
            let chunk_result = match tokio::time::timeout(
                STALLED_STREAM_TIMEOUT,
                upstream_stream.next(),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => break,
                Err(_) => {
                    log::error!("流式响应超时: 5分钟内未收到数据");
                    let data = json!({
                        "type": "error",
                        "message": "流式响应超时: 5分钟内未收到数据"
                    });
                    send_data(&tx, &data.to_string()).await;
                    break;
                }
            };

            match chunk_result {
                Ok(bytes) => {
                    // 累积二进制数据
                    raw_buffer.extend_from_slice(&bytes);
                    // 逐个解码 EventStream 消息
                    loop {
                        match decode_message(&raw_buffer) {
                            Ok(Some((msg, consumed_bytes))) => {
                                // 成功解码一个消息
                                let message_type = msg.headers.get(":message-type").map(String::as_str);
                                let event_type = msg.headers.get(":event-type").map(String::as_str);

                                if matches!(message_type, Some("error") | Some("exception")) {
                                    let error_text = String::from_utf8_lossy(&msg.payload);
                                    log::error!(
                                        "EventStream 上游错误: message_type={:?}, event_type={:?}, payload={}",
                                        message_type,
                                        event_type,
                                        error_text
                                    );
                                    let data = json!({
                                        "type": "error",
                                        "message": sanitize_error(error_text.as_ref())
                                    });
                                    send_data(&tx, &data.to_string()).await;
                                    raw_buffer.drain(..consumed_bytes);
                                    break;
                                }

                                if !matches!(message_type, Some("event")) {
                                    raw_buffer.drain(..consumed_bytes);
                                    continue;
                                }

                                // 将 payload 转换为文本
                                let json_text = String::from_utf8_lossy(&msg.payload);

                                // 记录每个 Kiro API 事件（trace 级别，避免刷屏）
                                log::trace!("[Kiro API 响应事件] {}", json_text);

                                // 解析 JSON 事件
                                if let Some(event) = parse_kiro_event_full(&json_text) {
                                    match event {
                                        KiroEvent::Usage {
                                            input_tokens: input,
                                            output_tokens: output,
                                            cache_read_input_tokens,
                                            cache_creation_input_tokens,
                                        } => {
                                            log::info!(
                                                "[Stream] ✅ Received Usage event: input={}, output={}, cache_read={:?}, cache_write={:?}",
                                                input,
                                                output,
                                                cache_read_input_tokens,
                                                cache_creation_input_tokens
                                            );
                                            input_tokens = input;
                                            output_tokens = output;
                                            aggregated.input_tokens = input;
                                            aggregated.output_tokens = output;
                                            aggregated.cache_read_input_tokens = cache_read_input_tokens;
                                            aggregated.cache_creation_input_tokens = cache_creation_input_tokens;
                                        }
                                        KiroEvent::ContextUsage { percentage } => {
                                            aggregated.context_usage_percentage = Some(percentage);
                                            if matches!(format, ResponseFormat::Anthropic) {
                                                let data =
                                                    json!({"type":"context_usage","percentage":percentage});
                                                send_event(
                                                    &tx,
                                                    Some("context_usage"),
                                                    &data.to_string(),
                                                )
                                                .await;
                                            }
                                        }
                                        KiroEvent::Thinking(text) => {
                                            aggregated.thinking.push_str(&text);
                                            handle_stream_text(
                                                &tx,
                                                format,
                                                &model,
                                                &anthropic_id,
                                                &response_id,
                                                &completion_id,
                                                created_at,
                                                &text,
                                                true,
                                                &mut message_started,
                                                &mut next_block_index,
                                                &mut text_block_index,
                                                &mut thinking_block_index,
                                                input_tokens,
                                                output_tokens,
                                                aggregated.cache_read_input_tokens,
                                                aggregated.cache_creation_input_tokens,
                                            )
                                            .await;
                                        }
                                        KiroEvent::ThinkingSignature(sig) => {
                                            aggregated.thinking_signature = Some(sig);
                                        }
                                        KiroEvent::Text(text) => {
                                            aggregated.text.push_str(&text);
                                            for segment in parser.push_and_parse(&text) {
                                                handle_stream_text(
                                                    &tx,
                                                    format,
                                                    &model,
                                                    &anthropic_id,
                                                    &response_id,
                                                    &completion_id,
                                                    created_at,
                                                    &segment.content,
                                                    segment.segment_type == SegmentType::Thinking,
                                                    &mut message_started,
                                                    &mut next_block_index,
                                                    &mut text_block_index,
                                                    &mut thinking_block_index,
                                                    input_tokens,
                                                    output_tokens,
                                                    aggregated.cache_read_input_tokens,
                                                    aggregated.cache_creation_input_tokens,
                                                )
                                                .await;
                                            }
                                        }
                                        KiroEvent::ToolUseStart { id, name } => {
                                            saw_tool_calls = true;
                                            // 还原工具名称
                                            let original_name = restore_tool_name(&name);
                                            tool_accumulators
                                                .entry(id.clone())
                                                .or_insert((original_name.clone(), String::new()));
                                            match format {
                                                ResponseFormat::Anthropic => {
                                                    ensure_anthropic_message_start(
                                                        &tx,
                                                        &mut message_started,
                                                        &anthropic_id,
                                                        &model,
                                                        aggregated.input_tokens,
                                                        aggregated.output_tokens,
                                                        aggregated.cache_read_input_tokens,
                                                        aggregated.cache_creation_input_tokens,
                                                    )
                                                    .await;
                                                    close_content_block(&tx, &mut text_block_index)
                                                        .await;
                                                    close_content_block(
                                                        &tx,
                                                        &mut thinking_block_index,
                                                    )
                                                    .await;
                                                    let index = next_block_index;
                                                    next_block_index += 1;
                                                    tool_block_indexes.insert(id.clone(), index);
                                                    let data = json!({
                                                        "type": "content_block_start",
                                                        "index": index,
                                                        "content_block": {
                                                            "type": "tool_use",
                                                            "id": id,
                                                            "name": name,
                                                            "input": {}
                                                        }
                                                    });
                                                    send_event(
                                                        &tx,
                                                        Some("content_block_start"),
                                                        &data.to_string(),
                                                    )
                                                    .await;
                                                }
                                                ResponseFormat::Responses => {
                                                    let output_index = responses_next_output_index;
                                                    responses_next_output_index += 1;
                                                    responses_tool_output_indexes
                                                        .insert(id.clone(), output_index);
                                                    let data = json!({
                                                        "type": "response.output_item.added",
                                                        "response_id": response_id,
                                                        "output_index": output_index,
                                                        "item": {
                                                            "id": id,
                                                            "type": "function_call",
                                                            "status": "in_progress",
                                                            "call_id": id,
                                                            "name": name,
                                                            "arguments": ""
                                                        }
                                                    });
                                                    send_data(&tx, &data.to_string()).await;
                                                }
                                                ResponseFormat::OpenAI => {
                                                    // OpenAI Chat Completions: 发送工具调用开始 chunk
                                                    let tool_index = openai_next_tool_index;
                                                    openai_next_tool_index += 1;
                                                    openai_tool_call_indexes.insert(id.clone(), tool_index);
                                                    
                                                    let chunk = stream::build_openai_chunk(
                                                        &completion_id,
                                                        created_at,
                                                        &model,
                                                        crate::gateway::models::OpenAIChatDelta {
                                                            role: None,
                                                            content: None,
                                                            tool_calls: Some(vec![
                                                                crate::gateway::models::OpenAIDeltaToolCall {
                                                                    index: tool_index,
                                                                    id: id.clone(),
                                                                    call_type: "function".to_string(),
                                                                    function: crate::gateway::models::OpenAIToolCallFunction {
                                                                        name: name.clone(),
                                                                        arguments: "".to_string(),
                                                                    },
                                                                }
                                                            ]),
                                                        },
                                                        None,
                                                        None,
                                                    );
                                                    if let Ok(chunk_json) = serde_json::to_string(&chunk) {
                                                        send_data(&tx, &chunk_json).await;
                                                    }
                                                }
                                            }
                                        }
                                        KiroEvent::ToolUseInputDelta { id, name, input_delta } => {
                                            // 当 input delta 先于 start 到达时（Kiro 流可能乱序），
                                            // 用 delta 中携带的 name 主动发起 start 事件，避免客户端卡死
                                            let mut started_from_delta = false;
                                            if let Some((existing_name, current_input)) =
                                                tool_accumulators.get_mut(&id)
                                            {
                                                if existing_name.is_empty() {
                                                    if let Some(n) = name.as_ref() {
                                                        *existing_name = restore_tool_name(n);
                                                    }
                                                }
                                                current_input.push_str(&input_delta);
                                            } else {
                                                let resolved_name = name
                                                    .as_ref()
                                                    .map(|n| restore_tool_name(n))
                                                    .unwrap_or_default();
                                                tool_accumulators.insert(
                                                    id.clone(),
                                                    (resolved_name, input_delta.clone()),
                                                );
                                                started_from_delta = true;
                                            }

                                            // 如果是 delta 先到，并且携带了 name，则补发 start 事件
                                            if started_from_delta {
                                                if let Some(raw_name) = name.as_ref() {
                                                    let original_name = restore_tool_name(raw_name);
                                                    saw_tool_calls = true;
                                                    match format {
                                                        ResponseFormat::Anthropic => {
                                                            if !tool_block_indexes.contains_key(&id) {
                                                                ensure_anthropic_message_start(
                                                                    &tx,
                                                                    &mut message_started,
                                                                    &anthropic_id,
                                                                    &model,
                                                                    aggregated.input_tokens,
                                                                    aggregated.output_tokens,
                                                                    aggregated.cache_read_input_tokens,
                                                                    aggregated.cache_creation_input_tokens,
                                                                )
                                                                .await;
                                                                close_content_block(
                                                                    &tx,
                                                                    &mut text_block_index,
                                                                )
                                                                .await;
                                                                close_content_block(
                                                                    &tx,
                                                                    &mut thinking_block_index,
                                                                )
                                                                .await;
                                                                let index = next_block_index;
                                                                next_block_index += 1;
                                                                tool_block_indexes
                                                                    .insert(id.clone(), index);
                                                                let data = json!({
                                                                    "type": "content_block_start",
                                                                    "index": index,
                                                                    "content_block": {
                                                                        "type": "tool_use",
                                                                        "id": id,
                                                                        "name": original_name,
                                                                        "input": {}
                                                                    }
                                                                });
                                                                send_event(
                                                                    &tx,
                                                                    Some("content_block_start"),
                                                                    &data.to_string(),
                                                                )
                                                                .await;
                                                            }
                                                        }
                                                        ResponseFormat::Responses => {
                                                            if !responses_tool_output_indexes
                                                                .contains_key(&id)
                                                            {
                                                                let output_index =
                                                                    responses_next_output_index;
                                                                responses_next_output_index += 1;
                                                                responses_tool_output_indexes
                                                                    .insert(id.clone(), output_index);
                                                                let data = json!({
                                                                    "type": "response.output_item.added",
                                                                    "response_id": response_id,
                                                                    "output_index": output_index,
                                                                    "item": {
                                                                        "id": id,
                                                                        "type": "function_call",
                                                                        "status": "in_progress",
                                                                        "call_id": id,
                                                                        "name": original_name,
                                                                        "arguments": ""
                                                                    }
                                                                });
                                                                send_data(&tx, &data.to_string()).await;
                                                            }
                                                        }
                                                        ResponseFormat::OpenAI => {
                                                            if !openai_tool_call_indexes
                                                                .contains_key(&id)
                                                            {
                                                                let tool_index = openai_next_tool_index;
                                                                openai_next_tool_index += 1;
                                                                openai_tool_call_indexes
                                                                    .insert(id.clone(), tool_index);
                                                                let chunk = stream::build_openai_chunk(
                                                                    &completion_id,
                                                                    created_at,
                                                                    &model,
                                                                    crate::gateway::models::OpenAIChatDelta {
                                                                        role: None,
                                                                        content: None,
                                                                        tool_calls: Some(vec![
                                                                            crate::gateway::models::OpenAIDeltaToolCall {
                                                                                index: tool_index,
                                                                                id: id.clone(),
                                                                                call_type: "function".to_string(),
                                                                                function: crate::gateway::models::OpenAIToolCallFunction {
                                                                                    name: original_name.clone(),
                                                                                    arguments: "".to_string(),
                                                                                },
                                                                            }
                                                                        ]),
                                                                    },
                                                                    None,
                                                                    None,
                                                                );
                                                                if let Ok(chunk_json) =
                                                                    serde_json::to_string(&chunk)
                                                                {
                                                                    send_data(&tx, &chunk_json).await;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            // 不再立即转发片段，避免客户端收到不完整的 JSON（参考 Kiro-Go）
                                        }
                                        KiroEvent::ToolUseStop { id } => match format {
                                            ResponseFormat::Anthropic => {
                                                // 在 ToolUseStop 时，一次性发送完整的 input（参考 Kiro-Go）
                                                if let Some((name, input)) =
                                                    tool_accumulators.remove(&id)
                                                {
                                                    aggregated.tool_calls.push((id.clone(), name, input.clone()));

                                                    // 发送完整的 input_json_delta
                                                    if let Some(index) = tool_block_indexes.get(&id).copied() {
                                                        if !input.is_empty() {
                                                            let data = json!({
                                                                "type": "content_block_delta",
                                                                "index": index,
                                                                "delta": {
                                                                    "type": "input_json_delta",
                                                                    "partial_json": input
                                                                }
                                                            });
                                                            send_event(
                                                                &tx,
                                                                Some("content_block_delta"),
                                                                &data.to_string(),
                                                            )
                                                            .await;
                                                        }
                                                    }
                                                }
                                                if let Some(index) = tool_block_indexes.remove(&id) {
                                                    let data = json!({
                                                        "type": "content_block_stop",
                                                        "index": index
                                                    });
                                                    send_event(
                                                        &tx,
                                                        Some("content_block_stop"),
                                                        &data.to_string(),
                                                    )
                                                    .await;
                                                }
                                            }
                                            ResponseFormat::Responses => {
                                                if let Some((name, input)) =
                                                    tool_accumulators.remove(&id)
                                                {
                                                    aggregated.tool_calls.push((
                                                        id.clone(),
                                                        name.clone(),
                                                        input.clone(),
                                                    ));
                                                    let done = build_stream_responses_function_call_arguments_done_event(
                                                        &response_id,
                                                        &id,
                                                        &input,
                                                    );
                                                    send_data(&tx, &done.to_string()).await;
                                                    let output_index = responses_tool_output_indexes
                                                        .remove(&id)
                                                        .unwrap_or_else(|| {
                                                            let idx = responses_next_output_index;
                                                            responses_next_output_index += 1;
                                                            idx
                                                        });
                                                    let data = json!({
                                                        "type": "response.output_item.done",
                                                        "response_id": response_id,
                                                        "output_index": output_index,
                                                        "item": {
                                                            "id": id,
                                                            "type": "function_call",
                                                            "status": "completed",
                                                            "call_id": id,
                                                            "name": name,
                                                            "arguments": input
                                                        }
                                                    });
                                                    send_data(&tx, &data.to_string()).await;
                                                }
                                            }
                                            ResponseFormat::OpenAI => {
                                                if let Some((name, input)) =
                                                    tool_accumulators.remove(&id)
                                                {
                                                    aggregated.tool_calls.push((
                                                        id.clone(),
                                                        name.clone(),
                                                        input.clone(),
                                                    ));

                                                    // OpenAI 格式：在 ToolUseStop 时发送完整的 arguments
                                                    if let Some(&tool_index) = openai_tool_call_indexes.get(&id) {
                                                        let chunk = stream::build_openai_chunk(
                                                            &completion_id,
                                                            created_at,
                                                            &model,
                                                            crate::gateway::models::OpenAIChatDelta {
                                                                role: None,
                                                                content: None,
                                                                tool_calls: Some(vec![
                                                                    crate::gateway::models::OpenAIDeltaToolCall {
                                                                        index: tool_index,
                                                                        id: "".to_string(),
                                                                        call_type: "function".to_string(),
                                                                        function: crate::gateway::models::OpenAIToolCallFunction {
                                                                            name: "".to_string(),
                                                                            arguments: input,
                                                                        },
                                                                    }
                                                                ]),
                                                            },
                                                            None,
                                                            None,
                                                        );
                                                        if let Ok(chunk_json) = serde_json::to_string(&chunk) {
                                                            send_data(&tx, &chunk_json).await;
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        KiroEvent::Citation { text, link, target } => {
                                            let citation =
                                                stream::AggregatedCitation { text, link, target };
                                            aggregated.citations.push(citation.clone());

                                            match format {
                                                ResponseFormat::Anthropic => {
                                                    ensure_anthropic_message_start(
                                                        &tx,
                                                        &mut message_started,
                                                        &anthropic_id,
                                                        &model,
                                                        aggregated.input_tokens,
                                                        aggregated.output_tokens,
                                                        aggregated.cache_read_input_tokens,
                                                        aggregated.cache_creation_input_tokens,
                                                    )
                                                    .await;
                                                    close_content_block(
                                                        &tx,
                                                        &mut thinking_block_index,
                                                    )
                                                    .await;
                                                    if text_block_index.is_none() {
                                                        let index = next_block_index;
                                                        next_block_index += 1;
                                                        text_block_index = Some(index);
                                                        let data = json!({
                                                            "type": "content_block_start",
                                                            "index": index,
                                                            "content_block": {
                                                                "type": "text",
                                                                "text": ""
                                                            }
                                                        });
                                                        send_event(
                                                            &tx,
                                                            Some("content_block_start"),
                                                            &data.to_string(),
                                                        )
                                                        .await;
                                                    }
                                                    if let Some(index) = text_block_index {
                                                        if let Some(data) =
                                                            build_anthropic_citation_delta_event(
                                                                index,
                                                                &citation,
                                                                &aggregated.text,
                                                            )
                                                        {
                                                            send_event(
                                                                &tx,
                                                                Some("content_block_delta"),
                                                                &data.to_string(),
                                                            )
                                                            .await;
                                                        }
                                                    }
                                                }
                                                ResponseFormat::Responses => {
                                                    if let Some(annotation) =
                                                        build_responses_citation_annotations(
                                                            std::slice::from_ref(&citation),
                                                        )
                                                        .into_iter()
                                                        .next()
                                                    {
                                                        let data = build_responses_annotation_added_event(
                                                            &response_id,
                                                            &message_id,
                                                            annotation,
                                                            aggregated.citations.len() - 1,
                                                            responses_sequence_number,
                                                        );
                                                        responses_sequence_number += 1;
                                                        send_data(&tx, &data.to_string()).await;
                                                    }
                                                }
                                                ResponseFormat::OpenAI => {
                                                    // OpenAI Chat Completions stream should not emit
                                                    // Responses API events like response.annotation.added.
                                                    // Citations are not part of the Chat Completions API.
                                                }
                                            }
                                        }
                                        KiroEvent::Metering { unit, unit_plural, usage } => {
                                            // 记录 metering 信息到聚合响应
                                            aggregated.metering_usage = Some(usage);
                                            
                                            // 如果是 Anthropic 格式，发送 metering 事件
                                            if matches!(format, ResponseFormat::Anthropic) {
                                                let data = json!({
                                                    "type": "metering",
                                                    "unit": unit,
                                                    "unitPlural": unit_plural,
                                                    "usage": usage
                                                });
                                                send_event(&tx, Some("metering"), &data.to_string()).await;
                                            }
                                        }
                                    }
                                }

                                // 清理已处理的字节
                                raw_buffer.drain(..consumed_bytes);
                            }
                            Ok(None) => {
                                // 缓冲区数据不足，等待更多数据
                                break;
                            }
                            Err(error) => {
                                // 解码失败，记录错误并清空缓冲区
                                log::error!("EventStream 解码失败: {}", error);
                                raw_buffer.clear();
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    log::error!("流式读取错误: {:?}", error);
                    let error_msg = format!("流式读取失败: {error}");
                    log::error!("错误详情: {}", error_msg);
                    let data = json!({"type":"error","message":sanitize_error(&error_msg)});
                    send_data(&tx, &data.to_string()).await;
                    break;
                }
            }
        }

        for segment in parser.flush() {
            handle_stream_text(
                &tx,
                format,
                &model,
                &anthropic_id,
                &response_id,
                &completion_id,
                created_at,
                &segment.content,
                segment.segment_type == SegmentType::Thinking,
                &mut message_started,
                &mut next_block_index,
                &mut text_block_index,
                &mut thinking_block_index,
                input_tokens,
                output_tokens,
                aggregated.cache_read_input_tokens,
                aggregated.cache_creation_input_tokens,
            )
            .await;
        }
        // 收集未关闭的工具调用（没有收到 stop 事件的），不要直接 push 到 aggregated.tool_calls
        // 因为 Anthropic 末尾分支需要区分"已正常 stop"和"未 stop"的，避免重复发送事件
        let unstopped_tools: Vec<(String, String, String)> = tool_accumulators
            .drain()
            .filter(|(_, (name, input))| !name.is_empty() || !input.is_empty())
            .map(|(id, (name, input))| {
                log::warn!("[流式] 收集未关闭的工具调用: id={}, name={}", id, name);
                (id, name, input)
            })
            .collect();
        for tool in &unstopped_tools {
            aggregated.tool_calls.push(tool.clone());
        }
        aggregated.tool_calls = stream::deduplicate_tool_calls(aggregated.tool_calls);

        // 流结束后打印完整聚合响应
        log::debug!(
            "[流式响应完成] model={}, text_len={}, thinking_len={}, tool_calls={}, input_tokens={}, output_tokens={}, text_preview={:.200}",
            model,
            aggregated.text.len(),
            aggregated.thinking.len(),
            aggregated.tool_calls.len(),
            aggregated.input_tokens,
            aggregated.output_tokens,
            aggregated.text
        );

        // 流式结束后，使用本地估算 token（在发送响应之前）
        if aggregated.input_tokens == 0 || aggregated.output_tokens == 0 {
            log::info!("[流式] 响应中没有 token 信息，使用本地估算");

            // 估算输入 tokens（从请求消息中）
            let request_text = serde_json::to_string(&request_messages).unwrap_or_else(|error| {
                log::warn!("[流式] 请求消息序列化失败，token 估算退回空输入: {error}");
                String::new()
            });
            aggregated.input_tokens = crate::gateway::token_estimator::estimate_tokens(&request_text, &model);

            // 估算输出 tokens（从响应文本中）
            let response_text = format!("{}{}", aggregated.text, aggregated.thinking);
            aggregated.output_tokens = crate::gateway::token_estimator::estimate_tokens(&response_text, &model);

            log::info!(
                "[流式] 估算的 tokens: input={}, output={} (model={})",
                aggregated.input_tokens,
                aggregated.output_tokens,
                model
            );
        } else {
            log::info!(
                "[流式] 使用响应中的 token 信息: input={}, output={}",
                aggregated.input_tokens,
                aggregated.output_tokens
            );
        }

        // Prompt Cache 模拟：如果响应中没有缓存信息，用模拟器填充
        if aggregated.cache_read_input_tokens.is_none() && aggregated.cache_creation_input_tokens.is_none() {
            let tracker = crate::gateway::prompt_cache::global_prompt_cache_tracker();
            let messages_json: Vec<serde_json::Value> = request_messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect();
            let tools_json: Option<Vec<serde_json::Value>> = request_tools.as_ref().map(|tools| {
                tools.iter().map(|t| serde_json::to_value(t).unwrap_or_default()).collect()
            });

            if let Some(profile) = tracker.build_profile(
                None,
                &messages_json,
                tools_json.as_deref(),
                aggregated.input_tokens as usize,
                &model,
            ) {
                let account_id = model.as_str();
                let cache_usage = tracker.compute(account_id, &profile);
                tracker.update(account_id, &profile);

                if cache_usage.cache_read_input_tokens > 0 {
                    aggregated.cache_read_input_tokens = Some(cache_usage.cache_read_input_tokens as i32);
                }
                if cache_usage.cache_creation_input_tokens > 0 {
                    aggregated.cache_creation_input_tokens = Some(cache_usage.cache_creation_input_tokens as i32);
                }

                log::info!(
                    "[流式] Prompt Cache 模拟: read={}, creation={}",
                    cache_usage.cache_read_input_tokens,
                    cache_usage.cache_creation_input_tokens
                );
            }
        }

        match format {
            ResponseFormat::Anthropic => {
                close_content_block(&tx, &mut text_block_index).await;
                close_content_block(&tx, &mut thinking_block_index).await;

                // 只处理"未收到 stop 事件"的工具调用，避免重复发送已经在流中正常 stop 过的
                for (id, name, input) in &unstopped_tools {
                    // 如果之前已经发过 content_block_start（delta 先到时），直接补 delta+stop
                    let block_index = if let Some(idx) = tool_block_indexes.remove(id) {
                        idx
                    } else {
                        let idx = next_block_index;
                        next_block_index += 1;
                        let start = json!({
                            "type": "content_block_start",
                            "index": idx,
                            "content_block": {
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": {}
                            }
                        });
                        send_event(&tx, Some("content_block_start"), &start.to_string()).await;
                        idx
                    };
                    let parsed_input =
                        crate::gateway::converter::parse_tool_arguments(input, "proxy.streaming");
                    let delta = json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": serde_json::to_string(&parsed_input).unwrap_or_else(|_| "{}".to_string())
                        }
                    });
                    send_event(&tx, Some("content_block_delta"), &delta.to_string()).await;
                    let stop = json!({
                        "type": "content_block_stop",
                        "index": block_index
                    });
                    send_event(&tx, Some("content_block_stop"), &stop.to_string()).await;
                    saw_tool_calls = true;
                }

                // 兜底关闭：如果 tool_block_indexes 还有遗留（理论上 unstopped_tools 已经覆盖，
                // 但万一有 start 事件发了但既没 stop 也没在 unstopped_tools 里），统一发 stop
                for (_, idx) in tool_block_indexes.drain() {
                    let stop = json!({
                        "type": "content_block_stop",
                        "index": idx
                    });
                    send_event(&tx, Some("content_block_stop"), &stop.to_string()).await;
                }

                let mut usage = json!({
                    "input_tokens": aggregated.input_tokens,
                    "output_tokens": aggregated.output_tokens
                });

                // 添加 cache token 信息（如果存在）
                if let Some(cache_read) = aggregated.cache_read_input_tokens {
                    usage["cache_read_input_tokens"] = json!(cache_read);
                }
                if let Some(cache_creation) = aggregated.cache_creation_input_tokens {
                    usage["cache_creation_input_tokens"] = json!(cache_creation);
                }

                let finish = json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": if saw_tool_calls { "tool_use" } else { "end_turn" },
                        "stop_sequence": Value::Null
                    },
                    "usage": usage
                });
                send_event(&tx, Some("message_delta"), &finish.to_string()).await;
                send_event(&tx, Some("message_stop"), "{\"type\":\"message_stop\"}").await;
            }
            ResponseFormat::Responses => {
                let output_text = build_responses_output_text(&aggregated);
                if !output_text.text.is_empty() {
                    let text_done = build_stream_responses_output_text_done_event(
                        &response_id,
                        &output_text.text,
                    );
                    send_data(&tx, &text_done.to_string()).await;
                }
                if !aggregated.thinking.is_empty() {
                    let reasoning_done = build_stream_responses_reasoning_done_event(
                        &response_id,
                        &aggregated.thinking,
                    );
                    send_data(&tx, &reasoning_done.to_string()).await;
                }
                let content = build_responses_message_content(&aggregated);
                let output_item_done = json!({
                    "type": "response.output_item.done",
                    "response_id": response_id,
                    "output_index": 0,
                    "item": {
                        "id": message_id,
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": content
                    }
                });
                send_data(&tx, &output_item_done.to_string()).await;

                let completed = build_stream_responses_completed_event(
                    &model,
                    &aggregated,
                    &response_id,
                    &message_id,
                    created_at,
                    previous_response_id.as_deref(),
                );
                send_data(&tx, &completed.to_string()).await;
                persist_responses_session_entry(
                    &state,
                    &response_id,
                    request_messages.clone(),
                    request_tools.clone(),
                    request_tool_choice.clone(),
                    previous_response_id.clone(),
                    &aggregated,
                )
                .await;
                send_data(&tx, "[DONE]").await;
            }
            ResponseFormat::OpenAI => {
                // OpenAI Chat Completions: 工具调用已在流式过程中增量发送
                // 这里只需要发送最终 chunk（带 finish_reason 和 usage）
                let finish_reason = if saw_tool_calls { "tool_calls" } else { "stop" };
                let final_chunk = stream::build_openai_chunk(
                    &completion_id,
                    created_at,
                    &model,
                    crate::gateway::models::OpenAIChatDelta {
                        role: None,
                        content: None,
                        tool_calls: None,
                    },
                    Some(finish_reason.to_string()),
                    Some(crate::gateway::models::OpenAIChatUsage {
                        prompt_tokens: aggregated.input_tokens,
                        completion_tokens: aggregated.output_tokens,
                        total_tokens: aggregated.input_tokens + aggregated.output_tokens,
                    }),
                );
                match serde_json::to_string(&final_chunk) {
                    Ok(final_json) => {
                        let _ = send_data(&tx, &final_json).await;
                    }
                    Err(error) => log::error!("[流式] 序列化最终 OpenAI chunk 失败: {error}"),
                }
                send_data(&tx, "[DONE]").await;
            }
        }

        // 记录请求日志（token 已经在发送响应前估算好了）
        let response_body_log = if aggregated.text.is_empty() {
            None
        } else {
            Some(aggregated.text.clone())
        };

        write_request_log(
            &log_context,
            StatusCode::OK,
            "stream",
            None,
            None, // error_type
            response_body_log.as_deref(),
            Some(aggregated.input_tokens),
            Some(aggregated.output_tokens),
            aggregated.cache_read_input_tokens,
            aggregated.cache_creation_input_tokens,
            &state,
        );
    }); // tokio::spawn 闭合

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .header(header::CONNECTION, HeaderValue::from_static("keep-alive"))
        .body(Body::from_stream(ReceiverStream::new(rx)))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}
