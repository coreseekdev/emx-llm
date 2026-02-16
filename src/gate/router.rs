//! Router module for resolving model references to provider configurations

use crate::{ProviderConfig, ProviderType};
use serde::{Deserialize, Serialize};

/// Resolved model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModel {
    /// Provider type
    pub provider_type: ProviderType,

    /// Model name
    pub model_name: String,

    /// Full model reference (e.g., "openai.gpt-4")
    pub model_ref: String,
}

/// Resolve a model reference string to provider configuration
pub fn resolve_model(model: &str, _config: &ProviderConfig) -> Result<ResolvedModel, String> {
    // Parse model reference
    let model_ref = parse_model_reference(model)?;

    // Get provider type from model reference
    let provider_type = model_ref.provider_type;

    Ok(ResolvedModel {
        provider_type,
        model_name: model_ref.model_name,
        model_ref: model.to_string(),
    })
}

/// Resolve model for a specific provider type
/// This is used when the endpoint already indicates the provider (e.g., /openai/... or /anthropic/...)
pub fn resolve_model_for_provider(
    model: &str,
    provider_type: ProviderType,
) -> Result<ResolvedModel, String> {
    let provider_prefix = provider_type.config_key();

    // Try to find a matching model in config
    if let Ok(models) = ProviderConfig::list_models() {
        // Look for a model that ends with the provided model name
        for (model_ref, model_config) in &models {
            if model_ref.ends_with(&format!(".{}", model)) || model_ref == model {
                // Check if it matches our provider type
                if model_ref.starts_with(&format!("{}.", provider_prefix)) {
                    return Ok(ResolvedModel {
                        provider_type,
                        model_name: model_config
                            .model
                            .clone()
                            .unwrap_or_else(|| model.to_string()),
                        model_ref: model_ref.clone(),
                    });
                }
            }
        }
    }

    // Fall back: construct the model_ref
    let model_name = model.split('.').last().unwrap_or(model).to_string();
    let full_ref = format!("{}.{}", provider_prefix, model_name);
    Ok(ResolvedModel {
        provider_type,
        model_name,
        model_ref: full_ref,
    })
}

/// Parse model reference string
///
/// Supports three formats:
/// - Short name: "gpt-4"
/// - Qualified name: "openai.gpt-4"
/// - Fully qualified name: "openai.some_provider.gpt-4"
fn parse_model_reference(model: &str) -> Result<ModelReference, String> {
    let parts: Vec<&str> = model.split('.').collect();

    match parts.len() {
        1 => {
            // Short name: "gpt-4"
            // Need to look up in configuration to find provider
            Err(format!(
                "Ambiguous model reference '{}'. Please use qualified name (e.g., 'openai.{}')",
                model, model
            ))
        }
        2 => {
            // Qualified name: "openai.gpt-4"
            let provider_type = parse_provider_type(parts[0])?;

            Ok(ModelReference {
                provider_type,
                model_name: parts[1].to_string(),
            })
        }
        _ => {
            // Fully qualified name: "openai.some_provider.gpt-4"
            let provider_type = parse_provider_type(parts[0])?;

            // The model name is the last part
            let model_name = parts.last().unwrap().to_string();

            Ok(ModelReference {
                provider_type,
                model_name,
            })
        }
    }
}

/// Parse provider type from string
fn parse_provider_type(s: &str) -> Result<ProviderType, String> {
    match s.to_lowercase().as_str() {
        "openai" | "glm" => Ok(ProviderType::OpenAI),
        "anthropic" | "claude" => Ok(ProviderType::Anthropic),
        _ => Err(format!("Unknown provider type: {}", s)),
    }
}

/// Internal model reference representation
#[derive(Debug, Clone)]
struct ModelReference {
    pub provider_type: ProviderType,
    pub model_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qualified_model() {
        let result = parse_model_reference("openai.gpt-4");
        assert!(result.is_ok());
        let model_ref = result.unwrap();
        assert_eq!(model_ref.provider_type, ProviderType::OpenAI);
        assert_eq!(model_ref.model_name, "gpt-4");
    }

    #[test]
    fn test_parse_fully_qualified_model() {
        let result = parse_model_reference("openai.azure.gpt-4");
        assert!(result.is_ok());
        let model_ref = result.unwrap();
        assert_eq!(model_ref.provider_type, ProviderType::OpenAI);
        assert_eq!(model_ref.model_name, "gpt-4");
    }

    #[test]
    fn test_parse_short_model_fails() {
        let result = parse_model_reference("gpt-4");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_provider() {
        let result = parse_model_reference("unknown.gpt-4");
        assert!(result.is_err());
    }
}
