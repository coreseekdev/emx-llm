//! Test command implementation

use anyhow::Result;
use emx_llm::ProviderType;
use std::collections::HashMap;
use tracing::info;

/// Run the test command
pub fn run(provider: String) -> Result<()> {
    let provider_type = match provider.to_lowercase().as_str() {
        "openai" => ProviderType::OpenAI,
        "anthropic" => ProviderType::Anthropic,
        _ => {
            eprintln!("Unknown provider: {}", provider);
            eprintln!("Supported providers: openai, anthropic");
            std::process::exit(1);
        }
    };

    info!("Testing configuration for provider: {:?}", provider_type);

    // Build args to set provider type with fully nested structure
    let mut args = HashMap::new();
    let mut provider_table = toml::value::Table::new();
    provider_table.insert("type".to_string(), toml::Value::String(provider.to_lowercase()));
    let mut llm_table = toml::value::Table::new();
    llm_table.insert("provider".to_string(), toml::Value::Table(provider_table));
    args.insert("llm".to_string(), toml::Value::Table(llm_table));

    match emx_llm::ProviderConfig::load_with_args(Some(args)) {
        Ok(config) => {
            println!("Configuration loaded successfully:");
            println!("  Provider: {:?}", config.provider_type);
            println!("  API Base: {}", config.api_base);
            println!("  API Key: {}***", &config.api_key[..8.min(config.api_key.len())]);
            if let Some(model) = &config.model() {
                println!("  Default Model: {}", model);
            }
            println!();
            println!("Configuration is valid!");
        }
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            eprintln!();
            eprintln!("Make sure to set up your config.toml or environment variables:");
            eprintln!();
            eprintln!("config.toml:");
            eprintln!("  [llm.provider]");
            eprintln!("  type = \"{}\"", provider);
            eprintln!();
            eprintln!("  [llm.provider.{}]", provider);
            eprintln!("  api_base = \"...\"");
            eprintln!("  api_key = \"...\"");
            eprintln!("  model = \"...\"");
            eprintln!();
            eprintln!("Or set environment variables:");
            match provider_type {
                ProviderType::OpenAI => {
                    eprintln!("  export OPENAI_API_KEY=\"...\"");
                    eprintln!("  export OPENAI_API_BASE=\"...\"");
                }
                ProviderType::Anthropic => {
                    eprintln!("  export ANTHROPIC_AUTH_TOKEN=\"...\"");
                    eprintln!("  export ANTHROPIC_BASE_URL=\"...\"");
                }
            }
            std::process::exit(1);
        }
    }

    Ok(())
}
