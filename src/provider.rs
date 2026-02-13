//! Provider creation and management

use super::{client::AnthropicClient, client::Client, client::OpenAIClient, config::ProviderConfig, Result};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_openai_client() {
        let config = ProviderConfig::openai(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
        );
        let client = create_client(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_anthropic_client() {
        let config = ProviderConfig::anthropic(
            "https://api.anthropic.com".to_string(),
            "test-key".to_string(),
        );
        let client = create_client(config);
        assert!(client.is_ok());
    }
}
