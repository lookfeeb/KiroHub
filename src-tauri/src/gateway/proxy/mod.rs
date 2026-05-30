

use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::Local;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    core::account::{Account, AccountStore},
    commands::common::{
        is_token_expiring_soon,
        refresh_token_by_provider, resolve_default_profile_arn, update_account_status, RefreshResult,
    },
    commands::machine_guid::get_machine_id,
    clients::{
        http_client::{
            build_kiro_custom_user_agent, build_q_service_url,
            resolve_kiro_upstream_region, should_add_redirect_for_internal,
            should_send_codewhisperer_optout,
        },
        kiro_q_client::KiroQClient,
    },
};

const MAX_FAILURES_PER_ACCOUNT: u32 = 3;
const MAX_KIRO_PAYLOAD_SIZE: usize = 450 * 1024; // 450KB - Kiro API 的 HTTP 请求大小限制（更保守）

// Token 限制的默认值（当无法从 API 获取时使用）
#[allow(dead_code)]
const SUMMARIZATION_THRESHOLD_PERCENT: f64 = 0.55; // 55% 触发裁剪（预留更多安全空间，避免 Kiro IDE 上下文导致超限）
const COUNT_TOKENS_SAFETY_MULTIPLIER: f64 = 1.15;

use super::{
    append_gateway_request_log,
    converter::{
        build_kiro_payload, get_available_models,
        normalize_anthropic_request, normalize_responses_request,
    },
    eventstream::decode_message,
    effective_client_api_keys,
    models::{
        AnthropicContentBlock, AnthropicMessagesRequest, AnthropicMessagesResponse, AnthropicUsage,
        ModelsResponse, NormalizedMessage, NormalizedRequest, OpenAIChatRequest, Tool, ToolCall,
        ToolCallFunction,
    },
    stream::{self, parse_kiro_event_full, KiroEvent},
    thinking_parser::{SegmentType, ThinkingParser},
    GatewayConfig, GatewayRequestLogEntry, ResponseFormat, ResponsesSessionEntry, RouterState,
    DEFAULT_AGENT_MODE,
};


mod types;
mod session;
mod response_build;
mod tokens;
mod upstream;
mod guard;
mod normalize;
mod handlers;
mod nonstream;
mod logging;
mod errors;
mod streaming;
mod sse;

