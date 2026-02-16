//! HTTP request handlers for the gateway

use super::router::resolve_model;
use crate::message::Message;
use crate::{create_client_for_model, ProviderConfig, ProviderType};
use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::{self, Stream};
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

/// Create an OpenAI-compatible error response
fn openai_error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    let error_type = match status {
        StatusCode::BAD_REQUEST => "invalid_request_error",
        StatusCode::UNAUTHORIZED => "authentication_error",
        StatusCode::FORBIDDEN => "permission_error",
        StatusCode::NOT_FOUND => "invalid_request_error",
        StatusCode::TOO_MANY_REQUESTS => "rate_limit_error",
        StatusCode::SERVICE_UNAVAILABLE => "server_error",
        _ => "server_error",
    };
    
    (status, Json(json!({
        "error": {
            "message": message,
            "type": error_type,
            "code": status.as_u16()
        }
    })))
}

/// Create an Anthropic-compatible error response
fn anthropic_error(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({
        "error": {
            "type": "error",
            "message": message
        }
    })))
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
            openai_error(StatusCode::NOT_FOUND, &format!("Model '{}' not found: {}", model, e))
        })?;

    // Verify it's an OpenAI provider
    if resolved.provider_type != ProviderType::OpenAI {
        error!(
            "Model '{}' resolved to non-OpenAI provider: {:?}",
            model, resolved.provider_type
        );
        return Err(openai_error(StatusCode::BAD_REQUEST, 
            &format!("Model '{}' is not an OpenAI model", model)).1);
    }

    // Extract messages from request
    let messages_value = request
        .get("messages")
        .ok_or(openai_error(StatusCode::BAD_REQUEST, "Missing 'messages' field in request body"))?;

    // Convert OpenAI messages to emx-llm Message format
    let messages: Vec<Message> = serde_json::from_value(messages_value.clone())
        .map_err(|e| {
            error!("Failed to parse messages: {}", e);
            openai_error(StatusCode::BAD_REQUEST, &format!("Invalid messages format: {}", e)).1
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
                    Err(openai_error(StatusCode::BAD_GATEWAY, &format!("Provider API error: {}", e)).1)
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
    let model = match request.get("model").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    info!("Anthropic messages request for model: {}", model);

    let resolved = match resolve_model(model, &state.config) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to resolve model '{}': {}", model, e);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    if resolved.provider_type != ProviderType::Anthropic {
        error!("Model '{}' resolved to non-Anthropic provider: {:?}", model, resolved.provider_type);
        return Err(StatusCode::BAD_REQUEST);
    }

    let messages_value = match request.get("messages") {
        Some(v) => v,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let messages: Vec<Message> = match serde_json::from_value(messages_value.clone()) {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to parse messages: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match create_client_for_model(model) {
        Ok((client, model_id)) => {
            match client.chat(&messages, &model_id).await {
                Ok((content, usage)) => {
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
 
/// Handle OpenAI-compatible streaming chat completions
#[allow(dead_code)]
pub async fn openai_chat_stream_handler(
    State(state): State<GatewayState>,
    Json(mut request): Json<Value>,
) -> Result<Sse, StatusCode> {
    // Extract model from request body
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    info!("OpenAI streaming chat request for model: {}", model);

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

    // Try to create client and call the streaming API
    match create_client_for_model(model) {
        Ok((client, model_id)) => {
            let stream = client.chat_stream(&messages, &model_id);
            let model = model.to_string();
            let created = chrono::Utc::now().timestamp();
            let id = format!("chatcmpl-{}", uuid_simple());

            let sse_stream = stream.map(move |result| {
                match result {
                    Ok(event) => {
                        if event.done {
                            // Final chunk with usage
                            let usage = event.usage.unwrap_or(crate::message::Usage {
                                prompt_tokens: 0,
                                completion_tokens: 0,
                                total_tokens: 0,
                            });
                            let json = json!({
                                "id": id,
                                "object": "chat.completion.chunk",
                                "created": created,
                                "model": model,
                                "choices": [{
                                    "index": 0,
                                    "delta": {},
                                    "finish_reason": "stop"
                                }],
                                "usage": {
                                    "prompt_tokens": usage.prompt_tokens,
                                    "completion_tokens": usage.completion_tokens,
                                    "total_tokens": usage.total_tokens
                                }
                            });
                            Event::default().data(json.to_string()).to_string().parse().unwrap()
                        } else if !event.delta.is_empty() {
                            // Content chunk
                            let json = json!({
                                "id": id,
                                "object": "chat.completion.chunk",
                                "created": created,
                                "model": model,
                                "choices": [{
                                    "index": 0,
                                    "delta": {
                                        "content": event.delta
                                    },
                                    "finish_reason": null
                                }]
                            });
                            Event::default().data(json.to_string()).to_string().parse().unwrap()
                        } else {
                            // Empty delta, skip
                            Event::default().data("").to_string().parse().unwrap()
                        }
                    }
                    Err(e) => {
                        let json = json!({
                            "error": {
                                "message": e.to_string(),
                                "type": "api_error"
                            }
                        });
                        Event::default().data(json.to_string()).to_string().parse().unwrap()
                    }
                }
            });

            Ok(Sse::new(sse_stream))
        }
        Err(e) => {
            // Model not configured, return mock streaming response
            info!("Model '{}' not configured, returning mock stream: {}", model, e);
            
            let sse_stream = stream::iter(vec![
                Ok(Event::default().data(r#"{"id":"chatcmpl-mock","object":"chat.completion.chunk","created":0,"model":"mock","choices":[{"index":0,"delta":{"content":"Mock response for model mock"},"finish_reason":null}]}"#.to_string()).to_string().parse::<Event>().unwrap()),
                Ok(Event::default().data(r#"{"id":"chatcmpl-mock","object":"chat.completion.chunk","created":0,"model":"mock","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}"#.to_string()).to_string().parse::<Event>().unwrap()),
            ]);
            
            Ok(Sse::new(sse_stream))
        }
    }
}

/// Handle model list request
pub async fn list_models(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    match ProviderConfig::list_models() {
        Ok(models) => {
            let models_data: Vec<Value> = models
                .iter()
                .map(|(model_ref, config)| {
                    json!({
                        "id": model_ref,
                        "object": "model",
                        "owned_by": config.provider_type.config_key(),
                        "permission": [],
                        "created": 1677610602
                    })
                })
                .collect();
            
            if models_data.is_empty() {
                // Return default models if none configured
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
            } else {
                Json(json!({
                    "object": "list",
                    "data": models_data
                }))
            }
        }
        Err(_) => {
            // Return default models on error
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
    }
}

/// Handle provider list request
pub async fn list_providers(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    match ProviderConfig::list_providers() {
        Ok(providers) => {
            let providers_data: Vec<Value> = providers
                .iter()
                .map(|(name, provider_type)| {
                    json!({
                        "id": name,
                        "type": provider_type.config_key(),
                        "api_base": provider_type.default_base_url()
                    })
                })
                .collect();
            
            if providers_data.is_empty() {
                // Return default providers if none configured
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
            } else {
                Json(json!({
                    "object": "list",
                    "data": providers_data
                }))
            }
        }
        Err(_) => {
            // Return default providers on error
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
    }
}
