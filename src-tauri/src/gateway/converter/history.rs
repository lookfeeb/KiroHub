use super::*;

/// 修复 history 使其符合 Kiro API 的 7 条验证规则
/// 参考 Kiro IDE 源码中的 v10 函数，按顺序执行修复步骤：
/// 1. 确保以 user 开始
/// 2. 过滤空 user 消息
/// 3. 补充缺失的 toolResults
/// 4. 修复交替（插入占位消息）
/// 5. 确保以 user 结束
pub(crate) fn sanitize_history(mut items: Vec<HistoryItem>) -> Vec<HistoryItem> {
    if items.is_empty() {
        return items;
    }

    // 步骤 1：确保以 user 开始
    if !matches!(items.first(), Some(HistoryItem::User { .. })) {
        items.insert(0, HistoryItem::User {
            user_input_message: HistoryUserMessage {
                content: "Hello".to_string(),
                model_id: String::new(),
                origin: "AI_EDITOR".to_string(),
                images: None,
                user_input_message_context: None,
            },
        });
    }

    // 步骤 2：过滤空 user 消息（保留第一个 user 和有 content/toolResults 的 user）
    let first_user_idx = items.iter().position(|item| matches!(item, HistoryItem::User { .. }));
    items = items.into_iter().enumerate().filter(|(idx, item)| {
        match item {
            HistoryItem::User { user_input_message } => {
                // 保留第一个 user
                if Some(*idx) == first_user_idx {
                    return true;
                }
                // 保留有 content 的 user
                if !user_input_message.content.trim().is_empty() {
                    return true;
                }
                // 保留有 toolResults 的 user
                if let Some(ctx) = &user_input_message.user_input_message_context {
                    if let Some(results) = &ctx.tool_results {
                        if !results.is_empty() {
                            return true;
                        }
                    }
                }
                false
            }
            _ => true,
        }
    }).map(|(_, item)| item).collect();

    // 步骤 3：补充缺失的 toolResults
    // 如果 assistant 有 toolUses 但下一条 user 没有对应 toolResults，插入错误占位
    let mut patched: Vec<HistoryItem> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        patched.push(item.clone());

        if let HistoryItem::Assistant { assistant_response_message } = item {
            if let Some(tool_uses) = &assistant_response_message.tool_uses {
                if !tool_uses.is_empty() {
                    // 检查下一条是否是带 toolResults 的 user
                    let next = items.get(idx + 1);
                    let next_has_results = match next {
                        Some(HistoryItem::User { user_input_message }) => {
                            user_input_message.user_input_message_context
                                .as_ref()
                                .and_then(|ctx| ctx.tool_results.as_ref())
                                .map(|r| !r.is_empty())
                                .unwrap_or(false)
                        }
                        _ => false,
                    };

                    if !next_has_results {
                        // 插入错误占位的 toolResults
                        let error_results: Vec<KiroToolResult> = tool_uses.iter().map(|tu| {
                            KiroToolResult {
                                tool_use_id: tu.tool_use_id.clone(),
                                content: vec![KiroToolResultContent::Text {
                                    text: "Tool execution failed".to_string(),
                                }],
                                status: "error".to_string(),
                            }
                        }).collect();

                        patched.push(HistoryItem::User {
                            user_input_message: HistoryUserMessage {
                                content: String::new(),
                                model_id: String::new(),
                                origin: "AI_EDITOR".to_string(),
                                images: None,
                                user_input_message_context: Some(UserInputMessageContext {
                                    additional_context: None,
                                    app_studio_context: None,
                                    console_state: None,
                                    diagnostic: None,
                                    editor_state: None,
                                    env_state: None,
                                    git_state: None,
                                    shell_state: None,
                                    tool_results: Some(error_results),
                                    tools: None,
                                    user_settings: None,
                                }),
                            },
                        });
                    }
                }
            }
        }
    }
    items = patched;

    // 步骤 4：修复交替（两个连续 user 之间插入 assistant，两个连续 assistant 之间插入 user）
    let mut alternated: Vec<HistoryItem> = Vec::new();
    for item in items {
        if let Some(last) = alternated.last() {
            let both_user = matches!(last, HistoryItem::User { .. }) && matches!(&item, HistoryItem::User { .. });
            let both_assistant = matches!(last, HistoryItem::Assistant { .. }) && matches!(&item, HistoryItem::Assistant { .. });

            if both_user {
                // 插入占位 assistant
                alternated.push(HistoryItem::Assistant {
                    assistant_response_message: HistoryAssistantMessage {
                        content: "understood".to_string(),
                        tool_uses: None,
                        reasoning_content: None,
                        references: None,
                        supplementary_web_links: None,
                        followup_prompt: None,
                        message_id: None,
                        cache_point: None,
                    },
                });
            } else if both_assistant {
                // 插入占位 user
                alternated.push(HistoryItem::User {
                    user_input_message: HistoryUserMessage {
                        content: "Continue".to_string(),
                        model_id: String::new(),
                        origin: "AI_EDITOR".to_string(),
                        images: None,
                        user_input_message_context: None,
                    },
                });
            }
        }
        alternated.push(item);
    }
    items = alternated;

    items
}



