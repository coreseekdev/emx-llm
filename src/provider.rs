//! Provider creation and management

use super::client::{AnthropicClient, Client, OpenAIClient};
use super::config::ProviderConfig;
use super::Result;

/// Create an LLM client based on the provider configuration.
///
/// Returns a trait object so callers are decoupled from concrete provider
/// types. Adding a new provider only requires a new match arm here.
pub fn create_client(config: ProviderConfig) -> Result<Box<dyn Client>> {
    match config.provider_type {
        crate::ProviderType::OpenAI => Ok(Box::new(OpenAIClient::new(config)?)),
        crate::ProviderType::Anthropic => Ok(Box::new(AnthropicClient::new(config)?)),
    }
}

/// Create an LLM client based on model-specific configuration.
///
/// This function supports hierarchical configuration where model-specific
/// settings inherit from parent sections.
///
/// # Arguments
///
/// * `model_ref` - A model reference (e.g., "glm-5", "anthropic.glm.glm-5")
///
/// # Examples
///
/// ```rust,ignore
/// use emx_llm::{create_client, create_client_for_model, Client};
///
/// # async fn example() -> anyhow::Result<()> {
/// let (client, model_id) = create_client_for_model("glm-5")?;
/// let response = client.chat(&[], &model_id).await?;
/// # Ok(())
/// # }
/// ```
pub fn create_client_for_model(model_ref: &str) -> anyhow::Result<(Box<dyn Client>, String)> {
    let (model_config, model_id) = ProviderConfig::load_for_model(model_ref)?;

    let provider_config = ProviderConfig {
        provider_type: model_config.provider_type,
        api_base: model_config.api_base,
        api_key: model_config.api_key,
        model: Some(model_id.clone()),
        max_tokens: model_config.max_tokens,
        timeout_secs: None, // Use default timeout
    };

    let client = create_client(provider_config)?;
    Ok((client, model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_openai_client() {
        let config = ProviderConfig {
            provider_type: crate::ProviderType::OpenAI,
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            model: None,
            max_tokens: None,
            timeout_secs: None,
        };
        let client = create_client(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_anthropic_client() {
        let config = ProviderConfig {
            provider_type: crate::ProviderType::Anthropic,
            api_base: "https://api.anthropic.com".to_string(),
            api_key: "test-key".to_string(),
            model: None,
            max_tokens: None,
            timeout_secs: None,
        };
        let client = create_client(config);
        assert!(client.is_ok());
    }
}
