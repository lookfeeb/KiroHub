use super::*;

pub(crate) fn convert_openai_chat_messages(messages: Option<&Value>) -> Vec<NormalizedMessage> {
    let Some(Value::Array(items)) = messages else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let role = item.get("role").and_then(Value::as_str)?.to_string();
            let tool_calls = item
                .get("tool_calls")
                .and_then(Value::as_array)
                .map(|calls| {
                    calls
                        .iter()
                        .filter_map(|call| {
                            Some(ToolCall {
                                id: call.get("id").and_then(Value::as_str)?.to_string(),
                                call_type: call
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("function")
                                    .to_string(),
                                function: ToolCallFunction {
                                    name: call
                                        .get("function")?
                                        .get("name")
                                        .and_then(Value::as_str)?
                                        .to_string(),
                                    arguments: call
                                        .get("function")?
                                        .get("arguments")
                                        .and_then(Value::as_str)
                                        .unwrap_or("{}")
                                        .to_string(),
                                },
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|calls| !calls.is_empty());

            let content = item.get("content").map(convert_openai_chat_content);
            Some(NormalizedMessage {
                role,
                content,
                tool_calls,
                tool_call_id: item
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                metadata: None,
            })
        })
        .collect()
}


pub(crate) fn convert_openai_chat_content(content: &Value) -> Value {
    match content {
        Value::String(text) => Value::String(text.clone()),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| {
                    if item.get("type").and_then(Value::as_str) == Some("text") {
                        json!({
                            "type": "input_text",
                            "text": item.get("text").and_then(Value::as_str).unwrap_or_default()
                        })
                    } else {
                        item.clone()
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}


pub(crate) fn convert_openai_chat_tools(tools: Option<&Value>) -> (Option<Vec<Tool>>, std::collections::HashMap<String, String>) {
    convert_responses_tools(tools)
}
