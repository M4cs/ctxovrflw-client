use axum::{
    extract::Query,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::config::Config;

type SessionMap = Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>;

/// Create the MCP SSE router (mount under /mcp)
pub fn router(cfg: Config) -> Router {
    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));

    Router::new()
        .route("/sse", get({
            let sessions = sessions.clone();
            let cfg = cfg.clone();
            move || handle_sse(sessions, cfg)
        }))
        .route("/messages", post({
            let sessions = sessions.clone();
            let cfg = cfg.clone();
            move |query, body| handle_message(sessions, cfg, query, body)
        }))
}

/// GET /mcp/sse — establish SSE stream
async fn handle_sse(
    sessions: SessionMap,
    _cfg: Config,
) -> Sse<impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::channel::<String>(32);

    sessions.lock().await.insert(session_id.clone(), tx);

    let stream = async_stream::stream! {
        // First event: tell the client where to POST messages
        let endpoint = format!("/mcp/messages?sessionId={}", session_id);
        yield Ok(Event::default().event("endpoint").data(endpoint));

        // Stream responses back to client
        while let Some(msg) = rx.recv().await {
            yield Ok(Event::default().event("message").data(msg));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct MessageQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

/// POST /mcp/messages?sessionId=xxx — receive JSON-RPC from client
async fn handle_message(
    sessions: SessionMap,
    cfg: Config,
    Query(query): Query<MessageQuery>,
    body: String,
) -> impl IntoResponse {
    let tx = {
        let map = sessions.lock().await;
        map.get(&query.session_id).cloned()
    };

    let Some(tx) = tx else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "Session not found".to_string(),
        );
    };

    // Process through the shared handler
    match super::handle_message(&cfg, &body).await {
        Ok(Some(response)) => {
            // Send response via SSE
            if tx.send(response).await.is_err() {
                return (
                    axum::http::StatusCode::GONE,
                    "SSE connection closed".to_string(),
                );
            }
            (axum::http::StatusCode::ACCEPTED, "ok".to_string())
        }
        Ok(None) => {
            // Notification — no response needed
            (axum::http::StatusCode::ACCEPTED, "ok".to_string())
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {e}"),
        ),
    }
}
