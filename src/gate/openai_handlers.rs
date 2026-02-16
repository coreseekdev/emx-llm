//! OpenAI-compatible handlers

use crate::gate::handlers::GatewayState;
use crate::gate::router::resolve_model_for_provider;
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

/// Handle OpenAI chat completions (streaming and non-streaming)
pub async fn chat_handler(
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

    info!("OpenAI chat request for model: {} (stream: {})", model, stream);

    // For OpenAI endpoint, always use OpenAI provider type
    let resolved = resolve_model_for_provider(model, ProviderType::OpenAI)
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
                // Streaming
                let stream = client.chat_stream(&messages, &model_id);
                let model = model.to_string();
                let created = chrono::Utc::now().timestamp();
                let id = format!("chatcmpl-{}", uuid_simple());

                let events: Vec<Result<Event, std::io::Error>> = stream.map(move |result| {
                    match result {
                        Ok(event) => {
                            if event.done {
                                let json = json!({
                                    "id": id,
                                    "object": "chat.completion.chunk",
                                    "created": created,
                                    "model": model,
                                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
                                });
                                Ok(Event::default().data(json.to_string()))
                            } else if !event.delta.is_empty() {
                                let json = json!({
                                    "id": id,
                                    "object": "chat.completion.chunk",
                                    "created": created,
                                    "model": model,
                                    "choices": [{"index": 0, "delta": {"content": event.delta}, "finish_reason": null}]
                                });
                                Ok(Event::default().data(json.to_string()))
                            } else {
                                Ok(Event::default())
                            }
                        }
                        Err(e) => {
                            let json = json!({"error": {"message": e.to_string(), "type": "api_error"}});
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
                            "id": format!("chatcmpl-{}", uuid_simple()),
                            "object": "chat.completion",
                            "created": chrono::Utc::now().timestamp(),
                            "model": model,
                            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
                            "usage": {"prompt_tokens": usage.prompt_tokens, "completion_tokens": usage.completion_tokens, "total_tokens": usage.total_tokens}
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
            let id = format!("chatcmpl-{}", uuid_simple());
            let created = chrono::Utc::now().timestamp();

            if stream {
                let mock_events = vec![
                    Ok(Event::default().data(format!(r#"{{"id":"{}","object":"chat.completion.chunk","created":{},"model":"{}","choices":[{{"index":0,"delta":{{"content":"Mock"}},"finish_reason":null}}]}}"#, id, created, model))),
                    Ok(Event::default().data(format!(r#"{{"id":"{}","object":"chat.completion.chunk","created":{},"model":"{}","choices":[{{"index":0,"delta":{{}},"finish_reason":"stop"}}]}}"#, id, created, model))),
                ];
                let stream = stream::iter(mock_events);
                Ok(Sse::new(Box::pin(stream)))
            } else {
                let json = json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model,
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "Mock response"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
                });
                let events = vec![Ok(Event::default().data(json.to_string()))];
                let stream = stream::iter(events);
                Ok(Sse::new(Box::pin(stream)))
            }
        }
    }
}
