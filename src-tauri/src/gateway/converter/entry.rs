use super::*;

pub fn normalize_anthropic_request(request: &AnthropicMessagesRequest) -> NormalizedRequest {
    let mut messages = Vec::new();

    // 处理 system prompt，提取 cache_control
    if let Some(system) = &request.system {
        let (system_text, system_cache_point) = extract_text_and_cache_control(system);
        if !system_text.is_empty() {
            let mut metadata = None;
            if let Some(cache_point) = system_cache_point {
                metadata = Some(json!({"cache_point": cache_point}));
            }
            messages.push(NormalizedMessage {
                role: "system".to_string(),
                content: Some(Value::String(system_text)),
                tool_calls: None,
                tool_call_id: None,
                metadata,
            });
        }
    }

    // 处理消息，提取每条消息中的 cache_control
    for message in &request.messages {
        let cache_point = extract_cache_control_from_content(&message.content);
        let mut metadata = extract_anthropic_message_metadata(message);

        // 如果消息内容中有 cache_control，添加到 metadata
        if let Some(cp) = cache_point {
            let mut meta_obj = metadata.unwrap_or_else(|| json!({}));
            if let Some(obj) = meta_obj.as_object_mut() {
                obj.insert("cache_point".to_string(), cp);
            }
            metadata = Some(meta_obj);
        }

        messages.push(NormalizedMessage {
            role: message.role.clone(),
            content: Some(convert_anthropic_content(&message.content)),
            tool_calls: extract_anthropic_tool_calls(&message.content),
            tool_call_id: extract_anthropic_tool_result_id(&message.content),
            metadata,
        });
    }

    let mut tool_name_map = std::collections::HashMap::new();
    let tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|tool| {
                let (converted_tool, mapping) = convert_anthropic_tool(tool);
                if let Some((sanitized, original)) = mapping {
                    tool_name_map.insert(sanitized, original);
                }
                converted_tool
            })
            .collect()
    });

    let mut normalized = NormalizedRequest {
        model: request.model.clone(),
        messages,
        stream: request.stream,
        max_tokens: Some(request.max_tokens),
        temperature: request.temperature,
        top_p: request.top_p,
        stop: request.stop_sequences.clone(),
        tools,
        tool_choice: request.tool_choice.clone(),
        previous_response_id: None,
        thinking: request.thinking.clone(),
        tool_name_map,
    };

    // 检测模型名是否包含 "thinking" 后缀，若包含则自动启用 thinking
    override_thinking_from_model_name(&mut normalized);

    normalized
}


pub fn normalize_responses_request(payload: &Value) -> Result<NormalizedRequest, String> {
    if payload.get("messages").is_some() && payload.get("input").is_none() {
        return normalize_openai_chat_payload(payload);
    }

    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("claude-sonnet-4-5-20250929")
        .to_string();

    let mut messages = Vec::new();

    if let Some(instructions) = payload.get("instructions") {
        let text = extract_text_blocks(instructions, &["text", "input_text", "output_text"]);
        if !text.is_empty() {
            messages.push(NormalizedMessage {
                role: "system".to_string(),
                content: Some(Value::String(text)),
                tool_calls: None,
                tool_call_id: None,
                metadata: None,
            });
        }
    }

    if let Some(input) = payload.get("input") {
        messages.extend(convert_responses_input(input));
    }

    if messages.is_empty() {
        return Err("Responses 请求缺少可转换的 input".to_string());
    }

    let (tools, tool_name_map) = convert_responses_tools(payload.get("tools"));
    Ok(build_normalized_request_from_payload(
        payload,
        model,
        messages,
        tools,
        tool_name_map,
    ))
}


pub(crate) fn normalize_openai_chat_payload(payload: &Value) -> Result<NormalizedRequest, String> {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("claude-sonnet-4-5-20250929")
        .to_string();

    let messages = convert_openai_chat_messages(payload.get("messages"));
    if messages.is_empty() {
        return Err("chat.completions 请求缺少可转换的 messages".to_string());
    }

    let (tools, tool_name_map) = convert_openai_chat_tools(payload.get("tools"));
    Ok(build_normalized_request_from_payload(
        payload,
        model,
        messages,
        tools,
        tool_name_map,
    ))
}


