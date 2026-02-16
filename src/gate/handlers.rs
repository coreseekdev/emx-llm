//! HTTP request handlers for the gateway

use super::router::resolve_model;
use crate::message::Message;
use crate::{create_client_for_model, ProviderConfig, ProviderType};
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Generate a simple UUID-like string
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

/// Gateway state shared across handlers
#[derive(Clone)]
pub struct GatewayState {
    pub config: Arc<ProviderConfig>,
}

/// Handle OpenAI-compatible chat completions
pub async fn openai_chat_handler(
    State(state): State<GatewayState>,
    Json(mut request): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Extract model from request body
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    info!("OpenAI chat request for model: {}", model);

    // Resolve model to provider
    let resolved = resolve_model(model, &state.config)
        .map_err(|e| {
            error!("Failed to resolve model '{}': {}", model, e);
            StatusCode::NOT_FOUND
        })?;

    // Verify it's an OpenAI provider
    if resolved.provider_type != ProviderType::OpenAI {
        error!(
            "Model '{}' resolved to non-OpenAI provider: {:?}",
            model, resolved.provider_type
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract messages from request
    let messages_value = request
        .get("messages")
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Convert OpenAI messages to emx-llm Message format
    let messages: Vec<Message> = serde_json::from_value(messages_value.clone())
        .map_err(|e| {
            error!("Failed to parse messages: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Try to create client and call the API
    match create_client_for_model(model) {
        Ok((client, model_id)) => {
            // Call the actual API
            match client.chat(&messages, &model_id).await {
                Ok((content, usage)) => {
                    // Convert response to OpenAI format
                    Ok(Json(json!({
                        "id": format!("chatcmpl-{}", uuid_simple()),
                        "object": "chat.completion",
                        "created": chrono::Utc::now().timestamp(),
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": content
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": usage.prompt_tokens,
                            "completion_tokens": usage.completion_tokens,
                            "total_tokens": usage.total_tokens
                        }
                    })))
                }
                Err(e) => {
                    error!("API call failed: {}", e);
                    Err(StatusCode::BAD_GATEWAY)
                }
            }
        }
        Err(e) => {
            // Model not configured, return mock response
            info!("Model '{}' not configured, returning mock response: {}", model, e);
            Ok(Json(json!({
                "id": "chatcmpl-mock",
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Mock response for model ".to_string() + model
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })))
        }
    }
}

/// Handle Anthropic-compatible messages
pub async fn anthropic_messages_handler(
    State(state): State<GatewayState>,
    Json(mut request): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Extract model from request body
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    info!("Anthropic messages request for model: {}", model);

    // Resolve model to provider
    let resolved = resolve_model(model, &state.config)
        .map_err(|e| {
            error!("Failed to resolve model '{}': {}", model, e);
            StatusCode::NOT_FOUND
        })?;

    // Verify it's an Anthropic provider
    if resolved.provider_type != ProviderType::Anthropic {
        error!(
            "Model '{}' resolved to non-Anthropic provider: {:?}",
            model, resolved.provider_type
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract messages from request
    let messages_value = request
        .get("messages")
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Extract max_tokens
    let max_tokens = request
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(1024);

    // Convert Anthropic messages to emx-llm Message format
    // Anthropic uses the same message format as OpenAI
    let messages: Vec<Message> = serde_json::from_value(messages_value.clone())
        .map_err(|e| {
            error!("Failed to parse messages: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Try to create client and call the API
    match create_client_for_model(model) {
        Ok((client, model_id)) => {
            // Call the actual API
            match client.chat(&messages, &model_id).await {
                Ok((content, usage)) => {
                    // Convert response to Anthropic format
                    Ok(Json(json!({
                        "id": format!("msg_{}", uuid_simple()),
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "text",
                            "text": content
                        }],
                        "model": model,
                        "stop_reason": "end_turn",
                        "usage": {
                            "input_tokens": usage.prompt_tokens,
                            "output_tokens": usage.completion_tokens
                        }
                    })))
                }
                Err(e) => {
                    error!("API call failed: {}", e);
                    Err(StatusCode::BAD_GATEWAY)
                }
            }
        }
        Err(e) => {
            // Model not configured, return mock response
            info!("Model '{}' not configured, returning mock response: {}", model, e);
            Ok(Json(json!({
                "id": "msg-mock",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": "Mock response for model ".to_string() + model
                }],
                "model": model,
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 10
                }
            })))
        }
    }
}

    // Try to create client, but fall back to mock if not configured
    let model_name = match create_client_for_model(model) {
        Ok((_client, name)) => name,
        Err(_) => {
            // Model not configured, return mock response
            info!("Model '{}' not configured, returning mock response", model);
            return Ok(Json(json!({
                "id": "msg-mock",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": "Mock response for model ".to_string() + model
                }],
                "model": model,
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 10
                }
            })));
        }
    };

    // TODO: Implement actual Anthropic message handling
    // For now, return a mock response
    Ok(Json(json!({
        "id": "msg-mock",
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": "Mock response for model ".to_string() + model
        }],
        "model": model_name,
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 10,
            "output_tokens": 10
        }
    })))
}

/// Handle model list request
pub async fn list_models(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    // TODO: Return actual models from configuration
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "openai.gpt-4",
                "object": "model",
                "owned_by": "openai",
                "permission": [],
                "created": 1677610602
            },
            {
                "id": "anthropic.claude-3-opus-20240229",
                "object": "model",
                "owned_by": "anthropic",
                "permission": [],
                "created": 1677610602
            }
        ]
    }))
}

/// Handle provider list request
pub async fn list_providers(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    // TODO: Return actual providers from configuration
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "openai",
                "type": "openai",
                "models": ["gpt-4", "gpt-3.5-turbo"],
                "api_base": "https://api.openai.com/v1"
            },
            {
                "id": "anthropic",
                "type": "anthropic",
                "models": ["claude-3-opus-20240229"],
                "api_base": "https://api.anthropic.com"
            }
        ]
    }))
}
