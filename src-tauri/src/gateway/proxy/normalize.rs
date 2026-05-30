// 请求规范化：按 Anthropic / Responses / OpenAI 协议把原始 payload 转为统一请求结构。
use super::*;

pub(crate) fn normalize_request(format: ResponseFormat, payload: &Value) -> Result<NormalizedRequest, String> {
    match format {
        ResponseFormat::Anthropic => {
            let request: AnthropicMessagesRequest = serde_json::from_value(payload.clone())
                .map_err(|error| format!("Anthropic 请求解析失败: {error}"))?;
            Ok(normalize_anthropic_request(&request))
        }
        ResponseFormat::Responses => {
            normalize_responses_request(payload)
        }
        ResponseFormat::OpenAI => {
            let request: OpenAIChatRequest = serde_json::from_value(payload.clone())
                .map_err(|error| format!("OpenAI 请求解析失败: {error}"))?;
            Ok(crate::gateway::converter::normalize_openai_chat_request(&request))
        }
    }
}
