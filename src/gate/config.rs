//! Gateway configuration

use serde::{Deserialize, Serialize};

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Host address to listen on
    #[serde(default = "default_host")]
    pub host: String,

    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,

    /// Request timeout in seconds (default: 120)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            timeout_secs: default_timeout(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8848
}

fn default_timeout() -> u64 {
    120
}