pub(crate) fn merge_adjacent_messages(messages: &[&NormalizedMessage]) -> Vec<NormalizedMessage> {
    let mut merged: Vec<NormalizedMessage> = Vec::new();

    for message in messages {
        if let Some(last) = merged.last_mut() {
            if last.role == message.role && last.role != "tool" {
                let existing = extract_text_content(last.content.as_ref());
                let incoming = extract_text_content(message.content.as_ref());
                last.content = Some(Value::String(join_with_newline(&existing, &incoming)));

                match (&mut last.tool_calls, &message.tool_calls) {
                    (Some(existing_calls), Some(next_calls)) => {
                        existing_calls.extend(next_calls.clone())
                    }
                    (None, Some(next_calls)) => last.tool_calls = Some(next_calls.clone()),
                    _ => {}
                }
                if last.tool_call_id.is_none() {
                    last.tool_call_id = message.tool_call_id.clone();
                }
                continue;
            }
        }
        merged.push((*message).clone());
    }

    merged
}


pub(crate) fn build_user_context(
    tools: Option<Vec<KiroTool>>,
    tool_results: Vec<KiroToolResult>,
) -> Option<UserInputMessageContext> {
    if tools.is_none() && tool_results.is_empty() {
        return None;
    }

    Some(UserInputMessageContext {
        additional_context: None,
        app_studio_context: None,
        console_state: None,
        diagnostic: None,
        editor_state: None,
        env_state: None,
        git_state: None,
        shell_state: None,
        tool_results: if tool_results.is_empty() {
            None
        } else {
            Some(tool_results)
        },
        tools,
        user_settings: None,
    })
}


pub(crate) fn images_option(images: Vec<ImageBlock>) -> Option<Vec<ImageBlock>> {
    if images.is_empty() {
        None
    } else {
        Some(images)
    }
}


pub(crate) fn extract_text_content(content: Option<&Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(value @ Value::Array(_)) => {
            extract_text_blocks(value, &["text", "input_text", "output_text"])
        }
        Some(other) => other.to_string(),
    }
}


#[allow(dead_code)]
pub fn normalized_user_message_from_text(text: &str) -> NormalizedMessage {
    NormalizedMessage {
        role: "user".to_string(),
        content: Some(Value::String(text.to_string())),
        tool_calls: None,
        tool_call_id: None,
        metadata: None,
    }
}


#[allow(dead_code)]
pub fn normalized_tool_message_from_output(tool_call_id: &str, output: &str) -> NormalizedMessage {
    NormalizedMessage {
        role: "tool".to_string(),
        content: Some(Value::String(output.to_string())),
        tool_calls: None,
        tool_call_id: Some(tool_call_id.to_string()),
        metadata: None,
    }
}


#[allow(dead_code)]
pub fn history_assistant_message_from_response_content(
    content: &str,
    tool_calls: &[(String, String, String)],
) -> HistoryAssistantMessage {
    let tool_uses = if tool_calls.is_empty() {
        None
    } else {
        Some(
            tool_calls
                .iter()
                .map(|(id, name, arguments)| KiroToolUse {
                    name: name.clone(),
                    input: serde_json::from_str(arguments).unwrap_or_else(|_| json!({})),
                    tool_use_id: id.clone(),
                })
                .collect(),
        )
    };

    HistoryAssistantMessage {
        content: if content.trim().is_empty() {
            "I understand.".to_string()
        } else {
            content.to_string()
        },
        tool_uses,
        reasoning_content: None,
        references: None,
        supplementary_web_links: None,
        followup_prompt: None,
        message_id: None,
        cache_point: None,
    }
}


