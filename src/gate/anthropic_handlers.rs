//! Anthropic-compatible handlers

use crate::gate::handlers::GatewayState;
use crate::gate::router::resolve_model;
use crate::message::Message;
use crate::{create_client_for_model, ProviderType};
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

/// Handle Anthropic messages (streaming and non-streaming)
pub async fn messages_handler(
    State(state): State<GatewayState>,
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

    let resolved = resolve_model(model, &state.config)
        .map_err(|e| {
            error!("Failed to resolve model '{}': {}", model, e);
            StatusCode::NOT_FOUND
        })?;

    if resolved.provider_type != ProviderType::Anthropic {
        error!("Model '{}' resolved to non-Anthropic provider: {:?}", model, resolved.provider_type);
        return Err(StatusCode::BAD_REQUEST);
    }

    let messages_value = request
        .get("messages")
        .ok_or(StatusCode::BAD_REQUEST)?;

    let messages: Vec<Message> = serde_json::from_value(messages_value.clone())
        .map_err(|e| {
            error!("Failed to parse messages: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    match create_client_for_model(model) {
        Ok((client, model_id)) => {
            if stream {
                // Streaming
                let stream = client.chat_stream(&messages, &model_id);
                let id = format!("msg_{}", uuid_simple());

                let events: Vec<Result<Event, std::io::Error>> = stream.map(move |result| {
                    match result {
                        Ok(event) => {
                            if event.done {
                                let json = json!({"type": "message_stop", "id": id});
                                Ok(Event::default().data(json.to_string()))
                            } else if !event.delta.is_empty() {
                                let json = json!({
                                    "type": "content_block_delta",
                                    "index": 0,
                                    "delta": {"type": "text_delta", "text": event.delta}
                                });
                                Ok(Event::default().data(json.to_string()))
                            } else {
                                Ok(Event::default())
                            }
                        }
                        Err(e) => {
                            let json = json!({"type": "error", "error": {"type": "api_error", "message": e.to_string()}});
                            Ok(Event::default().data(json.to_string()))
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
                            "usage": {"input_tokens": usage.prompt_tokens, "output_tokens": usage.completion_tokens}
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
