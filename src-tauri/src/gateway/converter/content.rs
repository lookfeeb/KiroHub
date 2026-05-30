use super::*;

pub(crate) fn extract_text_blocks(value: &Value, text_types: &[&str]) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if text_types.contains(&item_type) {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                } else if item_type == "image" {
                    Some("[Image]".to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}


/// 从 Anthropic 的 system/messages 内容中提取文本和 cache_control
///
/// Anthropic 格式：
/// ```json
/// [
///   {"type": "text", "text": "...", "cache_control": {"type": "ephemeral"}},
///   {"type": "text", "text": "..."}
/// ]
/// ```
///
/// 转换为 Kiro 格式的 cache_point：
/// ```json
/// {"type": "default"}
/// ```
pub(crate) fn extract_text_and_cache_control(value: &Value) -> (String, Option<Value>) {
    match value {
        Value::String(text) => (text.clone(), None),
        Value::Array(items) => {
            let mut texts = Vec::new();
            let mut cache_point = None;

            for item in items {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();

                // 提取文本
                if item_type == "text" {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        texts.push(text.to_string());
                    }
                } else if item_type == "image" {
                    texts.push("[Image]".to_string());
                }

                // 提取 cache_control（转换为 cache_point）
                if let Some(cache_control) = item.get("cache_control") {
                    cache_point = Some(convert_cache_control_to_cache_point(cache_control));
                }
            }

            (texts.join("\n"), cache_point)
        }
        Value::Object(map) => {
            let text = map
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let cache_point = map
                .get("cache_control")
                .map(convert_cache_control_to_cache_point);
            (text, cache_point)
        }
        _ => (String::new(), None),
    }
}


/// 从消息内容中提取 cache_control
pub(crate) fn extract_cache_control_from_content(content: &Value) -> Option<Value> {
    match content {
        Value::Array(items) => {
            // 查找最后一个带 cache_control 的内容块
            items
                .iter()
                .rev()
                .find_map(|item| item.get("cache_control"))
                .map(convert_cache_control_to_cache_point)
        }
        Value::Object(obj) => obj
            .get("cache_control")
            .map(convert_cache_control_to_cache_point),
        _ => None,
    }
}


/// 将 Anthropic 的 cache_control 转换为 Kiro 的 cache_point
///
/// Anthropic 格式：
/// ```json
/// {"type": "ephemeral", "ttl": "5m"}  // 或 "1h"
/// ```
///
/// Kiro 格式：
/// ```json
/// {"type": "default"}
/// ```
pub(crate) fn convert_cache_control_to_cache_point(_cache_control: &Value) -> Value {
    // Kiro API 使用简化的 cache_point 格式
    // 不需要 ttl 参数，直接使用 {"type": "default"}
    json!({"type": "default"})
}


pub(crate) fn extract_tool_results(content: Option<&Value>) -> Vec<KiroToolResult> {
    let Some(Value::Array(items)) = content else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
            if item_type != "tool_result" {
                return None;
            }

            let tool_use_id = item
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let content_text = match item.get("content") {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(array)) => {
                    extract_text_blocks(&Value::Array(array.clone()), &["text", "output_text"])
                }
                Some(other) => other.to_string(),
                None => String::new(),
            };
            let status = if item
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "error"
            } else {
                "success"
            };

            Some(KiroToolResult {
                content: vec![KiroToolResultContent::Text { text: content_text }],
                status: status.to_string(),
                tool_use_id,
            })
        })
        .collect()
}


pub(crate) fn extract_tool_results_from_tool_message(message: &NormalizedMessage) -> Vec<KiroToolResult> {
    vec![KiroToolResult {
        content: vec![KiroToolResultContent::Text {
            text: extract_text_content(message.content.as_ref()),
        }],
        status: "success".to_string(),
        tool_use_id: message.tool_call_id.clone().unwrap_or_default(),
    }]
}


pub(crate) fn extract_tool_uses(message: &NormalizedMessage) -> Option<Vec<KiroToolUse>> {
    let tool_calls = message.tool_calls.as_ref()?;
    let tool_uses: Vec<KiroToolUse> = tool_calls
        .iter()
        .map(|tool_call| KiroToolUse {
            name: tool_call.function.name.clone(),
            input: serde_json::from_str(&tool_call.function.arguments)
                .unwrap_or_else(|_| json!({})),
            tool_use_id: tool_call.id.clone(),
        })
        .collect();

    if tool_uses.is_empty() {
        None
    } else {
        Some(tool_uses)
    }
}
