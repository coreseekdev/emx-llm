//! Gateway HTTP server

use crate::gate::anthropic_handlers_v2;
use crate::gate::config::GatewayConfig;
use crate::gate::handlers::{self, GatewayState};
use crate::gate::openai_handlers_v2;
use crate::gate::provider_handlers;
use crate::load_with_default;
use crate::ProviderConfig;
use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::signal;
use tracing::info;
use uuid::Uuid;

/// Start the gateway server
pub async fn start_server(config: GatewayConfig) -> anyhow::Result<()> {
    // Load provider configuration from config file
    let provider_config = load_with_default().map_err(|e| {
        tracing::warn!("Failed to load provider config, using default: {}", e);
        e
    })?;

    // Create GatewayState with loaded config
    let state = GatewayState {
        config: Arc::new(provider_config),
    };

    // Maximum request body size (10 MB) to prevent DoS attacks
    const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

    // Build our application with routes
    let app = Router::new()
        // OpenAI-compatible endpoints (using new passthrough handler)
        .route(
            "/openai/v1/chat/completions",
            post(openai_handlers_v2::chat_handler_passthrough),
        )
        .route("/openai/v1/models", get(provider_handlers::list_openai_models))
        // Anthropic-compatible endpoints (using new passthrough handler)
        .route(
            "/anthropic/v1/messages",
            post(anthropic_handlers_v2::messages_handler_passthrough),
        )
        .route("/anthropic/v1/models", get(provider_handlers::list_anthropic_models))
        // Utility endpoints
        .route("/health", get(health_check))
        .route("/v1/providers", get(handlers::list_providers))
        .with_state(state)
        // Apply request body size limit to prevent DoS
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_SIZE))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(logging_middleware));

    // Create socket address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid address");

    info!("Starting Gateway on http://{}", addr);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Gateway shutdown complete");
    Ok(())
}

/// Handle graceful shutdown signals
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Received shutdown signal, stopping server...");
}

/// Health check handler with provider status
async fn health_check() -> axum::Json<serde_json::Value> {
    // Try to get provider count
    let providers_count = ProviderConfig::list_models()
        .map(|m| m.len())
        .unwrap_or(0);
    
    axum::Json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "providers": providers_count
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

    // Extract request ID from headers (if set by previous middleware)
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    info!(
        request_id = %request_id,
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = duration.as_millis(),
        "{} {} {} {:?}",
        method, uri, status, duration
    );

    response
}

/// Request ID middleware - adds a unique ID to each request for tracing
async fn request_id_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    req.headers_mut().insert(
        "x-request-id",
        request_id.parse().unwrap(),
    );

    next.run(req).await
}