pub(crate) fn build_history_assistant_message(message: &NormalizedMessage) -> HistoryAssistantMessage {
    let content = extract_text_content(message.content.as_ref());
    let tool_uses = extract_tool_uses(message);
    // Kiro API 要求 assistant content 非空
    let content = if content.trim().is_empty() {
        if tool_uses.is_some() {
            " ".to_string() // 有 toolUses 时用空格占位
        } else {
            "I understand.".to_string()
        }
    } else {
        content
    };
    HistoryAssistantMessage {
        content,
        tool_uses,
        reasoning_content: assistant_metadata_value(message, "reasoningContent")
            .or_else(|| extract_reasoning_content(message.content.as_ref()))
            .and_then(|value| meaningful_optional_value(Some(value)))
            .map(|mut rc| {
                // 清理空 signature（Kiro API 不接受空字符串的 signature）
                if let Some(rt) = rc.get_mut("reasoningText") {
                    if let Some(sig) = rt.get("signature") {
                        if sig.as_str().map(|s| s.is_empty()).unwrap_or(false) {
                            rt.as_object_mut().map(|m| m.remove("signature"));
                        }
                    }
                }
                rc
            }),
        references: assistant_metadata_value(message, "references")
            .and_then(|value| meaningful_optional_value(Some(value))),
        supplementary_web_links: assistant_metadata_value(message, "supplementaryWebLinks")
            .and_then(|value| meaningful_optional_value(Some(value))),
        followup_prompt: assistant_metadata_value(message, "followupPrompt")
            .and_then(|value| meaningful_optional_value(Some(value))),
        message_id: assistant_metadata_value(message, "messageId")
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|value| !value.trim().is_empty()),
        cache_point: assistant_metadata_value(message, "cachePoint")
            .and_then(|value| meaningful_optional_value(Some(value))),
    }
}


pub(crate) fn assistant_metadata_value(message: &NormalizedMessage, key: &str) -> Option<Value> {
    message
        .metadata
        .as_ref()
        .and_then(|value| value.get(key).cloned())
        .or_else(|| {
            message
                .content
                .as_ref()
                .and_then(|value| value.get(key).cloned())
        })
}


/// 判断 reasoning_content 的签名是否为空或缺失
///
/// Kiro API 后端会校验 `reasoningContent.reasoningText.signature`（SHA-256）：
/// - opus-4.7 原生 thinking 会产生有效签名 → 此函数返回 false（保留 reasoningContent）
/// - 其他模型靠 `<thinking_mode>` 提示词强制思考时签名为空字符串
///   → 此函数返回 true（必须从 history 剥掉，否则 400 THINKING_SIGNATURE_INVALID）
pub(crate) fn has_empty_thinking_signature(reasoning_content: &Option<Value>) -> bool {
    let Some(rc) = reasoning_content else {
        return false; // 没有就不需要剥
    };
    // 结构: { reasoningText: { text, signature } } 或 { redactedContent: bytes }
    let signature = rc
        .get("reasoningText")
        .and_then(|rt| rt.get("signature"))
        .and_then(|s| s.as_str());
    match signature {
        None => true,           // 缺 signature 字段
        Some("") => true,       // 空字符串
        Some(_) => false,       // 有值，保留
    }
}


pub(crate) fn extract_reasoning_content(content: Option<&Value>) -> Option<Value> {
    let content = content?;

    if let Some(existing) = content.get("reasoningContent") {
        return meaningful_optional_value(Some(existing.clone()));
    }

    let content_items = content.get("content").unwrap_or(content);
    let Value::Array(items) = content_items else {
        return None;
    };

    let mut texts = Vec::new();
    let mut signature: Option<Value> = None;
    let mut redacted_content: Option<Value> = None;

    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
        if item_type != "reasoning" && item_type != "thinking" {
            continue;
        }

        if let Some(text) = item
            .get("summary")
            .map(|value| extract_text_content(Some(value)))
        {
            if !text.is_empty() {
                texts.push(text);
            }
        } else if let Some(text) = item.get("thinking").and_then(Value::as_str) {
            if !text.is_empty() {
                texts.push(text.to_string());
            }
        } else if let Some(text) = item.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                texts.push(text.to_string());
            }
        }

        if signature.is_none() {
            signature = item.get("signature").cloned();
        }
        if redacted_content.is_none() {
            redacted_content = item.get("redactedContent").cloned();
        }
    }

    if texts.is_empty() && signature.is_none() && redacted_content.is_none() {
        return None;
    }

    let mut reasoning_text = Map::new();
    let merged_text = texts.join("\n");
    if !merged_text.is_empty() {
        reasoning_text.insert("text".to_string(), Value::String(merged_text));
    }
    if let Some(signature) = signature {
        reasoning_text.insert("signature".to_string(), signature);
    }

    let mut reasoning = Map::new();
    if !reasoning_text.is_empty() {
        reasoning.insert("reasoningText".to_string(), Value::Object(reasoning_text));
    }
    if let Some(redacted_content) = redacted_content {
        reasoning.insert("redactedContent".to_string(), redacted_content);
    }

    meaningful_optional_value(Some(Value::Object(reasoning)))
}


pub(crate) fn meaningful_optional_value(value: Option<Value>) -> Option<Value> {
    match value {
        Some(Value::Null) => None,
        Some(Value::String(text)) if text.trim().is_empty() => None,
        Some(Value::Array(items)) if items.is_empty() => None,
        Some(Value::Object(map)) if map.is_empty() => None,
        other => other,
    }
}
