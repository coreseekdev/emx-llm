//! LLM configuration using emx-config
//!
//! Configuration is loaded from multiple sources in priority order:
//! 1. Command-line arguments (highest)
//! 2. Environment variables (EMX_LLM_*)
//! 3. Local config file (./config.toml)
//! 4. Global config file ($EMX_HOME/config.toml or ~/.emx/config.toml)
//! 5. Default values (lowest)
//!
//! ## Hierarchical Configuration
//!
//! Model-specific configuration inherits from parent sections:
//!
//! ```toml
//! [llm.provider]
//! type = "openai"
//!
//! [llm.provider.openai]
//! api_base = "https://api.openai.com/v1"
//! api_key = "sk-..."
//! default_model = "gpt-4"
//! max_tokens = 4096
//!
//! [llm.provider.anthropic]
//! api_base = "https://api.anthropic.com"
//! api_key = "sk-ant-..."
//! default_model = "claude-3-opus-20240229"
//! max_tokens = 4096
//!
//! # Third-party Anthropic-compatible provider
//! [llm.provider.anthropic.glm]
//! api_base = "https://open.bigmodel.cn/api/paas/v4/"
//! api_key = "..."
//! default_model = "glm-4.5"
//!
//! # Model under third-party provider (inherits from parent)
//! [llm.provider.anthropic.glm.glm-5]
//! model = "glm-5"
//! # api_base inherited from glm section
//! # api_key inherited from glm section
//! ```

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

    /// Model to use
    pub model: Option<String>,

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
        let base_key = format!("llm.provider.{}", provider_key);

        // Get API key - try emx-config first, then fallback to legacy env vars
        let api_key = config
            .get_string(&format!("{}.api_key", base_key))
            .or_else(|_| {
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
        let model = config
            .get_string(&format!("{}.model", base_key))
            .ok()
            .or_else(|| config.get_string("llm.provider.model").ok());

        // Get max_tokens
        let max_tokens = config
            .get_int(&format!("{}.max_tokens", base_key))
            .ok()
            .map(|v| v as u32);

        Ok(ProviderConfig {
            provider_type,
            api_base,
            api_key,
            model,
            max_tokens,
        })
    }

    /// Load configuration for a specific model using hierarchical key resolution
    ///
    /// This method supports the hierarchical configuration structure where models
    /// can inherit settings from parent sections.
    ///
    /// # Arguments
    ///
    /// * `model_ref` - A model reference (e.g., "glm-5", "anthropic.glm.glm-5")
    ///
    /// # Examples
    ///
    /// ```toml
    /// [llm.provider]
    /// type = "anthropic"
    ///
    /// [llm.provider.anthropic.glm]
    /// api_base = "https://open.bigmodel.cn/api/paas/v4/"
    /// api_key = "sk-..."
    ///
    /// [llm.provider.anthropic.glm.glm-5]
    /// model = "glm-5"
    /// ```
    ///
    /// This example shows the hierarchical configuration structure.
    /// The actual loading requires proper environment setup.
    /// # See above
    /// ProviderConfig
    /// load_for_model
    pub fn load_for_model(model_ref: &str) -> anyhow::Result<(ModelConfig, String)> {
        let parsed = ModelReference::parse(model_ref)?;

        // Set up default values
        let mut defaults = HashMap::new();
        defaults.insert(
            "llm.provider.type".to_string(),
            toml::Value::String("openai".to_string()),
        );

        let config = ConfigBuilder::new()
            .with_prefix("EMX_LLM")
            .with_defaults(defaults)
            .build()?;

        // Resolve the model configuration hierarchically
        let model_config = Self::resolve_model_config(&config, &parsed, &[])
            .ok_or_else(|| anyhow::anyhow!("Model configuration not found for: {}", model_ref))?;

        let model_id = model_config.model.clone().unwrap_or_else(|| parsed.model_name.clone());

        Ok((model_config, model_id))
    }

    /// Resolve model configuration using hierarchical key lookup
    ///
    /// For a model reference like "anthropic.glm.glm-5", this searches:
    /// 1. llm.provider.anthropic.glm.glm-5.model
    /// 2. llm.provider.anthropic.glm.model
    /// 3. llm.provider.anthropic.model
    ///
    /// And for api_base:
    /// 1. llm.provider.anthropic.glm.glm-5.api_base
    /// 2. llm.provider.anthropic.glm.api_base
    /// 3. llm.provider.anthropic.api_base
    /// 4. Default URL for provider type
    fn resolve_model_config(
        config: &emx_config_core::Config,
        model_ref: &ModelReference,
        path_prefix: &[String],
    ) -> Option<ModelConfig> {
        // Build the search path for this model
        let mut search_path = path_prefix.to_vec();
        search_path.push(model_ref.model_name.clone());

        let provider_type = if let Some(pt) = &model_ref.provider_type {
            match pt.to_lowercase().as_str() {
                "openai" => Some(ProviderType::OpenAI),
                "anthropic" => Some(ProviderType::Anthropic),
                _ => None,
            }
        } else {
            None
        };

        // Determine the provider type - from explicit reference, config, or default
        let provider_type = provider_type.or_else(|| {
            // Try to get from config at this level
            let key = build_key(&search_path, "type");
            config.get_string(&key).ok().and_then(|s| {
                match s.to_lowercase().as_str() {
                    "openai" => Some(ProviderType::OpenAI),
                    "anthropic" => Some(ProviderType::Anthropic),
                    _ => None,
                }
            })
        });

        // Try to resolve at current path
        if let Some(resolved) = Self::try_resolve_at_level(config, &search_path, provider_type) {
            return Some(resolved);
        }

        // If we have more levels in the path, try going up
        if let Some(_prefix) = path_prefix.last() {
            let mut parent_path = path_prefix[..path_prefix.len() - 1].to_vec();
            // Include the intermediate section name (like "glm" from "anthropic.glm.glm-5")
            let parts: Vec<&str> = model_ref.full_path.split('.').collect();
            if parts.len() > 1 {
                parent_path.push(parts[parts.len() - 2].to_string());
            }
            return Self::resolve_model_config(config, model_ref, &parent_path);
        }

        // Try with the intermediate sections if we have a full path
        if model_ref.full_path.contains('.') {
            let parts: Vec<String> = model_ref
                .full_path
                .split('.')
                .map(|s| s.to_string())
                .collect();

            // Build path progressively through all sections
            for i in (0..parts.len() - 1).rev() {
                let section_path = &parts[..=i];
                let search_path = section_path.to_vec();
                if let Some(resolved) = Self::try_resolve_at_level(config, &search_path, provider_type) {
                    return Some(resolved);
                }
            }
        }

        None
    }

    /// Try to resolve configuration at a specific level in the hierarchy
    fn try_resolve_at_level(
        config: &emx_config_core::Config,
        search_path: &[String],
        provider_type: Option<ProviderType>,
    ) -> Option<ModelConfig> {
        let _base_key_prefix = "llm.provider";

        // Helper function to search for a key at current level and up the hierarchy
        let find_key = |key_suffix: &str| -> Option<String> {
            // Build the key for current level
            let mut parts = vec!["llm", "provider"];
            parts.extend(search_path.iter().map(|s| s.as_str()));
            parts.push(key_suffix);
            let direct_key = parts.join(".");

            if let Ok(v) = config.get_string(&direct_key) {
                return Some(v);
            }

            // Search up the hierarchy by removing parts from the end
            for i in (3..parts.len() - 1).rev() {
                let parent_key = format!("{}.{}", parts[..i].join("."), key_suffix);
                if let Ok(v) = config.get_string(&parent_key) {
                    return Some(v);
                }
            }

            None
        };

        // Get provider type
        let provider_type = provider_type.or_else(|| {
            find_key("type").and_then(|s| match s.to_lowercase().as_str() {
                "openai" => Some(ProviderType::OpenAI),
                "anthropic" => Some(ProviderType::Anthropic),
                _ => None,
            })
        });

        let provider_type = provider_type?;

        // Get api_key with hierarchical fallback
        let api_key = find_key("api_key").or_else(|| {
            // Fallback to legacy env vars
            let legacy_key = match provider_type {
                ProviderType::OpenAI => "OPENAI_API_KEY",
                ProviderType::Anthropic => "ANTHROPIC_AUTH_TOKEN",
            };
            std::env::var(legacy_key).ok()
        })?;

        // Get api_base with hierarchical fallback
        let api_base = find_key("api_base").or_else(|| {
            find_key("base_url").or_else(|| {
                // Fallback to legacy env vars
                let legacy_key = match provider_type {
                    ProviderType::OpenAI => "OPENAI_API_BASE",
                    ProviderType::Anthropic => "ANTHROPIC_BASE_URL",
                };
                std::env::var(legacy_key).ok()
            })
        }).unwrap_or_else(|| provider_type.default_base_url().to_string());

        // Get model name (may be None for provider-level config)
        let model = find_key("model");

        // Get max_tokens
        let max_tokens = find_key("max_tokens").and_then(|s| s.parse::<u32>().ok());

        Some(ModelConfig {
            provider_type,
            api_base,
            api_key,
            model,
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

    /// Get the model, if set
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
}

/// Load configuration with default settings
pub fn load_with_default() -> anyhow::Result<ProviderConfig> {
    ProviderConfig::load()
}

/// Model-specific configuration resolved from hierarchical config
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Provider type (OpenAI or Anthropic)
    pub provider_type: ProviderType,

    /// API base URL
    pub api_base: String,

    /// API key
    pub api_key: String,

    /// Model name (optional, may be inferred from section name)
    pub model: Option<String>,

    /// Maximum tokens for response
    pub max_tokens: Option<u32>,
}

impl ModelConfig {
    /// Get the max_tokens value, falling back to 4096 for Anthropic
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    /// Get the model name, or a default based on provider type
    pub fn model_name(&self) -> String {
        self.model.clone().unwrap_or_else(|| {
            match self.provider_type {
                ProviderType::OpenAI => "gpt-4".to_string(),
                ProviderType::Anthropic => "claude-3-opus-20240229".to_string(),
            }
        })
    }
}

/// A parsed model reference (e.g., "glm-5" or "anthropic.glm.glm-5")
#[derive(Debug, Clone)]
pub struct ModelReference {
    /// Full path as provided (e.g., "anthropic.glm.glm-5")
    pub full_path: String,

    /// Provider type if explicitly specified (e.g., "anthropic" from "anthropic.glm.glm-5")
    pub provider_type: Option<String>,

    /// Model name (last component of path, e.g., "glm-5")
    pub model_name: String,
}

impl ModelReference {
    /// Parse a model reference string
    ///
    /// # Examples
    ///
    /// ```
    /// # use emx_llm::ModelReference;
    /// let ref1 = ModelReference::parse("glm-5").unwrap();
    /// assert_eq!(ref1.full_path, "glm-5");
    /// assert_eq!(ref1.model_name, "glm-5");
    ///
    /// let ref2 = ModelReference::parse("anthropic.glm.glm-5").unwrap();
    /// assert_eq!(ref2.full_path, "anthropic.glm.glm-5");
    /// assert_eq!(ref2.provider_type, Some("anthropic".to_string()));
    /// assert_eq!(ref2.model_name, "glm-5");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("Model reference cannot be empty"));
        }

        // Case-insensitive: convert to lowercase for internal processing
        let input_lower = trimmed.to_lowercase();

        let parts: Vec<&str> = input_lower.split('.').collect();

        let (provider_type, model_name) = match parts.as_slice() {
            [model] => (None, model.to_string()),
            [provider, model] => {
                // Check if first part is a known provider
                match *provider {
                    "openai" | "anthropic" => (Some(provider.to_string()), model.to_string()),
                    _ => (None, trimmed.to_string()), // Treat as single model name
                }
            }
            [provider, _, .., model] if matches!(*provider, "openai" | "anthropic") => {
                (Some(provider.to_string()), model.to_string())
            }
            _ => (None, trimmed.to_string()),
        };

        Ok(ModelReference {
            full_path: input_lower,
            provider_type,
            model_name,
        })
    }
}