pub(crate) use types::*;
pub(crate) use session::*;
pub(crate) use response_build::*;
pub(crate) use tokens::*;
pub(crate) use upstream::*;
pub(crate) use guard::*;
pub(crate) use normalize::*;
pub(crate) use handlers::*;
pub(crate) use nonstream::*;
pub(crate) use logging::*;
pub(crate) use errors::*;
pub(crate) use streaming::*;
pub(crate) use sse::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::token_cache::TokenCache;
    use serde_json::json;
    use std::sync::{
        atomic::AtomicU64,
        Arc,
    };
    use tokio::sync::Mutex as AsyncMutex;

    fn proxy_test_state() -> RouterState {
        RouterState {
            config: GatewayConfig {
                access_token: Some("sk-test".to_string()),
                account_mode: "single".to_string(),
                account_id: Some("test-account".to_string()),
                ..GatewayConfig::default()
            },
            request_count: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(AsyncMutex::new(None)),
            http: Client::new(),
            responses_sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            token_cache: Arc::new(AsyncMutex::new(TokenCache::new())),
            load_balancer: Arc::new(crate::gateway::load_balancer::LoadBalancer::new(
                crate::gateway::load_balancer::LoadBalancerStrategy::RoundRobin,
            )),
            log_store: Arc::new(crate::gateway::log_store::LogStore::new(1000)),
            response_cache: Arc::new(AsyncMutex::new(
                crate::gateway::response_cache::ResponseCache::new(
                    crate::gateway::response_cache::CacheConfig::default(),
                    None,
                ),
            )),
        }
    }

    #[test]
    fn normalize_request_accepts_openai_chat_payloads() {
        let responses_payload = json!({
            "model": "claude-3-7-sonnet-20250219",
            "stream": true,
            "previous_response_id": "resp_prev_123",
            "tool_choice": { "type": "function", "name": "search_docs" },
            "tools": [
                {
                    "type": "function",
                    "name": "search_docs",
                    "description": "搜索文档",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "q": { "type": "string" }
                        },
                        "required": ["q"]
                    }
                }
            ],
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "先检索 gateway" }
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "search_docs",
                    "arguments": "{\"q\":\"gateway\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "命中结果"
                }
            ]
        });

        let chat_payload = json!({
            "model": "claude-3-7-sonnet-20250219",
            "stream": true,
            "tool_choice": { "type": "function", "name": "search_docs" },
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "search_docs",
                        "description": "搜索文档",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "q": { "type": "string" }
                            },
                            "required": ["q"]
                        }
                    }
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "先检索 gateway" }
                    ]
                },
                {
                    "role": "assistant",
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "search_docs",
                                "arguments": "{\"q\":\"gateway\"}"
                            }
                        }
                    ]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_1",
                    "content": "命中结果"
                }
            ]
        });

        let responses_request = normalize_request(ResponseFormat::Responses, &responses_payload)
            .expect("responses payload should normalize");
        let chat_request = normalize_request(ResponseFormat::Responses, &chat_payload)
            .expect("chat payload should normalize through the OpenAI protocol adapter");

        assert_eq!(responses_request.model, "claude-3-7-sonnet-20250219");
        assert!(responses_request.stream);
        assert_eq!(
            responses_request.previous_response_id.as_deref(),
            Some("resp_prev_123")
        );
        assert_eq!(
            responses_request.tool_choice,
            Some(json!({ "type": "function", "name": "search_docs" }))
        );
        assert_eq!(responses_request.tools.as_ref().map(Vec::len), Some(1));
        assert_eq!(responses_request.tools.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            responses_request
                .tools
                .as_ref()
                .and_then(|items| items.first())
                .map(|tool| tool.function.name.as_str()),
            Some("searchDocs")
        );
        assert_eq!(responses_request.messages.len(), 3);
        assert_eq!(
            responses_request.messages[1]
                .tool_calls
                .as_ref()
                .and_then(|items| items.first())
                .map(|call| &call.function.arguments),
            Some(&"{\"q\":\"gateway\"}".to_string())
        );
        assert_eq!(
            responses_request.messages[2].content,
            Some(json!("命中结果"))
        );
        assert_eq!(chat_request.model, responses_request.model);
        assert_eq!(chat_request.stream, responses_request.stream);
        assert_eq!(chat_request.tool_choice, responses_request.tool_choice);
        assert_eq!(chat_request.tools.as_ref().map(Vec::len), Some(1));
        assert_eq!(chat_request.messages.len(), responses_request.messages.len());
        assert_eq!(
            chat_request.messages[1]
                .tool_calls
                .as_ref()
                .and_then(|items| items.first())
                .map(|call| &call.function.arguments),
            Some(&"{\"q\":\"gateway\"}".to_string())
        );
        assert_eq!(chat_request.messages[2].content, Some(json!("命中结果")));
    }

    #[test]
    fn test_tokenizer_type_from_model_id() {
        assert!(matches!(
            TokenizerType::from_model_id("claude-3-7-sonnet-20250219"),
            TokenizerType::Claude
        ));
        assert!(matches!(
            TokenizerType::from_model_id("gpt-4"),
            TokenizerType::OpenAI
        ));
        assert!(matches!(
            TokenizerType::from_model_id("o1-preview"),
            TokenizerType::OpenAI
        ));
        assert!(matches!(
            TokenizerType::from_model_id("llama-3-70b"),
            TokenizerType::Llama
        ));
        assert!(matches!(
            TokenizerType::from_model_id("unknown-model"),
            TokenizerType::Generic
        ));
    }

    #[test]
    fn test_estimate_text_tokens_claude() {
        let text = "Hello, world!";
        let tokens = estimate_text_tokens(text, TokenizerType::Claude);
        assert_eq!(tokens, (text.len() + 3) / 4);
    }

    #[test]
    fn test_estimate_text_tokens_llama() {
        let text = "Hello, world!";
        let tokens = estimate_text_tokens(text, TokenizerType::Llama);
        assert_eq!(tokens, ((text.len() as f64 / 3.5).ceil() as usize).max(1));
    }

    #[test]
    fn test_estimate_text_tokens_generic() {
        let text = "Hello\nWorld\n```rust\nfn main() {}\n```";
        let tokens = estimate_text_tokens(text, TokenizerType::Generic);

        let base_tokens = (text.len() + 3) / 4;
        let lines = text.lines().count();
        let newline_tokens = (lines + 1) / 2;
        let code_blocks = text.matches("```").count();
        let code_block_tokens = code_blocks * 2;
        let expected = base_tokens + newline_tokens + code_block_tokens;

        assert_eq!(tokens, expected);
    }

    #[test]
    fn test_estimate_request_tokens() {
        let messages = vec![
            NormalizedMessage {
                role: "user".to_string(),
                content: Some(json!("Hello, how are you?")),
                tool_calls: None,
                tool_call_id: None,
                metadata: None,
            },
            NormalizedMessage {
                role: "assistant".to_string(),
                content: Some(json!("I'm doing well, thank you!")),
                tool_calls: None,
                tool_call_id: None,
                metadata: None,
            },
        ];

        let tokens = estimate_request_tokens(&messages, "claude-3-7-sonnet-20250219");
        assert!(tokens > 0);
    }

    #[test]
    fn test_check_payload_size() {
        let payload = json!({
            "model": "claude-3-7-sonnet-20250219",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });

        let size = check_payload_size(&payload);
        assert!(size > 0);
    }

    #[test]
    fn test_trim_kiro_payload_history_removes_oldest_messages() {
        let mut payload = json!({
            "conversationState": {
                "history": [
                    {
                        "user_input_message": {
                            "user_input_message_context": {
                                "text": "First message"
                            }
                        }
                    },
                    {
                        "assistant_response_message": {
                            "text": "First response"
                        }
                    },
                    {
                        "user_input_message": {
                            "user_input_message_context": {
                                "text": "Second message"
                            }
                        }
                    },
                    {
                        "assistant_response_message": {
                            "text": "Second response"
                        }
                    }
                ]
            }
        });

        let max_bytes = 100;
        let trimmed = trim_kiro_payload_history(&mut payload, max_bytes);

        assert!(trimmed);
        let history = payload
            .pointer("/conversationState/history")
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(history.len() < 4);
        assert!(history.len() >= 2);
    }

    #[test]
    fn test_trim_kiro_payload_history_preserves_tool_call_pairs() {
        let mut payload = json!({
            "conversationState": {
                "history": [
                    {
                        "assistant_response_message": {
                            "text": "Let me search for that",
                            "tool_uses": [
                                {
                                    "id": "call_1",
                                    "name": "search",
                                    "input": {"q": "test"}
                                }
                            ]
                        }
                    },
                    {
                        "user_input_message": {
                            "user_input_message_context": {
                                "tool_results": [
                                    {
                                        "call_id": "call_1",
                                        "output": "Found results"
                                    }
                                ]
                            }
                        }
                    },
                    {
                        "user_input_message": {
                            "user_input_message_context": {
                                "text": "Recent message"
                            }
                        }
                    }
                ]
            }
        });

        let max_bytes = 200;
        let trimmed = trim_kiro_payload_history(&mut payload, max_bytes);

        if trimmed {
            let history = payload
                .pointer("/conversationState/history")
                .and_then(|v| v.as_array())
                .unwrap();

            if history.len() == 1 {
                assert!(history[0].get("user_input_message").is_some());
            }
        }
    }

    #[tokio::test]
    async fn test_get_model_max_input_tokens() {
        assert_eq!(get_model_max_input_tokens("auto").await, 1_000_000);
        assert_eq!(get_model_max_input_tokens("claude-3-7-sonnet-20250219").await, 200_000);
        assert_eq!(get_model_max_input_tokens("gpt-4").await, 200_000);
        assert_eq!(get_model_max_input_tokens("deepseek-chat").await, 128_000);
        assert_eq!(get_model_max_input_tokens("llama-3-70b").await, 128_000);
        assert_eq!(get_model_max_input_tokens("unknown-model").await, 200_000);
    }

    #[tokio::test]
    async fn restore_responses_session_messages_replays_previous_assistant_turn() {
        let state = proxy_test_state();
        {
            let mut sessions = state.responses_sessions.lock().await;
            sessions.insert(
                "resp_prev_123".to_string(),
                ResponsesSessionEntry {
                    response_id: "resp_prev_123".to_string(),
                    previous_response_id: None,
                    request_messages: vec![NormalizedMessage {
                        role: "user".to_string(),
                        content: Some(json!("第一问")),
                        tool_calls: None,
                        tool_call_id: None,
                        metadata: None,
                    }],
                    response_text: "第一答".to_string(),
                    tool_calls: vec![(
                        "call_1".to_string(),
                        "search_docs".to_string(),
                        "{\"q\":\"gateway\"}".to_string(),
                    )],
                    request_tools: None,
                    request_tool_choice: None,
                    updated_at: Instant::now(),
                },
            );
        }

        let request = NormalizedRequest {
            model: "claude-sonnet-4-5".to_string(),
            messages: vec![NormalizedMessage {
                role: "user".to_string(),
                content: Some(json!("第二问")),
                tool_calls: None,
                tool_call_id: None,
                metadata: None,
            }],
            stream: false,
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
            previous_response_id: Some("resp_prev_123".to_string()),
            thinking: None,
            tool_name_map: std::collections::HashMap::new(),
        };

        let merged = restore_responses_session_messages(&state, &request).await;

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].role, "user");
        assert_eq!(merged[1].role, "assistant");
        assert_eq!(merged[2].role, "user");
        assert_eq!(merged[1].content, Some(json!("第一答")));
        assert_eq!(
            merged[1]
                .tool_calls
                .as_ref()
                .and_then(|items| items.first())
                .map(|call| call.function.name.as_str()),
            Some("search_docs")
        );
    }

    #[test]
    fn verify_client_auth_accepts_any_configured_client_api_key() {
        let config = GatewayConfig {
            access_token: Some("sk-primary".to_string()),
            client_api_keys: vec!["sk-primary".to_string(), "sk-secondary".to_string()],
            ..GatewayConfig::default()
        };

        let mut bearer_headers = HeaderMap::new();
        bearer_headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-secondary"),
        );
        assert!(verify_client_auth(&bearer_headers, &config).is_ok());

        let mut x_api_key_headers = HeaderMap::new();
        x_api_key_headers.insert("x-api-key", HeaderValue::from_static("sk-primary"));
        assert!(verify_client_auth(&x_api_key_headers, &config).is_ok());

        let mut invalid_headers = HeaderMap::new();
        invalid_headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-unknown"),
        );
        assert!(verify_client_auth(&invalid_headers, &config).is_err());
    }

    #[test]
    fn detect_upstream_error_body_maps_success_status_error_payloads() {
        let error = detect_upstream_error_body(
            r#"{"error":{"message":"Invalid model. Please select a different model to continue.","type":"invalid_request_error"}}"#,
        )
        .expect("error payload should be detected");

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert_eq!(error.1, "invalid_request_error");
        assert!(error.2.contains("Invalid model"));
    }

    #[test]
    fn build_responses_response_emits_kiro_citation_annotations() {
        let aggregated = stream::AggregatedKiroResponse {
            text: "Hello Rust".to_string(),
            thinking: String::new(),
            thinking_signature: None,
            tool_calls: Vec::new(),
            input_tokens: 3,
            output_tokens: 5,
            context_usage_percentage: None,
            citations: vec![stream::AggregatedCitation {
                text: Some("Rust".to_string()),
                link: "https://example.com/rust".to_string(),
                target: json!({ "range": { "start": 6, "end": 10 } }),
            }],
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            metering_usage: None,
        };

        let response = build_responses_response_with_ids(
            "gpt-5.4",
            &aggregated,
            "resp_test",
            "msg_test",
            123,
            None,
        );

        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["type"],
            "url_citation"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["start_index"],
            6
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["end_index"],
            10
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["url"],
            "https://example.com/rust"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["citationText"],
            "Rust"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["citationLink"],
            "https://example.com/rust"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["target"]["range"]["start"],
            6
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["target"]["range"]["end"],
            10
        );
        assert!(response["output"][0]["content"][0]["annotations"][0]["title"].is_null());
    }

    #[test]
    fn build_responses_response_omits_guessed_range_for_location_citations() {
        let aggregated = stream::AggregatedKiroResponse {
            text: "Hello Rust".to_string(),
            thinking: String::new(),
            thinking_signature: None,
            tool_calls: Vec::new(),
            input_tokens: 3,
            output_tokens: 5,
            context_usage_percentage: None,
            citations: vec![stream::AggregatedCitation {
                text: Some("Rust".to_string()),
                link: "https://example.com/rust".to_string(),
                target: json!({ "location": 6 }),
            }],
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            metering_usage: None,
        };

        let response = build_responses_response_with_ids(
            "gpt-4.1",
            &aggregated,
            "resp_test",
            "msg_test",
            123,
            None,
        );

        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["type"],
            "url_citation"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["citationText"],
            "Rust"
        );
        assert_eq!(
            response["output"][0]["content"][0]["annotations"][0]["target"]["location"],
            6
        );
        assert!(response["output"][0]["content"][0]["annotations"][0]["start_index"].is_null());
        assert!(response["output"][0]["content"][0]["annotations"][0]["end_index"].is_null());
        assert!(response["output"][0]["content"][0]["annotations"][0]["title"].is_null());
    }

    #[test]
    fn build_anthropic_response_maps_kiro_citations_into_sdk_shape() {
        let aggregated = stream::AggregatedKiroResponse {
            text: "Hello Rust".to_string(),
            thinking: String::new(),
            thinking_signature: None,
            tool_calls: Vec::new(),
            input_tokens: 3,
            output_tokens: 5,
            context_usage_percentage: None,
            citations: vec![stream::AggregatedCitation {
                text: Some("Rust".to_string()),
                link: "https://example.com/rust".to_string(),
                target: json!({ "range": { "start": 6, "end": 10 } }),
            }],
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            metering_usage: None,
        };

        let response = build_anthropic_response("claude-sonnet-4-5", &aggregated);

        assert_eq!(response["content"][0]["type"], "text");
        assert_eq!(
            response["content"][0]["citations"][0]["type"],
            "char_location"
        );
        assert_eq!(
            response["content"][0]["citations"][0]["start_char_index"],
            6
        );
        assert_eq!(response["content"][0]["citations"][0]["end_char_index"], 10);
        assert_eq!(response["content"][0]["citations"][0]["cited_text"], "Rust");
        assert_eq!(
            response["content"][0]["citations"][0]["document_title"],
            "https://example.com/rust"
        );
        assert!(response["content"][0]["citations"][0]["file_id"].is_null());
    }

    #[test]
    fn build_stream_responses_completed_event_keeps_citations_and_tool_calls() {
        let aggregated = stream::AggregatedKiroResponse {
            text: "Hello Rust".to_string(),
            thinking: String::new(),
            thinking_signature: None,
            tool_calls: vec![(
                "call_1".to_string(),
                "search_docs".to_string(),
                "{\"q\":\"rust\"}".to_string(),
            )],
            input_tokens: 3,
            output_tokens: 5,
            context_usage_percentage: None,
            citations: vec![stream::AggregatedCitation {
                text: Some("Rust".to_string()),
                link: "https://example.com/rust".to_string(),
                target: json!({ "range": { "start": 6, "end": 10 } }),
            }],
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            metering_usage: None,
        };

        let event = build_stream_responses_completed_event(
            "gpt-4.1",
            &aggregated,
            "resp_test",
            "msg_test",
            123,
            None,
        );

        assert_eq!(event["type"], "response.completed");
        assert_eq!(event["response"]["output_text"], "Hello Rust");
        assert_eq!(
            event["response"]["output"][0]["content"][0]["annotations"][0]["citationText"],
            "Rust"
        );
        assert!(event["response"]["output"][0]["content"][0]["annotations"][0]["title"].is_null());
        assert_eq!(
            event["response"]["output"][0]["content"][1]["type"],
            "function_call"
        );
        assert_eq!(
            event["response"]["output"][0]["content"][1]["call_id"],
            "call_1"
        );
    }

    #[test]
    fn build_stream_responses_done_events_use_expected_shape() {
        let function_done = build_stream_responses_function_call_arguments_done_event(
            "resp_test",
            "call_1",
            "{\"q\":\"rust\"}",
        );
        let text_done = build_stream_responses_output_text_done_event("resp_test", "Hello Rust");
        let reasoning_done = build_stream_responses_reasoning_done_event("resp_test", "Think");

        assert_eq!(function_done["type"], "response.function_call_arguments.done");
        assert_eq!(function_done["response_id"], "resp_test");
        assert_eq!(function_done["call_id"], "call_1");
        assert_eq!(function_done["arguments"], "{\"q\":\"rust\"}");

        assert_eq!(text_done["type"], "response.output_text.done");
        assert_eq!(text_done["response_id"], "resp_test");
        assert_eq!(text_done["text"], "Hello Rust");

        assert_eq!(reasoning_done["type"], "response.reasoning.done");
        assert_eq!(reasoning_done["response_id"], "resp_test");
        assert_eq!(reasoning_done["text"], "Think");
    }

    #[test]
    fn with_kiro_upstream_headers_adds_generate_request_headers() {
        let upstream = UpstreamCredentials {
            access_token: "token-1".to_string(),
            profile_arn: None,
            provider: None,
            region: "us-east-1".to_string(),
            account_id: "test".to_string(),
            source_label: "single:test".to_string(),
            user_agent: "KiroIDE 0.11.34 machine-123".to_string(),
            auth_method: Some("external_idp".to_string()),
            send_opt_out: true,
        };

        let request = with_kiro_upstream_headers(
            reqwest::Client::new()
                .post("https://q.us-east-1.amazonaws.com/generateAssistantResponse"),
            &upstream,
            "application/vnd.amazon.eventstream",
            true,
            true,
            false,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token-1")
        );
        assert_eq!(
            request
                .headers()
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("KiroIDE 0.11.34 machine-123")
        );
        assert_eq!(
            request
                .headers()
                .get("x-amz-user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("KiroIDE 0.11.34 machine-123")
        );
        assert_eq!(
            request
                .headers()
                .get("x-amzn-codewhisperer-optout")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        assert_eq!(
            request
                .headers()
                .get("x-amzn-kiro-agent-mode")
                .and_then(|value| value.to_str().ok()),
            Some(DEFAULT_AGENT_MODE)
        );
        // TokenType header 已移除（会导致某些接口 403 错误）
        assert!(request.headers().get("TokenType").is_none());
        assert!(request.headers().get("x-amzn-kiro-profile-arn").is_none());
        assert!(request.headers().get("redirect-for-internal").is_none());
    }

    #[test]
    fn with_kiro_upstream_headers_keeps_runtime_requests_minimal() {
        let upstream = UpstreamCredentials {
            access_token: "token-2".to_string(),
            profile_arn: None,
            provider: None,
            region: "us-east-1".to_string(),
            account_id: "test".to_string(),
            source_label: "single:test".to_string(),
            user_agent: "KiroIDE 0.11.34 machine-456".to_string(),
            auth_method: Some("social".to_string()),
            send_opt_out: true,
        };

        let request = with_kiro_upstream_headers(
            reqwest::Client::new()
                .get("https://q.us-east-1.amazonaws.com/ListAvailableModels?origin=AI_EDITOR"),
            &upstream,
            "application/json",
            false,
            false,
            false,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("x-amz-user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("KiroIDE 0.11.34 machine-456")
        );
        assert!(request
            .headers()
            .get("x-amzn-codewhisperer-optout")
            .is_none());
        assert!(request.headers().get("x-amzn-kiro-agent-mode").is_none());
        assert!(request.headers().get("TokenType").is_none());
        assert!(request.headers().get("x-amzn-kiro-profile-arn").is_none());
        assert!(request.headers().get("redirect-for-internal").is_none());
    }

    #[test]
    fn with_kiro_upstream_headers_adds_mcp_profile_arn_header() {
        let upstream = UpstreamCredentials {
            access_token: "token-3".to_string(),
            profile_arn: Some(
                "arn:aws:codewhisperer:us-east-1:123456789012:profile/test".to_string(),
            ),
            provider: None,
            region: "us-east-1".to_string(),
            account_id: "test".to_string(),
            source_label: "single:test".to_string(),
            user_agent: "KiroIDE 0.11.34 machine-789".to_string(),
            auth_method: Some("social".to_string()),
            send_opt_out: true,
        };

        let request = with_kiro_upstream_headers(
            reqwest::Client::new().post("https://q.us-east-1.amazonaws.com/mcp"),
            &upstream,
            "application/json",
            false,
            false,
            true,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("x-amzn-kiro-profile-arn")
                .and_then(|value| value.to_str().ok()),
            Some("arn:aws:codewhisperer:us-east-1:123456789012:profile/test")
        );
        assert!(request.headers().get("redirect-for-internal").is_none());
    }

    #[test]
    fn with_kiro_upstream_headers_adds_redirect_for_internal_only_for_internal_provider() {
        let upstream = UpstreamCredentials {
            access_token: "token-4".to_string(),
            profile_arn: None,
            provider: Some("Internal".to_string()),
            region: "us-east-1".to_string(),
            account_id: "test".to_string(),
            source_label: "single:test".to_string(),
            user_agent: "KiroIDE 0.11.34 machine-999".to_string(),
            auth_method: Some("IdC".to_string()),
            send_opt_out: true,
        };

        let request = with_kiro_upstream_headers(
            reqwest::Client::new()
                .post("https://q.us-east-1.amazonaws.com/generateAssistantResponse"),
            &upstream,
            "application/vnd.amazon.eventstream",
            true,
            true,
            false,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("redirect-for-internal")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
    }

    #[test]
    fn with_kiro_upstream_headers_does_not_add_redirect_for_enterprise_or_builderid() {
        for provider in ["Enterprise", "BuilderId"] {
            let upstream = UpstreamCredentials {
                access_token: "token-5".to_string(),
                profile_arn: None,
                provider: Some(provider.to_string()),
                region: "us-east-1".to_string(),
                account_id: "test".to_string(),
            source_label: "single:test".to_string(),
                user_agent: "KiroIDE 0.11.34 machine-1000".to_string(),
                auth_method: Some("IdC".to_string()),
                send_opt_out: true,
            };

            let request = with_kiro_upstream_headers(
                reqwest::Client::new()
                    .post("https://q.us-east-1.amazonaws.com/generateAssistantResponse"),
                &upstream,
                "application/vnd.amazon.eventstream",
                true,
                true,
                false,
            )
            .build()
            .expect("request should build");

            assert!(
                request.headers().get("redirect-for-internal").is_none(),
                "provider {provider} should not add redirect-for-internal"
            );
        }
    }
}