pub fn normalize_openai_chat_request(request: &OpenAIChatRequest) -> NormalizedRequest {
    let mut messages = Vec::new();
    let mut pending_tool_results = Vec::new();

    for msg in &request.messages {
        match msg.role.as_str() {
            "system" => {
                let text = extract_text_content(Some(&msg.content));
                if !text.is_empty() {
                    messages.push(NormalizedMessage {
                        role: "system".to_string(),
                        content: Some(Value::String(text)),
                        tool_calls: None,
                        tool_call_id: None,
                        metadata: None,
                    });
                }
            }
            "tool" => {
                let content = extract_text_content(Some(&msg.content));
                let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
                pending_tool_results.push((tool_call_id, content));
            }
            "user" | "assistant" => {
                if !pending_tool_results.is_empty() {
                    messages.push(create_tool_results_message(&pending_tool_results));
                    pending_tool_results.clear();
                }

                let tool_calls = if msg.role == "assistant" {
                    msg.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| ToolCall {
                                id: tc.id.clone(),
                                call_type: tc.call_type.clone(),
                                function: ToolCallFunction {
                                    name: tc.function.name.clone(),
                                    arguments: tc.function.arguments.to_string(),
                                },
                            })
                            .collect()
                    })
                } else {
                    None
                };

                messages.push(NormalizedMessage {
                    role: msg.role.clone(),
                    content: Some(msg.content.clone()),
                    tool_calls,
                    tool_call_id: None,
                    metadata: None,
                });
            }
            _ => {}
        }
    }

    if !pending_tool_results.is_empty() {
        messages.push(create_tool_results_message(&pending_tool_results));
    }

    let mut tool_name_map = std::collections::HashMap::new();
    let tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| {
                let original_name = t.function.name.clone();
                let sanitized_name = shorten_tool_name(&sanitize_tool_name(&original_name));

                if sanitized_name != original_name {
                    tool_name_map.insert(sanitized_name.clone(), original_name);
                }

                Tool {
                    tool_type: t.tool_type.clone(),
                    function: ToolFunction {
                        name: sanitized_name,
                        description: t.function.description.clone(),
                        parameters: t.function.parameters.clone(),
                    },
                    cache_control: None,
                }
            })
            .collect()
    });

    NormalizedRequest {
        model: request.model.clone(),
        messages,
        stream: request.stream,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        top_p: request.top_p,
        stop: request.stop.clone(),
        tools,
        tool_choice: request.tool_choice.clone(),
        previous_response_id: None,
        thinking: None,
        tool_name_map,
    }
}


pub(crate) fn create_tool_results_message(tool_results: &[(String, String)]) -> NormalizedMessage {
    let mut content_array = Vec::new();
    for (tool_call_id, content) in tool_results {
        content_array.push(json!({
            "type": "tool_result",
            "tool_use_id": tool_call_id,
            "content": content
        }));
    }

    NormalizedMessage {
        role: "user".to_string(),
        content: Some(Value::Array(content_array)),
        tool_calls: None,
        tool_call_id: None,
        metadata: None,
    }
}


pub(crate) fn build_normalized_request_from_payload(
    payload: &Value,
    model: String,
    messages: Vec<NormalizedMessage>,
    tools: Option<Vec<Tool>>,
    tool_name_map: std::collections::HashMap<String, String>,
) -> NormalizedRequest {
    NormalizedRequest {
        model,
        messages,
        stream: payload
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(true),  // 默认使用流式响应
        max_tokens: payload
            .get("max_output_tokens")
            .or_else(|| payload.get("max_tokens"))
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        temperature: payload
            .get("temperature")
            .and_then(Value::as_f64)
            .map(|value| value as f32),
        top_p: payload
            .get("top_p")
            .and_then(Value::as_f64)
            .map(|value| value as f32),
        stop: payload.get("stop").and_then(|value| match value {
            Value::String(item) => Some(vec![item.to_string()]),
            Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        }),
        tools,
        tool_choice: payload.get("tool_choice").cloned(),
        previous_response_id: payload
            .get("previous_response_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        thinking: None,
        tool_name_map,
    }
}