/// Build a configuration key from path components and a final key
fn build_key(path: &[String], key: &str) -> String {
    let mut parts = vec!["llm", "provider"];
    for part in path {
        parts.push(part);
    }
    parts.push(key);
    parts.join(".")
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

    #[test]
    fn test_model_reference_parse_simple() {
        let ref1 = ModelReference::parse("glm-5").unwrap();
        assert_eq!(ref1.full_path, "glm-5");
        assert_eq!(ref1.model_name, "glm-5");
        assert!(ref1.provider_type.is_none());
    }

    #[test]
    fn test_model_reference_parse_qualified() {
        let ref1 = ModelReference::parse("anthropic.glm-5").unwrap();
        assert_eq!(ref1.full_path, "anthropic.glm-5");
        assert_eq!(ref1.model_name, "glm-5");
        assert_eq!(ref1.provider_type, Some("anthropic".to_string()));
    }

    #[test]
    fn test_model_reference_parse_fully_qualified() {
        let ref1 = ModelReference::parse("anthropic.glm.glm-5").unwrap();
        assert_eq!(ref1.full_path, "anthropic.glm.glm-5");
        assert_eq!(ref1.model_name, "glm-5");
        assert_eq!(ref1.provider_type, Some("anthropic".to_string()));
    }

    #[test]
    fn test_model_reference_parse_case_insensitive() {
        let ref1 = ModelReference::parse("GLM-5").unwrap();
        assert_eq!(ref1.full_path, "glm-5");
        assert_eq!(ref1.model_name, "glm-5");

        let ref2 = ModelReference::parse("ANTHROPIC.GLM.GLM-5").unwrap();
        assert_eq!(ref2.full_path, "anthropic.glm.glm-5");
    }

    #[test]
    fn test_model_reference_parse_empty() {
        let result = ModelReference::parse("");
        assert!(result.is_err());
    }
}
