use axum::{
    extract::{Json, Path, Query, State},
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use chrono::Utc;

use crate::config::Config;
use crate::db;
use super::AppState;

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



/// Sanitize error messages to avoid leaking internal paths or implementation details.
fn sanitize_error(e: &impl std::fmt::Display) -> String {
    let msg = e.to_string();
    // Strip file paths
    if msg.contains('/') || msg.contains("\\\\") {
        return "Internal error".to_string();
    }
    msg
}

pub fn router(state: AppState) -> Router {
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
        .route("/v1/entities", get(list_entities_http))
        .route("/v1/entities", post(create_entity))
        .route("/v1/entities/{id}", get(get_entity_http))
        .route("/v1/entities/{id}", delete(delete_entity_http))
        .route("/v1/relations", post(create_relation))
        .route("/v1/relations/{entity_id}", get(get_relations_http))
        .route("/v1/relations/{id}/delete", delete(delete_relation_http))
        .route("/v1/graph/traverse/{entity_id}", get(traverse_http))
        .route("/v1/webhooks", get(list_webhooks))
        .route("/v1/webhooks", post(create_webhook))
        .route("/v1/webhooks/{id}", delete(delete_webhook))
        .route("/v1/status", get(status))
        .with_state(state)
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

/// Maximum memory content size (100 KB). Prevents unbounded allocation from oversized payloads.
const MAX_CONTENT_SIZE: usize = 100 * 1024;
const MAX_TAG_LENGTH: usize = 200;
const MAX_TAGS: usize = 50;
const MAX_SUBJECT_LENGTH: usize = 500;

/// Deduplicate and validate tags. Returns cleaned tags or an error message.
fn validate_tags(tags: &[String]) -> Result<Vec<String>, String> {
    if tags.len() > MAX_TAGS {
        return Err(format!("Too many tags ({}). Maximum is {}.", tags.len(), MAX_TAGS));
    }
    for tag in tags {
        if tag.len() > MAX_TAG_LENGTH {
            return Err(format!("Tag too long ({} chars). Maximum is {} chars.", tag.len(), MAX_TAG_LENGTH));
        }
    }
    let mut deduped: Vec<String> = tags.to_vec();
    deduped.sort();
    deduped.dedup();
    Ok(deduped)
}

fn validate_subject(subject: Option<&str>) -> Result<(), String> {
    if let Some(s) = subject {
        if s.len() > MAX_SUBJECT_LENGTH {
            return Err(format!("Subject too long ({} chars). Maximum is {} chars.", s.len(), MAX_SUBJECT_LENGTH));
        }
    }
    Ok(())
}

async fn store_memory(State(state): State<AppState>, Json(body): Json<StoreRequest>) -> Json<Value> {
    if body.content.trim().is_empty() {
        return Json(json!({ "ok": false, "error": "Content cannot be empty" }));
    }
    if body.content.len() > MAX_CONTENT_SIZE {
        return Json(json!({ "ok": false, "error": format!("Content too large ({} bytes). Maximum is {} bytes.", body.content.len(), MAX_CONTENT_SIZE) }));
    }
    let tags = match validate_tags(&body.tags) {
        Ok(t) => t,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    if let Err(e) = validate_subject(body.subject.as_deref()) {
        return Json(json!({ "ok": false, "error": e }));
    }

    let cfg = &state.config;

    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": sanitize_error(&e) })),
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

    // Generate embedding using shared embedder
    let embedding = if let Some(ref emb) = state.embedder {
        let mut e = emb.lock().await;
        e.embed(&body.content).ok()
    } else {
        None
    };

    let expires_at = match resolve_expiry(body.ttl.as_deref(), body.expires_at.as_deref()) {
        Ok(e) => e,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    match db::memories::store_with_expiry(&conn, &body.content, &mtype, &tags, body.subject.as_deref(), Some(source), embedding.as_deref(), expires_at.as_deref()) {
        Ok(memory) => {
            crate::webhooks::fire("memory.created", json!({ "memory": memory }));
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

async fn recall(State(state): State<AppState>, Json(body): Json<RecallRequest>) -> Json<Value> {
    let cfg = &state.config;

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

    let (results, method) = if let Some(ref emb) = state.embedder {
        let mut embedder = emb.lock().await;
        match embedder.embed(&body.query) {
            Ok(embedding) => {
                drop(embedder); // Release lock before DB query
                let sem = db::search::semantic_search(&conn, &embedding, fetch_limit).unwrap_or_default();
                if !sem.is_empty() {
                    (sem, SearchMethod::Semantic)
                } else {
                    (db::search::keyword_search(&conn, &body.query, fetch_limit).unwrap_or_default(), SearchMethod::Keyword)
                }
            }
            Err(_) => {
                drop(embedder);
                (db::search::keyword_search(&conn, &body.query, fetch_limit).unwrap_or_default(), SearchMethod::Keyword)
            }
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
        Ok(true) => {
            crate::webhooks::fire("memory.deleted", json!({ "memory_id": id }));
            Json(json!({ "ok": true }))
        }
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

async fn update_memory(State(state): State<AppState>, Path(id): Path<String>, Json(body): Json<UpdateRequest>) -> Json<Value> {
    let cfg = &state.config;

    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": sanitize_error(&e) })),
    };

    // Validate tags and subject if provided
    let validated_tags = if let Some(ref tags) = body.tags {
        match validate_tags(tags) {
            Ok(t) => Some(t),
            Err(e) => return Json(json!({ "ok": false, "error": e })),
        }
    } else {
        None
    };
    if let Err(e) = validate_subject(body.subject.as_deref()) {
        return Json(json!({ "ok": false, "error": e }));
    }

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
        if let Some(ref emb) = state.embedder {
            let mut e = emb.lock().await;
            e.embed(c).ok()
        } else { None }
    } else { None };

    let subject = if body.subject.is_some() {
        Some(body.subject.as_deref())
    } else { None };

    let expires_ref = expires_at.as_ref().map(|e| e.as_deref());

    match db::memories::update(&conn, &id, body.content.as_deref(), validated_tags.as_deref(), subject, expires_ref, embedding.as_deref()) {
        Ok(Some(memory)) => {
            crate::webhooks::fire("memory.updated", json!({ "memory": memory }));
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

// ── Knowledge Graph routes ──────────────────────────────────

#[derive(Deserialize)]
struct CreateEntityRequest {
    name: String,
    #[serde(rename = "type", default = "default_entity_type")]
    entity_type: String,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn default_entity_type() -> String {
    "generic".to_string()
}

async fn create_entity(Json(body): Json<CreateEntityRequest>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::upsert_entity(&conn, &body.name, &body.entity_type, body.metadata.as_ref()) {
        Ok(entity) => {
            crate::webhooks::fire("entity.created", json!({ "entity": entity }));
            Json(json!({ "ok": true, "entity": entity }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct ListEntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default = "default_entity_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_entity_limit() -> usize {
    50
}

async fn list_entities_http(Query(q): Query<ListEntitiesQuery>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    let limit = q.limit.min(200);
    let entities = if let Some(ref query) = q.query {
        db::graph::search_entities(&conn, query, q.entity_type.as_deref(), limit)
    } else {
        db::graph::list_entities(&conn, q.entity_type.as_deref(), limit, q.offset)
    };
    match entities {
        Ok(entities) => {
            let total = db::graph::count_entities(&conn).unwrap_or(0);
            Json(json!({ "ok": true, "entities": entities, "total": total }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn get_entity_http(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::get_entity(&conn, &id) {
        Ok(Some(entity)) => Json(json!({ "ok": true, "entity": entity })),
        Ok(None) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn delete_entity_http(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::delete_entity(&conn, &id) {
        Ok(true) => {
            crate::webhooks::fire("entity.deleted", json!({ "entity_id": id }));
            Json(json!({ "ok": true }))
        }
        Ok(false) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct CreateRelationRequest {
    source_id: String,
    target_id: String,
    relation_type: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    source_memory_id: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn default_confidence() -> f64 {
    1.0
}

async fn create_relation(Json(body): Json<CreateRelationRequest>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::upsert_relation(
        &conn,
        &body.source_id,
        &body.target_id,
        &body.relation_type,
        body.confidence,
        body.source_memory_id.as_deref(),
        body.metadata.as_ref(),
    ) {
        Ok(relation) => {
            crate::webhooks::fire("relation.created", json!({ "relation": relation }));
            Json(json!({ "ok": true, "relation": relation }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct GetRelationsQuery {
    #[serde(default)]
    relation_type: Option<String>,
    #[serde(default)]
    direction: Option<String>,
}

async fn get_relations_http(Path(entity_id): Path<String>, Query(q): Query<GetRelationsQuery>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    let direction = q.direction.as_deref();
    match db::graph::get_relations(&conn, &entity_id, q.relation_type.as_deref(), direction) {
        Ok(relations) => {
            let results: Vec<Value> = relations
                .iter()
                .map(|(rel, source, target)| json!({
                    "relation": rel,
                    "source": source,
                    "target": target,
                }))
                .collect();
            Json(json!({ "ok": true, "relations": results }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn delete_relation_http(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::delete_relation(&conn, &id) {
        Ok(true) => {
            crate::webhooks::fire("relation.deleted", json!({ "relation_id": id }));
            Json(json!({ "ok": true }))
        }
        Ok(false) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct TraverseQuery {
    #[serde(default = "default_max_depth")]
    max_depth: usize,
    #[serde(default)]
    relation_type: Option<String>,
    #[serde(default)]
    min_confidence: f64,
}

fn default_max_depth() -> usize {
    2
}

async fn traverse_http(Path(entity_id): Path<String>, Query(q): Query<TraverseQuery>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::graph::traverse(&conn, &entity_id, q.max_depth, q.relation_type.as_deref(), q.min_confidence) {
        Ok(nodes) => Json(json!({ "ok": true, "nodes": nodes, "total": nodes.len() })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

// ── Webhook routes ──────────────────────────────────────────

async fn list_webhooks() -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::webhooks::list(&conn) {
        Ok(hooks) => Json(json!({ "ok": true, "webhooks": hooks })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

#[derive(Deserialize)]
struct CreateWebhookRequest {
    url: String,
    events: Vec<String>,
    #[serde(default)]
    secret: Option<String>,
}

async fn create_webhook(Json(body): Json<CreateWebhookRequest>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::webhooks::create(&conn, &body.url, &body.events, body.secret.as_deref()) {
        Ok(hook) => Json(json!({ "ok": true, "webhook": hook })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}

async fn delete_webhook(Path(id): Path<String>) -> Json<Value> {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => return Json(json!({ "ok": false, "error": format!("{e}") })),
    };
    match db::webhooks::delete(&conn, &id) {
        Ok(true) => Json(json!({ "ok": true })),
        Ok(false) => Json(json!({ "ok": false, "error": "Not found" })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e}") })),
    }
}
