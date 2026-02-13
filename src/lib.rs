//! # emx-llm
//!
//! LLM client library for EMX with support for multiple providers.
//!
//! ## Features
//!
//! - **Multiple providers**: OpenAI and Anthropic-compatible APIs
//! - **Streaming**: Server-sent events (SSE) for streaming responses
//! - **Flexible configuration**: Environment variables or config files
//! - **Async/await**: Built on tokio for efficient async operations
//!
//! ## Quick Start
//!
//! ```no_run
//! use emx_llm::{Client, Message, ProviderConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = ProviderConfig::openai(
//!         "https://open.bigmodel.cn/api/paas/v4/".to_string(),
//!         std::env::var("OPENAI_API_KEY")?
//!     );
//!
//!     let client = emx_llm::create_client(config)?;
//!
//!     let messages = vec![
//!         Message::user("Hello, LLM!"),
//!     ];
//!
//!     let (response, usage) = client.chat(&messages, "glm-4-flash").await?;
//!     println!("{}", response);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Configuration
//!
//! Set environment variables:
//!
//! ```sh
//! export OPENAI_API_KEY="your-api-key"
//! export OPENAI_API_BASE="https://open.bigmodel.cn/api/paas/v4/"
//! # or for Anthropic-compatible API
//! export ANTHROPIC_API_KEY="your-api-key"
//! export ANTHROPIC_API_BASE="https://open.bigmodel.cn/api/anthropic"
//! ```

pub mod client;
pub mod config;
pub mod message;
pub mod provider;

#[cfg(test)]
mod mock_server;
#[cfg(test)]
mod fixture_recorder;

pub use client::{Client, StreamEvent};
pub use config::{ProviderConfig, ProviderType};
pub use message::{Message, MessageRole, Usage};
pub use provider::create_client;

/// Result type for LLM operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in LLM operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// API returned an error
    #[error("API error: {0}")]
    Api(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}
