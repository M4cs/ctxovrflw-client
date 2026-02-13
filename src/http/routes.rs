use axum::{
    extract::{Json, Path, Query},
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use chrono::Utc;

use crate::config::Config;
use crate::db;

/// Parse a TTL string like "1h", "24h", "7d", "30m" into an expiry timestamp.
fn parse_ttl(ttl: &str) -> Result<String, String> {
    let ttl = ttl.trim().to_lowercase();
    let (num_str, multiplier) = if ttl.ends_with('d') {
        (&ttl[..ttl.len() - 1], 86400i64)
    } else if ttl.ends_with('h') {
        (&ttl[..ttl.len() - 1], 3600i64)
    } else if ttl.ends_with('m') {
        (&ttl[..ttl.len() - 1], 60i64)
    } else if ttl.ends_with('s') {
        (&ttl[..ttl.len() - 1], 1i64)
    } else {
        return Err(format!("Invalid TTL format: '{ttl}'. Use '1h', '24h', '7d', '30m'"));
    };
    let num: i64 = num_str.parse().map_err(|_| format!("Invalid TTL number: '{num_str}'"))?;
    if num <= 0 { return Err("TTL must be positive".into()); }
    let expires = Utc::now() + chrono::Duration::seconds(num * multiplier);
    Ok(expires.to_rfc3339())
}

fn resolve_expiry(ttl: Option<&str>, expires_at: Option<&str>) -> Result<Option<String>, String> {
    if let Some(t) = ttl { return Ok(Some(parse_ttl(t)?)); }
    if let Some(e) = expires_at {
        chrono::DateTime::parse_from_rfc3339(e)
            .map_err(|_| "Invalid expires_at: must be ISO 8601 / RFC 3339".to_string())?;
        return Ok(Some(e.to_string()));
    }
    Ok(None)
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/v1/memories", post(store_memory))
        .route("/v1/memories", get(list_memories))
        .route("/v1/memories/recall", post(recall))
        .route("/v1/memories/{id}", get(get_memory))
        .route("/v1/memories/{id}", put(update_memory))
        .route("/v1/memories/{id}", delete(delete_memory))
        .route("/v1/subjects", get(subjects))
        .route("/v1/status", get(status))
}

async fn health() -> Json<Value> {
    Json(json!({
        "service": "ctxovrflw",
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[derive(Deserialize)]
struct StoreRequest {
    content: String,
    #[serde(rename = "type", default)]
    memory_type: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    ttl: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

async fn store_memory(Json(body): Json<StoreRequest>) -> Json<Value> {
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("Config error: {e}") })),
    };

    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("DB error: {e}") })),
    };

    // Check memory limit
    let count = db::memories::count(&conn).unwrap_or(0);
    if let Some(max) = cfg.tier.max_memories() {
        if count >= max {
            return Json(json!({
                "ok": false,
                "error": format!("Memory limit reached ({max}). Upgrade at https://ctxovrflw.dev/pricing")
            }));
        }
    }

    let mtype = body
        .memory_type
        .as_deref()
        .unwrap_or("semantic")
        .parse()
        .unwrap_or_default();
    let source = body.source.as_deref().unwrap_or("api");

    // Generate embedding if semantic search available
    let embedding = if cfg.tier.semantic_search_enabled() {
        crate::embed::Embedder::new()
            .ok()
            .and_then(|mut e| e.embed(&body.content).ok())
    } else {
        None
    };

    let expires_at = match resolve_expiry(body.ttl.as_deref(), body.expires_at.as_deref()) {
        Ok(e) => e,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    match db::memories::store_with_expiry(&conn, &body.content, &mtype, &body.tags, body.subject.as_deref(), Some(source), embedding.as_deref(), expires_at.as_deref()) {
        Ok(memory) => {
            // Immediate push to cloud if logged in
            if cfg.is_logged_in() {
                let id = memory.id.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move {
                    let _ = crate::sync::push_one(&cfg2, &id).await;
                });
            }
            Json(json!({ "ok": true, "memory": memory }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    20
}

async fn list_memories(Query(q): Query<ListQuery>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    let limit = q.limit.min(100);
    match db::memories::list(&conn, limit, q.offset) {
        Ok(memories) => {
            let total = db::memories::count(&conn).unwrap_or(0);
            Json(json!({ "ok": true, "memories": memories, "total": total, "limit": limit, "offset": q.offset }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct RecallRequest {
    query: String,
    #[serde(default = "default_recall_limit")]
    limit: usize,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    subject: Option<String>,
}

fn default_recall_limit() -> usize {
    10
}

async fn recall(Json(body): Json<RecallRequest>) -> Json<Value> {
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    // Sync before recall to get latest from other devices
    if cfg.is_logged_in() {
        let _ = crate::sync::run_silent(&cfg).await;
    }

    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    use crate::db::search::SearchMethod;

    // Subject-scoped search
    if let Some(ref subj) = body.subject {
        let memories = db::search::by_subject(&conn, subj, body.limit).unwrap_or_default();
        let results_json: Vec<Value> = memories
            .iter()
            .map(|memory| json!({ "memory": memory, "score": 1.0 }))
            .collect();
        return Json(json!({ "ok": true, "results": results_json, "search_method": "subject" }));
    }

    let fetch_limit = if body.max_tokens.is_some() { body.limit.max(20) } else { body.limit };

    let (results, method) = if cfg.tier.semantic_search_enabled() {
        match crate::embed::Embedder::new() {
            Ok(mut embedder) => match embedder.embed(&body.query) {
                Ok(embedding) => {
                    let sem = db::search::semantic_search(&conn, &embedding, fetch_limit).unwrap_or_default();
                    if !sem.is_empty() {
                        (sem, SearchMethod::Semantic)
                    } else {
                        (db::search::keyword_search(&conn, &body.query, fetch_limit).unwrap_or_default(), SearchMethod::Keyword)
                    }
                }
                Err(_) => (db::search::keyword_search(&conn, &body.query, fetch_limit)
                    .unwrap_or_default(), SearchMethod::Keyword),
            },
            Err(_) => (db::search::keyword_search(&conn, &body.query, fetch_limit)
                .unwrap_or_default(), SearchMethod::Keyword),
        }
    } else {
        (db::search::keyword_search(&conn, &body.query, fetch_limit).unwrap_or_default(), SearchMethod::Keyword)
    };

    // Apply token budget if specified
    let filtered: Vec<&(db::memories::Memory, f64)> = if let Some(budget) = body.max_tokens {
        let mut token_count = 0usize;
        results.iter().take_while(|(mem, _)| {
            let tokens = mem.content.len() / 4;
            if token_count + tokens > budget { return false; }
            token_count += tokens;
            true
        }).collect()
    } else {
        results.iter().take(body.limit).collect()
    };

    let results_json: Vec<Value> = filtered
        .iter()
        .map(|(memory, score)| json!({ "memory": memory, "score": score }))
        .collect();

    Json(json!({ "ok": true, "results": results_json, "search_method": method.to_string() }))
}

async fn get_memory(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    match db::memories::get(&conn, &id) {
        Ok(Some(memory)) => Json(json!({ "ok": true, "memory": memory })),
        Ok(None) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn delete_memory(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    match db::memories::delete(&conn, &id) {
        Ok(true) => Json(json!({ "ok": true })),
        Ok(false) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct UpdateRequest {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    ttl: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    remove_expiry: Option<bool>,
}

async fn update_memory(Path(id): Path<String>, Json(body): Json<UpdateRequest>) -> Json<Value> {
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("Config error: {e}") })),
    };

    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("DB error: {e}") })),
    };

    // Resolve expiry
    let expires_at = if body.remove_expiry.unwrap_or(false) {
        Some(None) // clear expiry
    } else if body.ttl.is_some() || body.expires_at.is_some() {
        match resolve_expiry(body.ttl.as_deref(), body.expires_at.as_deref()) {
            Ok(Some(e)) => Some(Some(e)),
            Ok(None) => None,
            Err(e) => return Json(json!({ "ok": false, "error": e })),
        }
    } else {
        None
    };

    // Re-embed if content changed
    let embedding = if let Some(ref c) = body.content {
        if cfg.tier.semantic_search_enabled() {
            crate::embed::Embedder::new()
                .ok()
                .and_then(|mut e| e.embed(c).ok())
        } else { None }
    } else { None };

    let subject = if body.subject.is_some() {
        Some(body.subject.as_deref())
    } else { None };

    let expires_ref = expires_at.as_ref().map(|e| e.as_deref());

    match db::memories::update(&conn, &id, body.content.as_deref(), body.tags.as_deref(), subject, expires_ref, embedding.as_deref()) {
        Ok(Some(memory)) => {
            if cfg.is_logged_in() {
                let mid = memory.id.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move {
                    let _ = crate::sync::push_one(&cfg2, &mid).await;
                });
            }
            Json(json!({ "ok": true, "memory": memory }))
        }
        Ok(None) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn subjects() -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };

    match db::search::list_subjects(&conn) {
        Ok(subjects) => {
            let list: Vec<Value> = subjects
                .iter()
                .map(|(name, count)| json!({ "subject": name, "memories": count }))
                .collect();
            Json(json!({ "ok": true, "subjects": list }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn status() -> Json<Value> {
    let cfg = Config::load().unwrap_or_default();
    let conn = db::open().ok();
    let count = conn
        .as_ref()
        .and_then(|c| db::memories::count(c).ok())
        .unwrap_or(0);
    let max = cfg
        .tier
        .max_memories()
        .map(|m| Value::Number(m.into()))
        .unwrap_or(Value::String("unlimited".into()));

    Json(json!({
        "service": "ctxovrflw",
        "version": env!("CARGO_PKG_VERSION"),
        "tier": format!("{:?}", cfg.tier),
        "memories": count,
        "memories_limit": max,
        "semantic_search": cfg.tier.semantic_search_enabled(),
        "cloud_sync": cfg.tier.cloud_sync_enabled(),
    }))
}
