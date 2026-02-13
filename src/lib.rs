//! Re-exports from all modules
mod client;
mod config;
mod message;
mod provider;

use thiserror::Error;

/// Result type for emx-llm operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for emx-llm operations
#[derive(Debug, Error)]
pub enum Error {
    /// API error
    #[error("API error: {0}")]
    Api(String),

    /// HTTP client error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}

pub use client::{Client, StreamEvent};
pub use config::{load_with_default, ModelConfig, ModelReference, ProviderConfig, ProviderType};
pub use message::{Message, MessageRole, Usage};
pub use provider::{create_client, create_client_for_model};
