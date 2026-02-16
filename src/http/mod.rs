pub mod routes;

use anyhow::Result;
use axum::Router;
use axum::http::{header, Method};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::config::Config;

pub async fn serve(cfg: Config, port: u16) -> Result<()> {
    let origins: Vec<axum::http::HeaderValue> = [
            "https://ctxovrflw.dev",
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:3000",
            "http://127.0.0.1:3000",
        ]
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .max_age(std::time::Duration::from_secs(86400));

    let app = Router::new()
        .merge(routes::router())
        .nest("/mcp", crate::mcp::sse::router(cfg))
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(512 * 1024)); // 512 KB max request body

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    tracing::info!("HTTP API listening on http://localhost:{port}");
    tracing::info!("MCP SSE endpoint at http://localhost:{port}/mcp/sse");

    axum::serve(listener, app).await?;
    Ok(())
}
