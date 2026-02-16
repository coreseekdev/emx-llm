//! Provider-specific handlers

use crate::gate::handlers::GatewayState;
use crate::{ProviderConfig, ProviderType};
use axum::{extract::State, Json};
use serde_json::json;
use serde_json::Value;

/// Handle OpenAI models list request
pub async fn list_openai_models(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    match ProviderConfig::list_models() {
        Ok(models) => {
            let models_data: Vec<Value> = models
                .iter()
                .filter(|(_, config)| config.provider_type == ProviderType::OpenAI)
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
                Json(json!({
                    "object": "list",
                    "data": [
                        {"id": "gpt-4", "object": "model", "owned_by": "openai", "permission": [], "created": 1677610602},
                        {"id": "gpt-3.5-turbo", "object": "model", "owned_by": "openai", "permission": [], "created": 1677610602}
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
            Json(json!({
                "object": "list",
                "data": [
                    {"id": "gpt-4", "object": "model", "owned_by": "openai", "permission": [], "created": 1677610602}
                ]
            }))
        }
    }
}

/// Handle Anthropic models list request
pub async fn list_anthropic_models(
    State(_state): State<GatewayState>,
) -> Json<Value> {
    match ProviderConfig::list_models() {
        Ok(models) => {
            let models_data: Vec<Value> = models
                .iter()
                .filter(|(_, config)| config.provider_type == ProviderType::Anthropic)
                .map(|(model_ref, _config)| {
                    json!({
                        "id": model_ref,
                        "object": "model",
                        "owned_by": "anthropic",
                        "permission": [],
                        "created": 1677610602
                    })
                })
                .collect();
            
            if models_data.is_empty() {
                Json(json!({
                    "object": "list",
                    "data": [
                        {"id": "claude-3-opus-20240229", "object": "model", "owned_by": "anthropic", "permission": [], "created": 1677610602}
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
            Json(json!({
                "object": "list",
                "data": [
                    {"id": "claude-3-opus-20240229", "object": "model", "owned_by": "anthropic", "permission": [], "created": 1677610602}
                ]
            }))
        }
    }
}
