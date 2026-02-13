//! LLM configuration using emx-config
//!
//! Configuration is loaded from multiple sources in priority order:
//! 1. Command-line arguments (highest)
//! 2. Environment variables (EMX_LLM_*)
//! 3. Local config file (./config.toml)
//! 4. Global config file ($EMX_HOME/config.toml or ~/.emx/config.toml)
//! 5. Default values (lowest)

use emx_config_core::ConfigBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// OpenAI-compatible API
    OpenAI,
    /// Anthropic-compatible API
    Anthropic,
}

impl ProviderType {
    /// Get the default API base URL for this provider type
    pub fn default_base_url(&self) -> &str {
        match self {
            ProviderType::OpenAI => "https://api.openai.com/v1",
            ProviderType::Anthropic => "https://api.anthropic.com",
        }
    }

    /// Get the config key for this provider
    pub fn config_key(&self) -> &str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
        }
    }
}

/// Configuration for an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (OpenAI or Anthropic)
    #[serde(rename = "type")]
    pub provider_type: ProviderType,

    /// API base URL
    pub api_base: String,

    /// API key
    #[serde(skip_serializing)]
    pub api_key: String,

    /// Default model to use
    pub default_model: Option<String>,

    /// Maximum tokens for response (Anthropic requires this; default: 4096)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: Option<u32>,
}

fn default_max_tokens() -> Option<u32> {
    None
}

impl ProviderConfig {
    /// Get the max_tokens value, falling back to 4096 for Anthropic
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    /// Load configuration from emx-config
    ///
    /// Uses the following configuration structure:
    /// ```toml
    /// [llm.provider]
    /// type = "openai"  # or "anthropic"
    /// model = "gpt-4"
    ///
    /// [llm.provider.openai]
    /// api_base = "https://api.openai.com/v1"
    /// api_key = "sk-..."
    /// default_model = "gpt-4"
    /// max_tokens = 4096
    ///
    /// [llm.provider.anthropic]
    /// api_base = "https://api.anthropic.com"
    /// api_key = "sk-ant-..."
    /// default_model = "claude-3-opus-20240229"
    /// max_tokens = 4096
    /// ```
    ///
    /// Environment variables (override config file):
    /// - `EMX_LLM_PROVIDER_TYPE` - Provider type (openai/anthropic)
    /// - `EMX_LLM_PROVIDER_MODEL` - Default model
    /// - `EMX_LLM_PROVIDER_OPENAI_API_KEY` - OpenAI API key
    /// - `EMX_LLM_PROVIDER_OPENAI_API_BASE` - OpenAI API base URL
    /// - `EMX_LLM_PROVIDER_ANTHROPIC_API_KEY` - Anthropic API key
    /// - `EMX_LLM_PROVIDER_ANTHROPIC_API_BASE` - Anthropic API base URL
    pub fn load() -> anyhow::Result<Self> {
        Self::load_with_args(None)
    }

    /// Load configuration with CLI argument overrides
    pub fn load_with_args(args: Option<HashMap<String, toml::Value>>) -> anyhow::Result<Self> {
        // Set up default values
        let mut defaults = HashMap::new();
        defaults.insert(
            "llm.provider.type".to_string(),
            toml::Value::String("openai".to_string()),
        );

        // Build config with emx-config
        let mut builder = ConfigBuilder::new()
            .with_prefix("EMX_LLM")
            .with_defaults(defaults);

        if let Some(args) = args {
            builder = builder.with_args(args);
        }

        let config = builder.build()?;

        // Get provider type
        let provider_type_str = config
            .get_string("llm.provider.type")
            .unwrap_or_else(|_| "openai".to_string());

        let provider_type = match provider_type_str.to_lowercase().as_str() {
            "openai" => ProviderType::OpenAI,
            "anthropic" => ProviderType::Anthropic,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid provider type: {}. Must be 'openai' or 'anthropic'",
                    provider_type_str
                ))
            }
        };

        let provider_key = provider_type.config_key();

        // Build provider config key prefix
        let base_key = format!("llm.provider.{}", provider_key);

        // Get API key - try emx-config first, then fallback to legacy env vars
        let api_key = config
            .get_string(&format!("{}.api_key", base_key))
            .or_else(|_| {
                // Fallback to legacy environment variables for backward compatibility
                let legacy_key = match provider_type {
                    ProviderType::OpenAI => "OPENAI_API_KEY",
                    ProviderType::Anthropic => "ANTHROPIC_AUTH_TOKEN",
                };
                std::env::var(legacy_key).map_err(|_| {
                    anyhow::anyhow!("{} not found in config or environment", format!("{}.api_key", base_key))
                })
            })?;

        // Get API base - try emx-config first, then fallback to legacy env vars
        let api_base = config
            .get_string(&format!("{}.api_base", base_key))
            .or_else(|_| {
                // Fallback to legacy environment variables
                let legacy_key = match provider_type {
                    ProviderType::OpenAI => "OPENAI_API_BASE",
                    ProviderType::Anthropic => "ANTHROPIC_BASE_URL",
                };
                std::env::var(legacy_key).map_err(|_| {
                    anyhow::anyhow!("{} not found in config or environment", format!("{}.api_base", base_key))
                })
            })
            .unwrap_or_else(|_| provider_type.default_base_url().to_string());

        // Get default model from config or CLI args
        let default_model = config
            .get_string(&format!("{}.default_model", base_key))
            .ok()
            .or_else(|| {
                config.get_string("llm.provider.model").ok()
            });

        // Get max_tokens
        let max_tokens = config
            .get_int(&format!("{}.max_tokens", base_key))
            .ok()
            .map(|v| v as u32);

        Ok(ProviderConfig {
            provider_type,
            api_base,
            api_key,
            default_model,
            max_tokens,
        })
    }

    /// Get the API key
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Get the API base URL
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// Get the default model, if set
    pub fn default_model(&self) -> Option<&str> {
        self.default_model.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_config_key() {
        assert_eq!(ProviderType::OpenAI.config_key(), "openai");
        assert_eq!(ProviderType::Anthropic.config_key(), "anthropic");
    }

    #[test]
    fn test_provider_type_default_base_url() {
        assert_eq!(
            ProviderType::OpenAI.default_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            ProviderType::Anthropic.default_base_url(),
            "https://api.anthropic.com"
        );
    }
}
