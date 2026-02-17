pub mod routes;

use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::http::{header, Method};
use axum::middleware::{self, Next};
use axum::extract::Request;
use axum::response::{Response, IntoResponse};
use std::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::config::Config;
use crate::embed::Embedder;

/// Shared application state — loaded once at daemon startup.
#[derive(Clone)]
pub struct AppState {
    pub embedder: Option<Arc<Mutex<Embedder>>>,
    pub config: Config,
}

/// Auth middleware: checks Bearer token on all routes except /health and /.
async fn auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Skip auth for health and MCP endpoints (MCP is how external agents connect)
    if path == "/" || path == "/health" || path.starts_with("/mcp") {
        return next.run(request).await;
    }

    // Get expected token from config
    let expected_token = match Config::load() {
        Ok(cfg) => cfg.auth_token,
        Err(_) => None,
    };

    // If no token configured, allow all (backwards compat during migration)
    let Some(expected) = expected_token else {
        return next.run(request).await;
    };

    // Check Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Also check ?token= query param (for SSE clients that can't set headers)
    let query_token = request
        .uri()
        .query()
        .and_then(|q| {
            q.split('&')
                .find_map(|pair| {
                    let (k, v) = pair.split_once('=')?;
                    if k == "token" { Some(v.to_string()) } else { None }
                })
        });

    let authenticated = if let Some(auth) = auth_header {
        auth == format!("Bearer {expected}")
    } else if let Some(tok) = query_token {
        tok == expected
    } else {
        false
    };

    if !authenticated {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "Unauthorized" })),
        ).into_response();
    }

    next.run(request).await
}

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

    // Use the global singleton embedder — shared with sync, MCP, CLI
    let embedder = match crate::embed::get_or_init() {
        Ok(e) => {
            tracing::info!("ONNX embedder loaded (global singleton)");
            Some(e)
        }
        Err(e) => {
            tracing::warn!("Failed to load embedder: {e}. Semantic search unavailable.");
            None
        }
    };

    let state = AppState {
        embedder,
        config: cfg.clone(),
    };

    let app = Router::new()
        .merge(routes::router(state))
        .nest("/mcp", crate::mcp::sse::router(cfg))
        .layer(middleware::from_fn(auth_middleware))
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(512 * 1024)); // 512 KB max request body

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    tracing::info!("HTTP API listening on http://localhost:{port}");
    tracing::info!("MCP SSE endpoint at http://localhost:{port}/mcp/sse");

    axum::serve(listener, app).await?;
    Ok(())
}
