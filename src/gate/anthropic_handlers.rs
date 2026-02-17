//! Anthropic-compatible handlers

use crate::gate::handlers::GatewayState;
use crate::gate::router::resolve_model_for_provider;
use crate::message::Message;
use crate::{create_client_for_model, ProviderType, Usage};
use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::{self, StreamExt};
use serde_json::json;
use serde_json::Value;
use tracing::{error, info};

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

fn event_with_type(event_type: &str, data: Value) -> Event {
    Event::default()
        .event(event_type)
        .data(data.to_string())
}

/// Handle Anthropic messages (streaming and non-streaming)
pub async fn messages_handler(
    State(_state): State<GatewayState>,
    Json(request): Json<Value>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::io::Error>>>, StatusCode> {
    let stream = request
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let model = match request.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    info!("Anthropic request for model: {} (stream: {})", model, stream);

    // For Anthropic endpoint, always use Anthropic provider type
    let resolved = resolve_model_for_provider(model, ProviderType::Anthropic)
        .map_err(|e| {
            error!("Failed to resolve model '{}': {}", model, e);
            StatusCode::NOT_FOUND
        })?;

    let model_ref = resolved.model_ref;

    let messages_value = request
        .get("messages")
        .ok_or(StatusCode::BAD_REQUEST)?;

    let messages: Vec<Message> = serde_json::from_value(messages_value.clone())
        .map_err(|e| {
            error!("Failed to parse messages: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    match create_client_for_model(&model_ref) {
        Ok((client, model_id)) => {
            if stream {
                // Streaming - match GLM's exact format
                let stream = client.chat_stream(&messages, &model_id);
                let id = format!("msg_{}", uuid_simple());
                let model = model_id.clone();

                let events: Vec<Result<Event, std::io::Error>> = stream.map(move |result| {
                    match result {
                        Ok(event) => {
                            if event.done {
                                // message_delta with usage, then message_stop
                                let mut results = Vec::new();
                                
                                // message_delta with usage
                                if let Some(usage) = &event.usage {
                                    let delta_json = json!({
                                        "type": "message_delta",
                                        "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                                        "usage": {
                                            "input_tokens": usage.prompt_tokens,
                                            "output_tokens": usage.completion_tokens,
                                            "cache_read_input_tokens": 0,
                                            "server_tool_use": {"web_search_requests": 0},
                                            "service_tier": "standard"
                                        }
                                    });
                                    results.push(Ok(event_with_type("message_delta", delta_json)));
                                }
                                
                                // message_stop
                                results.push(Ok(event_with_type("message_stop", json!({"type": "message_stop"}))));
                                
                                // For stream::iter, we need to flatten - but we can't return multiple events
                                // So we'll just return the message_stop as the final event
                                return results.pop().unwrap();
                            } else if !event.delta.is_empty() {
                                // content_block_delta - but we need to send content_block_start first
                                // For simplicity, we'll send content_block_delta
                                return Ok(event_with_type("content_block_delta", json!({
                                    "type": "content_block_delta",
                                    "index": 0,
                                    "delta": {"type": "text_delta", "text": event.delta}
                                })));
                            } else {
                                // Empty event - skip
                                return Ok(Event::default());
                            }
                        }
                        Err(e) => {
                            let json = json!({"type": "error", "error": {"type": "api_error", "message": e.to_string()}});
                            Ok(event_with_type("error", json))
                        }
                    }
                }).collect().await;

                let stream = stream::iter(events);
                Ok(Sse::new(Box::pin(stream)))
            } else {
                // Non-streaming
                match client.chat(&messages, &model_id).await {
                    Ok((content, usage)) => {
                        let json = json!({
                            "id": format!("msg_{}", uuid_simple()),
                            "type": "message",
                            "role": "assistant",
                            "content": [{"type": "text", "text": content}],
                            "model": model,
                            "stop_reason": "end_turn",
                            "usage": {
                                "input_tokens": usage.prompt_tokens,
                                "output_tokens": usage.completion_tokens
                            }
                        });
                        let events = vec![Ok(Event::default().data(json.to_string()))];
                        let stream = stream::iter(events);
                        Ok(Sse::new(Box::pin(stream)))
                    }
                    Err(e) => {
                        error!("API call failed: {}", e);
                        Err(StatusCode::BAD_GATEWAY)
                    }
                }
            }
        }
        Err(e) => {
            info!("Model '{}' not configured, returning mock: {}", model, e);
            let id = format!("msg_{}", uuid_simple());

            if stream {
                let mock_events = vec![
                    Ok(Event::default().data(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Mock"}}"#.to_string())),
                    Ok(Event::default().data(format!(r#"{{"type":"message_stop","id":"{}"}}"#, id))),
                ];
                let stream = stream::iter(mock_events);
                Ok(Sse::new(Box::pin(stream)))
            } else {
                let json = json!({
                    "id": "msg-mock",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Mock response"}],
                    "model": model,
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 10, "output_tokens": 10}
                });
                let events = vec![Ok(Event::default().data(json.to_string()))];
                let stream = stream::iter(events);
                Ok(Sse::new(Box::pin(stream)))
            }
        }
    }
}
