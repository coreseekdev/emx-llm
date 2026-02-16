//! Provider-specific handlers

use crate::gate::handlers::GatewayState;
use crate::{ProviderConfig, ProviderType};
use axum::{extract::State, Json};
use serde_json::json;
use serde_json::Value;

/// Strip provider type prefix from model_ref
fn strip_provider_prefix(model_ref: &str, provider_type: ProviderType) -> String {
    let prefix = format!("{}.", provider_type.config_key());
    model_ref.strip_prefix(&prefix).map(|s| s.to_string()).unwrap_or_else(|| model_ref.to_string())
}

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
                    let id = strip_provider_prefix(model_ref, ProviderType::OpenAI);
                    json!({
                        "id": id,
                        "object": "model",
                        "owned_by": config.provider_type.config_key(),
                        "permission": [],
                        "created": 1677610602
                    })
                })
                .collect();
            
            Json(json!({
                "object": "list",
                "data": models_data
            }))
        }
        Err(_) => {
            Json(json!({
                "object": "list",
                "data": []
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
                    let id = strip_provider_prefix(model_ref, ProviderType::Anthropic);
                    json!({
                        "id": id,
                        "object": "model",
                        "owned_by": "anthropic",
                        "permission": [],
                        "created": 1677610602
                    })
                })
                .collect();
            
            Json(json!({
                "object": "list",
                "data": models_data
            }))
        }
        Err(_) => {
            Json(json!({
                "object": "list",
                "data": []
            }))
        }
    }
}
