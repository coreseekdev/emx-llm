//! emx-gate binary
//!
//! LLM Gateway for aggregating multiple LLM providers

use anyhow::Result;
use clap::Parser;
use emx_llm::gate::config::GatewayConfig;
use emx_llm::gate::server::start_server;
use emx_llm::ProviderConfig;
use std::path::Path;
use tracing_subscriber::{EnvFilter, fmt};

/// emx-gate: LLM Gateway for EMX
#[derive(Parser, Debug)]
#[command(name = "emx-gate")]
#[command(about = "LLM Gateway for EMX", long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Host to listen on
    #[arg(long)]
    host: Option<String>,

    /// Port to listen on
    #[arg(long)]
    port: Option<u16>,

    /// Request timeout in seconds
    #[arg(long)]
    timeout: Option<u64>,

    /// Validate configuration
    #[arg(long)]
    validate: bool,

    /// Test configuration (test provider connections)
    #[arg(long)]
    test: bool,
}

/// Load gateway configuration from file
fn load_gateway_config(config_path: &str) -> Result<GatewayConfig> {
    let content = std::fs::read_to_string(config_path)?;
    let config: GatewayConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Load provider configurations from file
fn load_provider_configs(config_path: &str) -> Result<Vec<(String, ProviderConfig)>> {
    ProviderConfig::list_models()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Determine config file path
    let config_file = args.config.clone().or_else(|| {
        // Try default locations
        let local = "./config.toml";
        if Path::new(local).exists() {
            Some(local.to_string())
        } else if let Some(home) = dirs::home_dir() {
            let home_config = format!("{}/.emx/config.toml", home.display());
            if Path::new(&home_config).exists() {
                Some(home_config)
            } else {
                None
            }
        } else {
            None
        }
    });

    // Load configuration
    let mut gateway_config = if let Some(ref config_path) = config_file {
        println!("Loading config from: {}", config_path);
        load_gateway_config(config_path)?
    } else {
        println!("Using default configuration");
        GatewayConfig::default()
    };

    // Override with CLI arguments
    if let Some(host) = args.host {
        gateway_config.host = host;
    }
    if let Some(port) = args.port {
        gateway_config.port = port;
    }
    if let Some(timeout) = args.timeout {
        gateway_config.timeout_secs = timeout;
    }

    // Handle validation
    if args.validate {
        validate_config(&gateway_config, config_file.as_deref()).await?;
        return Ok(());
    }

    // Handle test
    if args.test {
        test_config(&gateway_config).await?;
        return Ok(());
    }

    // Start server
    start_server(gateway_config).await
}

/// Validate configuration
async fn validate_config(config: &GatewayConfig, config_file: Option<&str>) -> Result<()> {
    println!("Configuration validation:");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!("  Timeout: {}s", config.timeout_secs);

    // Validate port range
    if config.port < 1024 || config.port > 65535 {
        anyhow::bail!("Invalid port: {} (must be between 1024 and 65535)", config.port);
    }

    // Validate timeout
    if config.timeout_secs < 10 || config.timeout_secs > 600 {
        anyhow::bail!("Invalid timeout: {} (must be between 10 and 600 seconds)", config.timeout_secs);
    }

    // Try to load provider configs
    if let Some(file) = config_file {
        match ProviderConfig::list_models() {
            Ok(models) => {
                println!("  Providers configured: {}", models.len());
                for (model_ref, _) in &models {
                    println!("    - {}", model_ref);
                }
            }
            Err(e) => {
                println!("  Warning: Could not load provider configs: {}", e);
            }
        }
    }

    println!("\n✓ Configuration is valid");
    Ok(())
}

/// Test configuration (test provider connections)
async fn test_config(config: &GatewayConfig) -> Result<()> {
    println!("Testing configuration...");

    // Load provider configs
    let models = match ProviderConfig::list_models() {
        Ok(m) => m,
        Err(e) => {
            println!("✗ Failed to load provider configurations: {}", e);
            return Ok(());
        }
    };

    if models.is_empty() {
        println!("Warning: No providers configured");
        println!("✓ Configuration test complete (no providers to test)");
        return Ok(());
    }

    println!("Testing {} provider(s)...", models.len());

    for (model_ref, model_config) in &models {
        print!("  Testing {} ... ", model_ref);
        
        // Test endpoint - /models for OpenAI, /v1/models for Anthropic
        let url = if model_config.provider_type == emx_llm::ProviderType::OpenAI {
            format!("{}/models", model_config.api_base.trim_end_matches('/'))
        } else {
            format!("{}/v1/models", model_config.api_base.trim_end_matches('/'))
        };
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        
        // Use API key if available (some APIs require it)
        let mut request = client.get(&url);
        if !model_config.api_key.is_empty() && model_config.api_key != "mock" {
            if model_config.provider_type == emx_llm::ProviderType::OpenAI {
                request = request.header("Authorization", format!("Bearer {}", model_config.api_key));
            } else {
                request = request.header("x-api-key", &model_config.api_key);
            }
        }
        
        match request.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    println!("OK");
                } else if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
                    // Auth error means we can reach the API
                    println!("OK (auth required)");
                } else {
                    println!("HTTP {}", resp.status());
                }
            }
            Err(e) => {
                // Network error
                if e.is_connect() {
                    println!("Connection failed");
                } else if e.is_timeout() {
                    println!("Timeout");
                } else {
                    println!("Error: {}", e);
                }
            }
        }
    }

    println!("\n✓ Configuration test complete");
    Ok(())
}
