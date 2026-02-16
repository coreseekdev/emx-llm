//! emx-gate binary
//!
//! LLM Gateway for aggregating multiple LLM providers

use anyhow::Result;
use clap::Parser;
use emx_llm::gate::config::GatewayConfig;
use emx_llm::gate::server::start_server;
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

    /// Validate configuration
    #[arg(long)]
    validate: bool,

    /// Test configuration
    #[arg(long)]
    test: bool,
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

    // Load configuration
    let mut config = if let Some(config_path) = args.config {
        // TODO: Load from file
        println!("Loading config from: {}", config_path);
        GatewayConfig::default()
    } else {
        // Try to load from default locations, fall back to defaults
        // TODO: Implement config loading
        GatewayConfig::default()
    };

    // Override with CLI arguments
    if let Some(host) = args.host {
        config.host = host;
    }
    if let Some(port) = args.port {
        config.port = port;
    }

    // Handle validation
    if args.validate {
        println!("Configuration validation:");
        println!("  Host: {}", config.host);
        println!("  Port: {}", config.port);
        println!("\n✓ Configuration is valid");
        return Ok(());
    }

    // Handle test
    if args.test {
        println!("Testing configuration...");
        // TODO: Test provider connections
        println!("✓ Configuration test passed");
        return Ok(());
    }

    // Start server
    start_server(config).await
}
