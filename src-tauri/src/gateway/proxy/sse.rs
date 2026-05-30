use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_stream_text(
    tx: &mpsc::Sender<Result<Bytes, Infallible>>,
    format: ResponseFormat,
    model: &str,
    anthropic_id: &str,
    response_id: &str,
    completion_id: &str,
    created: i64,
    text: &str,
    is_thinking: bool,
    message_started: &mut bool,
    next_block_index: &mut usize,
    text_block_index: &mut Option<usize>,
    thinking_block_index: &mut Option<usize>,
    input_tokens: i32,
    output_tokens: i32,
    cache_read_input_tokens: Option<i32>,
    cache_creation_input_tokens: Option<i32>,
) {
    if text.is_empty() {
        return;
    }

    match format {
        ResponseFormat::Anthropic => {
            ensure_anthropic_message_start(
                tx,
                message_started,
                anthropic_id,
                model,
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
            )
            .await;

            if is_thinking {
                close_content_block(tx, text_block_index).await;
                if thinking_block_index.is_none() {
                    let index = *next_block_index;
                    *next_block_index += 1;
                    *thinking_block_index = Some(index);
                    let data = json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "thinking",
                            "thinking": ""
                        }
                    });
                    send_event(tx, Some("content_block_start"), &data.to_string()).await;
                }
                let data = json!({
                    "type": "content_block_delta",
                    "index": thinking_block_index.unwrap_or_default(),
                    "delta": {
                        "type": "thinking_delta",
                        "thinking": text
                    }
                });
                send_event(tx, Some("content_block_delta"), &data.to_string()).await;
            } else {
                close_content_block(tx, thinking_block_index).await;
                if text_block_index.is_none() {
                    let index = *next_block_index;
                    *next_block_index += 1;
                    *text_block_index = Some(index);
                    let data = json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "text",
                            "text": ""
                        }
                    });
                    send_event(tx, Some("content_block_start"), &data.to_string()).await;
                }
                let data = json!({
                    "type": "content_block_delta",
                    "index": text_block_index.unwrap_or_default(),
                    "delta": {
                        "type": "text_delta",
                        "text": text
                    }
                });
                send_event(tx, Some("content_block_delta"), &data.to_string()).await;
            }
        }
        ResponseFormat::Responses => {
            let data = json!({
                "type": if is_thinking { "response.reasoning.delta" } else { "response.output_text.delta" },
                "response_id": response_id,
                "delta": text
            });
            send_data(tx, &data.to_string()).await;
        }
        ResponseFormat::OpenAI => {
            if is_thinking {
                return;
            }
            let delta = crate::gateway::models::OpenAIChatDelta {
                role: if !*message_started {
                    *message_started = true;
                    Some("assistant".to_string())
                } else {
                    None
                },
                content: Some(text.to_string()),
                tool_calls: None,
            };
            let chunk = crate::gateway::stream::build_openai_chunk(
                completion_id,
                created,
                model,
                delta,
                None,
                None,
            );
            if let Ok(chunk_json) = serde_json::to_string(&chunk) {
                send_data(tx, &chunk_json).await;
            }
        }
    }
}


pub(crate) async fn ensure_anthropic_message_start(
    tx: &mpsc::Sender<Result<Bytes, Infallible>>,
    message_started: &mut bool,
    anthropic_id: &str,
    model: &str,
    input_tokens: i32,
    output_tokens: i32,
    cache_read_input_tokens: Option<i32>,
    cache_creation_input_tokens: Option<i32>,
) {
    if *message_started {
        return;
    }

    let mut usage = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens
    });

    // 添加 cache token 信息（如果存在）
    if let Some(cache_read) = cache_read_input_tokens {
        usage["cache_read_input_tokens"] = json!(cache_read);
    }
    if let Some(cache_creation) = cache_creation_input_tokens {
        usage["cache_creation_input_tokens"] = json!(cache_creation);
    }

    let data = json!({
        "type": "message_start",
        "message": {
            "id": anthropic_id,
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": model,
            "stop_reason": Value::Null,
            "stop_sequence": Value::Null,
            "usage": usage
        }
    });
    send_event(tx, Some("message_start"), &data.to_string()).await;
    *message_started = true;
}


pub(crate) async fn close_content_block(
    tx: &mpsc::Sender<Result<Bytes, Infallible>>,
    index: &mut Option<usize>,
) {
    if let Some(current) = index.take() {
        let data = json!({
            "type": "content_block_stop",
            "index": current
        });
        send_event(tx, Some("content_block_stop"), &data.to_string()).await;
    }
}


pub(crate) async fn send_event(
    tx: &mpsc::Sender<Result<Bytes, Infallible>>,
    event: Option<&str>,
    payload: &str,
) -> bool {
    let chunk = if let Some(event) = event {
        format!("event: {event}\ndata: {payload}\n\n")
    } else {
        format!("data: {payload}\n\n")
    };
    tx.send(Ok(Bytes::from(chunk))).await.is_ok()
}


pub(crate) async fn send_data(tx: &mpsc::Sender<Result<Bytes, Infallible>>, payload: &str) -> bool {
    send_event(tx, None, payload).await
}

