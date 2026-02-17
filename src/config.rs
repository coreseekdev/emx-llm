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
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (OpenAI or Anthropic)
    #[serde(rename = "type")]
    pub provider_type: ProviderType,

    /// API base URL
    pub api_base: String,

    /// API key (redacted in Debug output for security)
    #[serde(skip_serializing)]
    pub api_key: String,

    /// Model to use
    pub model: Option<String>,

    /// Maximum tokens for response (Anthropic requires this; default: 4096)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: Option<u32>,

    /// Request timeout in seconds (default: 120)
    #[serde(default = "default_timeout")]
    pub timeout_secs: Option<u64>,
}

fn default_timeout() -> Option<u64> {
    Some(120)
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact API key for security - only show first 8 chars if long enough
        let api_key_display = if self.api_key.len() > 8 {
            format!("{}***", &self.api_key[..8])
        } else if self.api_key.is_empty() {
            "(empty)".to_string()
        } else {
            "***".to_string()
        };

        f.debug_struct("ProviderConfig")
            .field("provider_type", &self.provider_type)
            .field("api_base", &self.api_base)
            .field("api_key", &api_key_display)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

fn default_max_tokens() -> Option<u32> {
    None
}

impl ProviderConfig {
    /// Get the max_tokens value, falling back to 4096 for Anthropic
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    /// Get the timeout duration
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs.unwrap_or(120))
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
                    anyhow::anyhow!(
                        "{} not found in config or environment",
                        format!("{}.api_key", base_key)
                    )
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
                    anyhow::anyhow!(
                        "{} not found in config or environment",
                        format!("{}.api_base", base_key)
                    )
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

        // Get timeout_secs
        let timeout_secs = config
            .get_int(&format!("{}.timeout_secs", base_key))
            .ok()
            .or_else(|| config.get_int("llm.provider.timeout_secs").ok())
            .map(|v| v as u64);

        Ok(ProviderConfig {
            provider_type,
            api_base,
            api_key,
            model,
            max_tokens,
            timeout_secs,
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

        // Load TOML config for hierarchical lookup
        let toml_value = Self::load_toml_config()?;

        // Set up default values
        let mut defaults = HashMap::new();
        defaults.insert(
            "llm.provider.type".to_string(),
            toml::Value::String("openai".to_string()),
        );

        // Build emx-config (environment variables + defaults)
        let config = ConfigBuilder::new()
            .with_prefix("EMX_LLM")
            .with_defaults(defaults)
            .build()?;

        // If full path provided (has provider prefix), resolve directly
        if parsed.provider_type.is_some() {
            // Try to resolve from TOML-based config
            let model_config = Self::resolve_model_config_from_toml(&toml_value, &parsed)
                .or_else(|| Self::resolve_model_config(&config, &parsed))
                .ok_or_else(|| {
                    anyhow::anyhow!("Model configuration not found for: {}", model_ref)
                })?;
            let model_id = model_config
                .model
                .clone()
                .unwrap_or_else(|| parsed.model_name.clone());
            return Ok((model_config, model_id));
        }

        // Short name: search for matching sections in TOML
        let matches = Self::find_sections_by_key(&toml_value, &parsed.model_name);

        match matches.len() {
            0 => Err(anyhow::anyhow!(
                "Model configuration not found for: {}",
                model_ref
            )),
            1 => {
                // Unique match - use it
                let full_ref = ModelReference {
                    full_path: matches[0].clone(),
                    provider_type: Some(
                        matches[0]
                            .split('.')
                            .next()
                            .unwrap_or("anthropic")
                            .to_string(),
                    ),
                    model_name: parsed.model_name.clone(),
                };
                let model_config =
                    Self::resolve_model_config(&config, &full_ref).ok_or_else(|| {
                        anyhow::anyhow!("Model configuration not found for: {}", model_ref)
                    })?;
                let model_id = model_config
                    .model
                    .clone()
                    .unwrap_or_else(|| parsed.model_name.clone());
                Ok((model_config, model_id))
            }
            _ => {
                // Multiple matches - report ambiguity
                let match_list: Vec<String> =
                    matches.iter().map(|p| format!("  - {}", p)).collect();
                Err(anyhow::anyhow!(
                    "Ambiguous model reference '{}'. Found {} matching sections:\n{}",
                    model_ref,
                    matches.len(),
                    match_list.join("\n")
                ))
            }
        }
    }

    /// Load TOML config file once, trying local then home directory
    fn load_toml_config() -> anyhow::Result<toml::Value> {
        let home_config = dirs::home_dir()
            .map(|p| {
                let mut path = p;
                path.push(".emx");
                path.push("config.toml");
                path.display().to_string()
            })
            .unwrap_or_default();

        let config_sources: Vec<&str> = vec!["./config.toml", &home_config];

        for source in config_sources {
            if let Ok(content) = std::fs::read_to_string(source) {
                if let Ok(toml_value) = content.parse::<toml::Value>() {
                    return Ok(toml_value);
                }
            }
        }

        // Return empty table if no config file found
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    /// Find all sections under that end with the given key
    /// Returns list of full paths (e.g., ["anthropic.glm.glm-5", "openai.models.glm-5"])
    fn find_sections_by_key(toml_value: &toml::Value, key: &str) -> Vec<String> {
        let mut matches = Vec::new();
        Self::search_toml_sections(toml_value, &["llm", "provider"], key, &mut matches);
        matches
    }

    /// Recursively search TOML structure for sections ending with target_key
    fn search_toml_sections(
        toml_value: &toml::Value,
        current_path: &[&str],
        target_key: &str,
        matches: &mut Vec<String>,
    ) {
        // Navigate to current path
        let mut current = Some(toml_value);
        for part in current_path {
            current = current.and_then(|v| v.get(part));
        }

        let Some(table) = current.and_then(|v| v.as_table()) else {
            return;
        };

        // Check each key in this table
        for (key, value) in table {
            let new_path: Vec<&str> = current_path
                .iter()
                .cloned()
                .chain(std::iter::once(key.as_str()))
                .collect();

            // If this key matches target and has a "model" field, it's a model section
            if key == target_key {
                if let Some(sub_table) = value.as_table() {
                    if sub_table.contains_key("model") {
                        // Build relative path from "llm.provider"
                        let relative_path = new_path[2..].join(".");
                        matches.push(relative_path);
                    }
                }
            }

            // Recurse into sub-tables (but not into the target_key itself to avoid infinite loop)
            if key != target_key && value.is_table() {
                Self::search_toml_sections(toml_value, &new_path, target_key, matches);
            }
        }
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
    ) -> Option<ModelConfig> {
        // Get path segments from full_path first
        let path_parts: Vec<String> = model_ref
            .full_path
            .split('.')
            .map(|s| s.to_string())
            .collect();

        // Determine provider type from explicit reference
        let explicit_provider_type = if let Some(pt) = &model_ref.provider_type {
            match pt.to_lowercase().as_str() {
                "openai" => Some(ProviderType::OpenAI),
                "anthropic" => Some(ProviderType::Anthropic),
                _ => None,
            }
        } else {
            None
        };

        // Try paths in order of specificity:
        // 1. Full path (e.g., ["anthropic", "glm", "glm-4-7"])
        // 2. Progressively shorter paths (e.g., ["anthropic", "glm"], ["anthropic"])
        // 3. Just model name (e.g., ["glm-4-7"])

        // Try full path first if we have multiple segments
        if path_parts.len() > 1 {
            if let Some(resolved) =
                Self::try_resolve_at_level(config, &path_parts, explicit_provider_type)
            {
                // Only accept if model field is set at this level
                if resolved.model.is_some() {
                    return Some(resolved);
                }
            }

            // Try progressively shorter paths
            for i in (0..path_parts.len() - 1).rev() {
                let search_path = path_parts[..=i].to_vec();
                if let Some(resolved) =
                    Self::try_resolve_at_level(config, &search_path, explicit_provider_type)
                {
                    return Some(resolved);
                }
            }
        }

        // Try with just model name
        let search_path = vec![model_ref.model_name.clone()];
        if let Some(resolved) =
            Self::try_resolve_at_level(config, &search_path, explicit_provider_type)
        {
            return Some(resolved);
        }

        None
    }

    /// Resolve model configuration from TOML config (loaded from file)
    fn resolve_model_config_from_toml(
        toml_value: &toml::Value,
        model_ref: &ModelReference,
    ) -> Option<ModelConfig> {
        let path_parts: Vec<String> = model_ref
            .full_path
            .split('.')
            .map(|s| s.to_string())
            .collect();

        let explicit_provider_type =
            model_ref
                .provider_type
                .as_ref()
                .and_then(|pt| match pt.to_lowercase().as_str() {
                    "openai" => Some(ProviderType::OpenAI),
                    "anthropic" => Some(ProviderType::Anthropic),
                    _ => None,
                });

        // Try full path first
        if path_parts.len() > 1 {
            if let Some(resolved) =
                Self::try_resolve_toml_at_level(toml_value, &path_parts, explicit_provider_type)
            {
                if resolved.model.is_some() {
                    return Some(resolved);
                }
            }

            // Try progressively shorter paths
            for i in (0..path_parts.len() - 1).rev() {
                let search_path = path_parts[..=i].to_vec();
                if let Some(resolved) = Self::try_resolve_toml_at_level(
                    toml_value,
                    &search_path,
                    explicit_provider_type,
                ) {
                    return Some(resolved);
                }
            }
        }

        // Try with just model name
        let search_path = vec![model_ref.model_name.clone()];
        Self::try_resolve_toml_at_level(toml_value, &search_path, explicit_provider_type)
    }

    /// Try to resolve configuration at a specific level from TOML
    fn try_resolve_toml_at_level(
        toml_value: &toml::Value,
        search_path: &[String],
        provider_type: Option<ProviderType>,
    ) -> Option<ModelConfig> {
        // Build the key path: llm.provider.${search_path}
        let mut key_parts: Vec<String> = vec!["llm".to_string(), "provider".to_string()];
        key_parts.extend(search_path.iter().cloned());
        let _key_path = key_parts.join(".");

        // Navigate to the section in TOML
        let mut current = Some(toml_value);
        for part in &key_parts {
            current = current.and_then(|v| v.get(part.as_str()));
        }

        let Some(section) = current.and_then(|v| v.as_table()) else {
            return None;
        };

        // Get provider type
        let provider_type = provider_type.or_else(|| {
            section
                .get("type")
                .and_then(|v| v.as_str())
                .and_then(|s| match s {
                    "openai" => Some(ProviderType::OpenAI),
                    "anthropic" => Some(ProviderType::Anthropic),
                    _ => None,
                })
        })?;

        // Get api_key - search current level and up
        let api_key = Self::find_toml_key(toml_value, &key_parts, "api_key").or_else(|| {
            let legacy_key = match provider_type {
                ProviderType::OpenAI => "OPENAI_API_KEY",
                ProviderType::Anthropic => "ANTHROPIC_AUTH_TOKEN",
            };
            std::env::var(legacy_key).ok()
        })?;

        // Get api_base
        let api_base = Self::find_toml_key(toml_value, &key_parts, "api_base")
            .or_else(|| Self::find_toml_key(toml_value, &key_parts, "base_url"))
            .or_else(|| {
                let legacy_key = match provider_type {
                    ProviderType::OpenAI => "OPENAI_API_BASE",
                    ProviderType::Anthropic => "ANTHROPIC_BASE_URL",
                };
                std::env::var(legacy_key).ok()
            })
            .unwrap_or_else(|| provider_type.default_base_url().to_string());

        // Get model name
        let model = section
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Get max_tokens
        let max_tokens = section
            .get("max_tokens")
            .and_then(|v| v.as_integer())
            .map(|v| v as u32);

        Some(ModelConfig {
            provider_type,
            api_base,
            api_key,
            model,
            max_tokens,
        })
    }

    /// Find a key in TOML by searching up the hierarchy
    fn find_toml_key(toml_value: &toml::Value, key_parts: &[String], key: &str) -> Option<String> {
        // Try at current level
        let mut current = Some(toml_value);
        for part in key_parts {
            current = current.and_then(|v| v.get(part.as_str()));
        }

        if let Some(table) = current.and_then(|v| v.as_table()) {
            if let Some(v) = table.get(key).and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }

        // Try parent levels
        for i in (2..key_parts.len()).rev() {
            let mut parent_parts = key_parts[..i].to_vec();
            parent_parts.push(key.to_string());
            let _search_key = parent_parts.join(".");

            let mut current = Some(toml_value);
            for part in &key_parts[..i] {
                current = current.and_then(|v| v.get(part.as_str()));
            }

            if let Some(table) = current.and_then(|v| v.as_table()) {
                if let Some(v) = table.get(key).and_then(|v| v.as_str()) {
                    return Some(v.to_string());
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
        let api_base = find_key("api_base")
            .or_else(|| {
                find_key("base_url").or_else(|| {
                    // Fallback to legacy env vars
                    let legacy_key = match provider_type {
                        ProviderType::OpenAI => "OPENAI_API_BASE",
                        ProviderType::Anthropic => "ANTHROPIC_BASE_URL",
                    };
                    std::env::var(legacy_key).ok()
                })
            })
            .unwrap_or_else(|| provider_type.default_base_url().to_string());

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

    /// List all configured models from TOML config
    /// Returns a list of (full_model_ref, model_config) tuples
    pub fn list_models() -> anyhow::Result<Vec<(String, ModelConfig)>> {
        let toml_value = Self::load_toml_config()?;
        let mut models = Vec::new();

        Self::collect_models_from_toml(&toml_value, &["llm", "provider"], "", &mut models);

        Ok(models)
    }

    /// Recursively collect model configurations from TOML
    fn collect_models_from_toml(
        toml_value: &toml::Value,
        current_path: &[&str],
        prefix: &str,
        models: &mut Vec<(String, ModelConfig)>,
    ) {
        let mut current = Some(toml_value);
        for part in current_path {
            current = current.and_then(|v| v.get(*part));
        }

        let Some(table) = current.and_then(|v| v.as_table()) else {
            return;
        };

        for (key, value) in table {
            let new_path: Vec<&str> = current_path
                .iter()
                .cloned()
                .chain(std::iter::once(key.as_str()))
                .collect();

            if let Some(sub_table) = value.as_table() {
                // If this has a "type" field, it's a provider section
                // If this has a "model" field (and type is above), it's a model section
                if sub_table.contains_key("api_base") || sub_table.contains_key("api_key") {
                    // This is a provider or sub-provider
                    let new_prefix = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::collect_models_from_toml(toml_value, &new_path, &new_prefix, models);
                } else if sub_table.contains_key("model") {
                    // This is a model section
                    let model_ref = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", prefix, key)
                    };

                    // Try to load this model's config
                    if let Ok((config, _)) = Self::load_for_model(&model_ref) {
                        models.push((model_ref, config));
                    }
                } else {
                    // Continue searching deeper
                    let new_prefix = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::collect_models_from_toml(toml_value, &new_path, &new_prefix, models);
                }
            }
        }
    }

    /// List all configured providers
    pub fn list_providers() -> anyhow::Result<Vec<(String, ProviderType)>> {
        let toml_value = Self::load_toml_config()?;
        let mut providers = Vec::new();

        // Navigate to llm.provider
        let provider_section = toml_value
            .get("llm")
            .and_then(|v| v.get("provider"))
            .and_then(|v| v.as_table());

        if let Some(table) = provider_section {
            for (key, value) in table {
                if let Some(sub_table) = value.as_table() {
                    // Check for type field
                    if let Some(type_value) = sub_table.get("type") {
                        if let Some(type_str) = type_value.as_str() {
                            match type_str.to_lowercase().as_str() {
                                "openai" => {
                                    providers.push((key.to_string(), ProviderType::OpenAI));
                                }
                                "anthropic" => {
                                    providers.push((key.to_string(), ProviderType::Anthropic));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(providers)
    }
}

/// Load configuration with default settings
pub fn load_with_default() -> anyhow::Result<ProviderConfig> {
    ProviderConfig::load()
}

/// Model-specific configuration resolved from hierarchical config
#[derive(Clone)]
pub struct ModelConfig {
    /// Provider type (OpenAI or Anthropic)
    pub provider_type: ProviderType,

    /// API base URL
    pub api_base: String,

    /// API key (redacted in Debug output for security)
    pub api_key: String,

    /// Model name (optional, may be inferred from section name)
    pub model: Option<String>,

    /// Maximum tokens for response
    pub max_tokens: Option<u32>,
}

impl std::fmt::Debug for ModelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact API key for security
        let api_key_display = if self.api_key.len() > 8 {
            format!("{}***", &self.api_key[..8])
        } else if self.api_key.is_empty() {
            "(empty)".to_string()
        } else {
            "***".to_string()
        };

        f.debug_struct("ModelConfig")
            .field("provider_type", &self.provider_type)
            .field("api_base", &self.api_base)
            .field("api_key", &api_key_display)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl ModelConfig {
    /// Get the max_tokens value, falling back to 4096 for Anthropic
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    /// Get the model name, or a default based on provider type
    pub fn model_name(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| match self.provider_type {
                ProviderType::OpenAI => "gpt-4".to_string(),
                ProviderType::Anthropic => "claude-3-opus-20240229".to_string(),
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

        // Check if input starts with a known provider prefix
        let (provider_type, full_path) = if input_lower.starts_with("anthropic.") {
            (Some("anthropic".to_string()), input_lower.clone())
        } else if input_lower.starts_with("openai.") {
            (Some("openai".to_string()), input_lower.clone())
        } else {
            (None, input_lower.clone())
        };

        // Model name is the last segment after "."
        let model_name = full_path
            .split('.')
            .last()
            .unwrap_or(&full_path)
            .to_string();

        Ok(ModelReference {
            full_path,
            provider_type,
            model_name,
        })
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
