//! OpenAI-compatible handlers with raw passthrough support

use crate::gate::handlers::GatewayState;
use crate::gate::router::resolve_model_for_provider;
use crate::message::Message;
use crate::{create_client_for_model, ProviderType};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::Response,
    Json,
};
use futures::stream::StreamExt;
use serde_json::json;
use serde_json::Value;
use tracing::{error, info};

/// Handle OpenAI chat completions with raw HTTP passthrough
/// This forwards the upstream response without parsing/rewriting, preserving all fields
pub async fn chat_handler_passthrough(
    State(_state): State<GatewayState>,
    Json(request): Json<Value>,
) -> Result<Response, StatusCode> {
    let stream = request
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let model = match request.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    info!("OpenAI chat request for model: {} (stream: {})", model, stream);

    let resolved = resolve_model_for_provider(model, ProviderType::OpenAI).map_err(|e| {
        error!("Failed to resolve model '{}': {}", model, e);
        StatusCode::NOT_FOUND
    })?;

    let model_ref = resolved.model_ref;

    let messages_value = request.get("messages").ok_or(StatusCode::BAD_REQUEST)?;

    let messages: Vec<Message> = serde_json::from_value(messages_value.clone()).map_err(|e| {
        error!("Failed to parse messages: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    match create_client_for_model(&model_ref) {
        Ok((client, model_id)) => {
            if stream {
                // Streaming with raw passthrough
                match client.chat_stream_raw(&messages, &model_id).await {
                    Ok(upstream_response) => {
                        // Forward the upstream response body stream directly
                        let upstream_body = upstream_response.bytes_stream();

                        // Create a properly typed stream for Axum
                        let body_stream = upstream_body.map(|result| {
                            result
                                .map(|bytes| bytes.to_vec())
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        });

                        let body = Body::from_stream(body_stream);

                        // Build response with SSE headers
                        let response = Response::builder()
                            .status(200)
                            .header("Content-Type", "text/event-stream")
                            .header("Cache-Control", "no-cache")
                            .header("Connection", "keep-alive")
                            .header("X-Accel-Buffering", "no")
                            .body(body)
                            .map_err(|e| {
                                error!("Failed to build response: {}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;

                        Ok(response)
                    }
                    Err(e) => {
                        error!("Upstream stream request failed: {}", e);
                        let json = json!({"error": {"message": e.to_string(), "type": "api_error"}});
                        Ok(Response::builder()
                            .status(500)
                            .header("Content-Type", "application/json")
                            .body(Body::from(json.to_string()))
                            .unwrap())
                    }
                }
            } else {
                // Non-streaming with raw passthrough
                match client.chat_raw(&messages, &model_id).await {
                    Ok(upstream_response) => {
                        // Get the response body bytes
                        let body_bytes = upstream_response.bytes().await.map_err(|e| {
                            error!("Failed to read upstream response body: {}", e);
                            StatusCode::BAD_GATEWAY
                        })?;

                        // Forward the raw response body
                        Ok(Response::builder()
                            .status(200)
                            .header("Content-Type", "application/json")
                            .body(Body::from(body_bytes))
                            .unwrap())
                    }
                    Err(e) => {
                        error!("Upstream request failed: {}", e);
                        let json = json!({"error": {"message": e.to_string(), "type": "api_error"}});
                        Ok(Response::builder()
                            .status(500)
                            .header("Content-Type", "application/json")
                            .body(Body::from(json.to_string()))
                            .unwrap())
                    }
                }
            }
        }
        Err(e) => {
            info!("Model '{}' not configured, returning mock: {}", model, e);
            let json = json!({
                "id": "chatcmpl-mock",
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": model,
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "Mock response"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
            });
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(Body::from(json.to_string()))
                .unwrap())
        }
    }
}
