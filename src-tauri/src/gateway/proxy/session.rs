use super::*;

pub(crate) async fn restore_responses_session_messages(
    state: &RouterState,
    request: &NormalizedRequest,
) -> Vec<NormalizedMessage> {
    let Some(mut current_response_id) = request.previous_response_id.clone() else {
        return request.messages.clone();
    };

    let sessions = state.responses_sessions.lock().await;
    let mut chain = Vec::new();
    while let Some(entry) = sessions.get(&current_response_id) {
        chain.push(entry.clone());
        let Some(previous) = entry.previous_response_id.clone() else {
            break;
        };
        current_response_id = previous;
    }
    drop(sessions);

    if chain.is_empty() {
        return request.messages.clone();
    }

    // 收集当前请求中的 tool_result_id，用于过滤最后一轮的 tool_calls
    let current_tool_result_ids: std::collections::HashSet<String> = request
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter_map(|message| message.tool_call_id.clone())
        .collect();

    chain.reverse();
    let mut merged = Vec::new();
    let chain_len = chain.len();
    for (index, entry) in chain.into_iter().enumerate() {
        let is_latest_entry = index + 1 == chain_len;

        // 对最后一轮的 tool_calls 进行过滤：只保留当前请求有对应 tool_result 的
        let effective_tool_calls = if is_latest_entry && !current_tool_result_ids.is_empty() {
            let filtered: Vec<_> = entry
                .tool_calls
                .iter()
                .filter(|(id, _, _)| current_tool_result_ids.contains(id))
                .cloned()
                .collect();
            // 如果过滤后为空（可能是 ID 不匹配），回退到全部
            if filtered.is_empty() {
                entry.tool_calls.clone()
            } else {
                filtered
            }
        } else {
            entry.tool_calls.clone()
        };

        merged.extend(entry.request_messages.clone());
        merged.push(NormalizedMessage {
            role: "assistant".to_string(),
            content: Some(Value::String(entry.response_text.clone())),
            tool_calls: if effective_tool_calls.is_empty() {
                None
            } else {
                Some(
                    effective_tool_calls
                        .iter()
                        .map(|(id, name, arguments)| ToolCall {
                            id: id.clone(),
                            call_type: "function".to_string(),
                            function: ToolCallFunction {
                                name: name.clone(),
                                arguments: if arguments.is_empty() { "{}".to_string() } else { arguments.clone() },
                            },
                        })
                        .collect(),
                )
            },
            tool_call_id: None,
            metadata: None,
        });
    }
    merged.extend(request.messages.clone());
    merged
}


/// 从历史 session 继承 tools 和 tool_choice（Responses API 有状态对话）
///
/// 当客户端使用 previous_response_id 但不重传 tools 时，
/// 需要从历史 session 中继承工具定义。
pub(crate) async fn restore_responses_session_request_options(
    state: &RouterState,
    request: &NormalizedRequest,
) -> (Option<Vec<Tool>>, Option<Value>) {
    let Some(mut current_response_id) = request.previous_response_id.clone() else {
        return (None, None);
    };

    let sessions = state.responses_sessions.lock().await;
    let mut inherited_tools = None;
    let mut inherited_tool_choice = None;

    while let Some(entry) = sessions.get(&current_response_id) {
        if inherited_tools.is_none() {
            inherited_tools = entry.request_tools.clone();
        }
        if inherited_tool_choice.is_none() {
            inherited_tool_choice = entry.request_tool_choice.clone();
        }
        if inherited_tools.is_some() && inherited_tool_choice.is_some() {
            break;
        }
        let Some(previous) = entry.previous_response_id.clone() else {
            break;
        };
        current_response_id = previous;
    }

    (inherited_tools, inherited_tool_choice)
}


pub(crate) async fn persist_responses_session_entry(
    state: &RouterState,
    response_id: &str,
    request_messages: Vec<NormalizedMessage>,
    request_tools: Option<Vec<Tool>>,
    request_tool_choice: Option<Value>,
    previous_response_id: Option<String>,
    aggregated: &stream::AggregatedKiroResponse,
) {
    let mut sessions = state.responses_sessions.lock().await;
    sessions.retain(|_, entry| entry.updated_at.elapsed() < Duration::from_secs(60 * 60));
    sessions.insert(
        response_id.to_string(),
        ResponsesSessionEntry {
            response_id: response_id.to_string(),
            previous_response_id,
            request_messages,
            request_tools,
            request_tool_choice,
            response_text: aggregated.text.clone(),
            tool_calls: aggregated.tool_calls.clone(),
            updated_at: Instant::now(),
        },
    );
}


/// 从请求中提取会话 ID（用于缓存）
pub(crate) fn extract_session_id_from_request(request: &NormalizedRequest) -> Option<String> {
    // 尝试从 previous_response_id 提取会话 ID
    if let Some(prev_id) = &request.previous_response_id {
        // 从 response ID 中提取会话部分（假设格式为 "session_xxx_response_yyy"）
        if let Some(session_part) = prev_id.split('_').nth(1) {
            return Some(format!("session_{}", session_part));
        }
        // 如果格式不匹配，直接使用 previous_response_id 作为会话标识
        return Some(prev_id.clone());
    }

    // 如果没有 previous_response_id，使用消息内容的哈希作为会话标识
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    for msg in &request.messages {
        msg.role.hash(&mut hasher);
        if let Some(content) = &msg.content {
            content.to_string().hash(&mut hasher);
        }
    }
    Some(format!("session_{:x}", hasher.finish()))
}

