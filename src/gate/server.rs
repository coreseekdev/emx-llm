//! Gateway HTTP server

use crate::gate::config::GatewayConfig;
use axum::{
    extract::Request,
    middleware::{self, Next},
    routing::get,
    Json, Router,
};
use serde_json::json;
use serde_json::Value;
use std::net::SocketAddr;
use std::time::Instant;
use tracing::info;

/// Start the gateway server
pub async fn start_server(config: GatewayConfig) -> anyhow::Result<()> {
    // Build our application with routes
    let app = Router::new()
        .route("/health", get(health_check))
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
async fn health_check() -> Json<Value> {
    Json(json!({
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
