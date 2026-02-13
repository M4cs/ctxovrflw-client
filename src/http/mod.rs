pub mod routes;

use anyhow::Result;
use axum::Router;
use tower_http::cors::CorsLayer;

use crate::config::Config;

pub async fn serve(cfg: Config, port: u16) -> Result<()> {
    let app = Router::new()
        .merge(routes::router())
        .nest("/mcp", crate::mcp::sse::router(cfg))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    tracing::info!("HTTP API listening on http://127.0.0.1:{port}");
    tracing::info!("MCP SSE endpoint at http://127.0.0.1:{port}/mcp/sse");

    axum::serve(listener, app).await?;
    Ok(())
}
