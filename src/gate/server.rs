//! Gateway HTTP server

use crate::gate::config::GatewayConfig;
use crate::gate::handlers::{self, GatewayState};
use axum::{
    extract::Request,
    middleware::{self, Next},
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Start the gateway server
pub async fn start_server(config: GatewayConfig) -> anyhow::Result<()> {
    // TODO: Load actual provider configuration
    let state = GatewayState {
        config: Arc::new(
            serde_json::from_str::<crate::ProviderConfig>(r#"{
                "type": "openai",
                "api_base": "https://api.openai.com/v1",
                "api_key": "mock",
                "model": "gpt-4"
            }"#)
            .unwrap(),
        ),
    };

    // Build our application with routes
    let app = Router::new()
        // OpenAI-compatible endpoints
        .route("/v1/chat/completions", post(handlers::openai_chat_handler))
        .route("/v1/chat/completions", get(handlers::openai_chat_stream_handler))
        // Anthropic-compatible endpoints
        .route("/v1/messages", post(handlers::anthropic_messages_handler))
        // Utility endpoints
        .route("/health", get(health_check))
        .route("/v1/models", get(handlers::list_models))
        .route("/v1/providers", get(handlers::list_providers))
        .with_state(state)
        .layer(middleware::from_fn(logging_middleware));

    // Create socket address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid address");

    info!("Starting Gateway on http://{}", addr);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check handler
async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Logging middleware
async fn logging_middleware(
    req: Request,
    next: Next,
) -> axum::response::Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    info!("{} {} {} {:?}", method, uri, status, duration);

    response
}
