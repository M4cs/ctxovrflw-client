use std::path::PathBuf;
use std::sync::Once;

static INIT_VEC: Once = Once::new();

fn init_sqlite_vec() {
    INIT_VEC.call_once(|| {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Helper: create a temporary database for testing (with sqlite-vec)
fn test_db() -> (rusqlite::Connection, tempfile::TempDir) {
    init_sqlite_vec();

    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test_memories.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS memories (
            id          TEXT PRIMARY KEY,
            content     TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'semantic',
            tags        TEXT NOT NULL DEFAULT '[]',
            subject     TEXT,
            source      TEXT,
            embedding   BLOB,
            expires_at  TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            synced_at   TEXT,
            deleted     INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(type);
        CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
        CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted);

        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            tags,
            content='memories',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE VIRTUAL TABLE IF NOT EXISTS memory_vectors USING vec0(
            id TEXT PRIMARY KEY,
            embedding float[384]
        );
        ",
    )
    .unwrap();

    // Graph tables
    ctxovrflw::db::graph::migrate(&conn).unwrap();

    // Webhook tables
    ctxovrflw::db::webhooks::migrate(&conn).unwrap();

    (conn, tmp)
}

/// Helper: generate a simple deterministic embedding for testing
/// Creates a unit vector with energy concentrated at specific dimensions
/// based on a seed, so different seeds produce different but comparable vectors
fn test_embedding(seed: u32) -> Vec<f32> {
    let mut emb = vec![0.0f32; 384];
    // Spread energy across dimensions based on seed
    for i in 0..32 {
        let idx = ((seed as usize * 7 + i * 13) % 384) as usize;
        let sign = if (seed as usize + i) % 2 == 0 { 1.0 } else { -1.0 };
        emb[idx] = sign * (1.0 / (1.0 + i as f32 * 0.3));
    }
    // L2 normalize
    let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut emb {
            *v /= norm;
        }
    }
    emb
}

/// Helper: cosine similarity between two vectors (for verification)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
    dot / (norm_a * norm_b).max(1e-9)
}

// ============================================================
// Memory CRUD Tests
// ============================================================

#[test]
fn test_store_memory() {
    let (conn, _tmp) = test_db();

    let mem = ctxovrflw::db::memories::store(
        &conn,
        "Max prefers TypeScript",
        &ctxovrflw::db::memories::MemoryType::Preference,
        &["coding".to_string()],
        None,
        Some("test"),
        None,
    )
    .unwrap();

    assert!(!mem.id.is_empty());
    assert_eq!(mem.content, "Max prefers TypeScript");
    assert_eq!(mem.tags, vec!["coding".to_string()]);
    assert_eq!(mem.source, Some("test".to_string()));
}

#[test]
fn test_get_memory() {
    let (conn, _tmp) = test_db();

    let stored = ctxovrflw::db::memories::store(
        &conn,
        "Rust is fast",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[],
        None,
        None,
        None,
    )
    .unwrap();

    let retrieved = ctxovrflw::db::memories::get(&conn, &stored.id)
        .unwrap()
        .expect("Memory should exist");

    assert_eq!(retrieved.id, stored.id);
    assert_eq!(retrieved.content, "Rust is fast");
}

#[test]
fn test_get_nonexistent_memory() {
    let (conn, _tmp) = test_db();

    let result = ctxovrflw::db::memories::get(&conn, "nonexistent-id").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_delete_memory() {
    let (conn, _tmp) = test_db();

    let mem = ctxovrflw::db::memories::store(
        &conn,
        "To be deleted",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[],
        None,
        None,
        None,
    )
    .unwrap();

    assert!(ctxovrflw::db::memories::delete(&conn, &mem.id).unwrap());

    // Should be soft-deleted (not found by get)
    let result = ctxovrflw::db::memories::get(&conn, &mem.id).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_delete_nonexistent() {
    let (conn, _tmp) = test_db();
    assert!(!ctxovrflw::db::memories::delete(&conn, "nope").unwrap());
}

#[test]
fn test_count_memories() {
    let (conn, _tmp) = test_db();

    assert_eq!(ctxovrflw::db::memories::count(&conn).unwrap(), 0);

    ctxovrflw::db::memories::store(
        &conn, "First", &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    ctxovrflw::db::memories::store(
        &conn, "Second", &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    assert_eq!(ctxovrflw::db::memories::count(&conn).unwrap(), 2);

    // Delete one
    let mems = ctxovrflw::db::memories::list(&conn, 10, 0).unwrap();
    ctxovrflw::db::memories::delete(&conn, &mems[0].id).unwrap();

    assert_eq!(ctxovrflw::db::memories::count(&conn).unwrap(), 1);
}

#[test]
fn test_list_memories() {
    let (conn, _tmp) = test_db();

    for i in 0..5 {
        ctxovrflw::db::memories::store(
            &conn,
            &format!("Memory {i}"),
            &ctxovrflw::db::memories::MemoryType::Semantic,
            &[],
            None,
            None,
            None,
        )
        .unwrap();
    }

    let all = ctxovrflw::db::memories::list(&conn, 10, 0).unwrap();
    assert_eq!(all.len(), 5);

    // Test limit
    let limited = ctxovrflw::db::memories::list(&conn, 3, 0).unwrap();
    assert_eq!(limited.len(), 3);

    // Test offset
    let offset = ctxovrflw::db::memories::list(&conn, 10, 3).unwrap();
    assert_eq!(offset.len(), 2);
}

#[test]
fn test_memory_types() {
    let (conn, _tmp) = test_db();

    let types = [
        ("fact", ctxovrflw::db::memories::MemoryType::Semantic),
        ("event", ctxovrflw::db::memories::MemoryType::Episodic),
        ("howto", ctxovrflw::db::memories::MemoryType::Procedural),
        ("likes dark mode", ctxovrflw::db::memories::MemoryType::Preference),
    ];

    for (content, mtype) in &types {
        let mem = ctxovrflw::db::memories::store(&conn, content, mtype, &[], None, None, None).unwrap();
        let retrieved = ctxovrflw::db::memories::get(&conn, &mem.id).unwrap().unwrap();
        assert_eq!(
            format!("{}", retrieved.memory_type),
            format!("{}", mtype)
        );
    }
}

#[test]
fn test_memory_with_tags() {
    let (conn, _tmp) = test_db();

    let tags = vec!["project:ctxovrflw".to_string(), "rust".to_string(), "architecture".to_string()];
    let mem = ctxovrflw::db::memories::store(
        &conn,
        "Using SQLite for local storage",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &tags,
        None,
        Some("test"),
        None,
    )
    .unwrap();

    let retrieved = ctxovrflw::db::memories::get(&conn, &mem.id).unwrap().unwrap();
    assert_eq!(retrieved.tags, tags);
}

// ============================================================
// Keyword Search Tests
// ============================================================

#[test]
fn test_keyword_search() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::memories::store(
        &conn, "Rust is a systems programming language",
        &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    ctxovrflw::db::memories::store(
        &conn, "TypeScript is great for web development",
        &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    ctxovrflw::db::memories::store(
        &conn, "Python is popular for data science",
        &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    let results = ctxovrflw::db::search::keyword_search(&conn, "Rust", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].0.content.contains("Rust"));
}

#[test]
fn test_keyword_search_no_results() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::memories::store(
        &conn, "Something about coding",
        &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    let results = ctxovrflw::db::search::keyword_search(&conn, "quantum physics", 10).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_keyword_search_limit() {
    let (conn, _tmp) = test_db();

    for i in 0..10 {
        ctxovrflw::db::memories::store(
            &conn,
            &format!("Rust feature number {i}"),
            &ctxovrflw::db::memories::MemoryType::Semantic,
            &[],
            None,
            None,
            None,
        )
        .unwrap();
    }

    let results = ctxovrflw::db::search::keyword_search(&conn, "Rust", 3).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn test_keyword_search_excludes_deleted() {
    let (conn, _tmp) = test_db();

    let mem = ctxovrflw::db::memories::store(
        &conn, "Secret memory about Rust",
        &ctxovrflw::db::memories::MemoryType::Semantic, &[], None, None, None,
    ).unwrap();

    ctxovrflw::db::memories::delete(&conn, &mem.id).unwrap();

    let results = ctxovrflw::db::search::keyword_search(&conn, "Rust", 10).unwrap();
    assert_eq!(results.len(), 0);
}

// ============================================================
// Config / Tier Tests
// ============================================================

#[test]
fn test_tier_limits() {
    use ctxovrflw::config::Tier;

    // Free
    assert_eq!(Tier::Free.max_memories(), Some(100));
    assert!(Tier::Free.semantic_search_enabled());
    assert!(!Tier::Free.cloud_sync_enabled());
    assert!(!Tier::Free.context_synthesis_enabled());
    assert_eq!(Tier::Free.max_devices(), Some(1));

    // Standard
    assert_eq!(Tier::Standard.max_memories(), None); // Unlimited
    assert!(Tier::Standard.semantic_search_enabled());
    assert!(Tier::Standard.cloud_sync_enabled());
    assert!(!Tier::Standard.context_synthesis_enabled());
    assert_eq!(Tier::Standard.max_devices(), Some(3));

    // Pro
    assert_eq!(Tier::Pro.max_memories(), None); // Unlimited
    assert!(Tier::Pro.semantic_search_enabled());
    assert!(Tier::Pro.cloud_sync_enabled());
    assert!(Tier::Pro.context_synthesis_enabled());
    assert_eq!(Tier::Pro.max_devices(), None); // Unlimited
}

#[test]
fn test_default_config() {
    let cfg = ctxovrflw::config::Config::default();
    assert_eq!(cfg.port, 7437);
    assert_eq!(cfg.tier, ctxovrflw::config::Tier::Free);
}

// ============================================================
// Semantic Search Tests
// ============================================================

#[test]
fn test_semantic_search_basic() {
    let (conn, _tmp) = test_db();

    // Store a memory with embedding
    let emb = test_embedding(1);
    ctxovrflw::db::memories::store(
        &conn, "Max prefers tabs over spaces",
        &ctxovrflw::db::memories::MemoryType::Preference,
        &[], None, Some("test"), Some(&emb),
    ).unwrap();

    // Search with same embedding — should return score close to 1.0
    let results = ctxovrflw::db::search::semantic_search(&conn, &emb, 5).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].1 > 0.99, "Self-similarity should be ~1.0, got {}", results[0].1);
}

#[test]
fn test_semantic_search_ranking() {
    let (conn, _tmp) = test_db();

    // Use the same base embedding with controlled perturbations
    let emb_target = test_embedding(1);

    // "Near" — small perturbation, high cosine similarity (>0.9)
    let mut emb_near = emb_target.clone();
    for i in 0..5 {
        emb_near[i] += 0.001;
    }
    let norm: f32 = emb_near.iter().map(|x| x * x).sum::<f32>().sqrt();
    for v in &mut emb_near { *v /= norm; }

    // "Mid" — moderate perturbation
    let mut emb_mid = emb_target.clone();
    for i in 0..384 {
        emb_mid[i] += if i % 2 == 0 { 0.3 } else { -0.3 };
    }
    let norm: f32 = emb_mid.iter().map(|x| x * x).sum::<f32>().sqrt();
    for v in &mut emb_mid { *v /= norm; }

    let cos_near = cosine_similarity(&emb_target, &emb_near);
    let cos_mid = cosine_similarity(&emb_target, &emb_mid);
    assert!(cos_near > cos_mid, "Setup: near {} > mid {}", cos_near, cos_mid);
    assert!(cos_near > 0.15, "Near should be above threshold: {}", cos_near);

    ctxovrflw::db::memories::store(
        &conn, "Near memory (should rank first)",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[], None, Some("test"), Some(&emb_near),
    ).unwrap();

    ctxovrflw::db::memories::store(
        &conn, "Mid memory (should rank second)",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[], None, Some("test"), Some(&emb_mid),
    ).unwrap();

    let results = ctxovrflw::db::search::semantic_search(&conn, &emb_target, 5).unwrap();
    // At least the near one should pass threshold
    assert!(!results.is_empty(), "Should return at least near memory");

    // First result should be the near one
    assert!(results[0].0.content.contains("Near"),
        "Closest should be 'Near', got: {}", results[0].0.content);

    // If both returned, they should be ordered
    if results.len() > 1 {
        assert!(results[0].1 > results[1].1,
            "Near score {} should > Mid score {}", results[0].1, results[1].1);
    }

    // Scores should be at or above threshold
    for (mem, score) in &results {
        assert!(*score >= ctxovrflw::db::search::MIN_SEMANTIC_SCORE,
            "Score {} below threshold for '{}'", score, mem.content);
    }
}

#[test]
fn test_semantic_search_limit() {
    let (conn, _tmp) = test_db();

    // Store 10 memories all very similar to the query (small perturbations)
    let base = test_embedding(0);
    for i in 0..10 {
        let mut emb = base.clone();
        emb[i % 384] += 0.001 * (i as f32 + 1.0);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        for v in &mut emb { *v /= norm; }
        ctxovrflw::db::memories::store(
            &conn, &format!("Memory number {i}"),
            &ctxovrflw::db::memories::MemoryType::Semantic,
            &[], None, Some("test"), Some(&emb),
        ).unwrap();
    }

    let results = ctxovrflw::db::search::semantic_search(&conn, &base, 3).unwrap();
    assert_eq!(results.len(), 3, "Should respect limit of 3");
}

#[test]
fn test_semantic_search_excludes_deleted() {
    let (conn, _tmp) = test_db();

    let emb = test_embedding(1);
    let mem = ctxovrflw::db::memories::store(
        &conn, "Secret memory",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[], None, Some("test"), Some(&emb),
    ).unwrap();

    ctxovrflw::db::memories::delete(&conn, &mem.id).unwrap();

    let results = ctxovrflw::db::search::semantic_search(&conn, &emb, 5).unwrap();
    assert_eq!(results.len(), 0, "Deleted memory should not appear in search");
}

#[test]
fn test_semantic_score_vs_cosine_similarity() {
    let (conn, _tmp) = test_db();

    let emb_a = test_embedding(1);

    // Create emb_b as a known perturbation of emb_a (guaranteed above threshold)
    let mut emb_b = emb_a.clone();
    for i in 0..384 {
        emb_b[i] += if i % 2 == 0 { 0.2 } else { -0.2 };
    }
    let norm: f32 = emb_b.iter().map(|x| x * x).sum::<f32>().sqrt();
    for v in &mut emb_b { *v /= norm; }

    ctxovrflw::db::memories::store(
        &conn, "Memory A",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[], None, Some("test"), Some(&emb_a),
    ).unwrap();

    ctxovrflw::db::memories::store(
        &conn, "Memory B",
        &ctxovrflw::db::memories::MemoryType::Semantic,
        &[], None, Some("test"), Some(&emb_b),
    ).unwrap();

    // Query with emb_a — Memory A should be exact match (score ~1.0)
    let results = ctxovrflw::db::search::semantic_search(&conn, &emb_a, 5).unwrap();

    let expected_self = cosine_similarity(&emb_a, &emb_a); // ~1.0
    let expected_other = cosine_similarity(&emb_a, &emb_b);

    let score_a = results.iter().find(|(m, _)| m.content == "Memory A").unwrap().1;

    // Self score should closely approximate cosine similarity
    assert!((score_a - expected_self).abs() < 0.01,
        "Self score {score_a} should be close to cosine {expected_self}");

    // If Memory B passes threshold, check it too
    if let Some((_, score_b)) = results.iter().find(|(m, _)| m.content == "Memory B") {
        assert!((score_b - expected_other).abs() < 0.15,
            "Other score {score_b} should approximate cosine {expected_other} (within 0.15)");
        assert!(score_a > *score_b, "Self-similarity should beat cross: {score_a} > {score_b}");
    }
}

#[test]
fn test_semantic_search_empty_db() {
    let (conn, _tmp) = test_db();

    let query = test_embedding(1);
    let results = ctxovrflw::db::search::semantic_search(&conn, &query, 5).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_memory_type_parsing() {
    use std::str::FromStr;
    use ctxovrflw::db::memories::MemoryType;

    assert!(matches!(MemoryType::from_str("semantic").unwrap(), MemoryType::Semantic));
    assert!(matches!(MemoryType::from_str("episodic").unwrap(), MemoryType::Episodic));
    assert!(matches!(MemoryType::from_str("procedural").unwrap(), MemoryType::Procedural));
    assert!(matches!(MemoryType::from_str("preference").unwrap(), MemoryType::Preference));
    assert!(matches!(MemoryType::from_str("SEMANTIC").unwrap(), MemoryType::Semantic));
    assert!(MemoryType::from_str("invalid").is_err());
}

#[test]
fn test_memory_type_display() {
    use ctxovrflw::db::memories::MemoryType;

    assert_eq!(format!("{}", MemoryType::Semantic), "semantic");
    assert_eq!(format!("{}", MemoryType::Episodic), "episodic");
    assert_eq!(format!("{}", MemoryType::Procedural), "procedural");
    assert_eq!(format!("{}", MemoryType::Preference), "preference");
}

// ============================================================
// Knowledge Graph Tests
// ============================================================

#[test]
fn test_create_entity() {
    let (conn, _tmp) = test_db();

    let entity = ctxovrflw::db::graph::upsert_entity(&conn, "auth-service", "service", None).unwrap();
    assert_eq!(entity.name, "auth-service");
    assert_eq!(entity.entity_type, "service");
    assert!(!entity.id.is_empty());
}

#[test]
fn test_upsert_entity_dedup() {
    let (conn, _tmp) = test_db();

    let e1 = ctxovrflw::db::graph::upsert_entity(&conn, "PostgreSQL", "database", None).unwrap();
    let e2 = ctxovrflw::db::graph::upsert_entity(&conn, "PostgreSQL", "database", None).unwrap();
    assert_eq!(e1.id, e2.id, "Same name+type should return same entity");
    assert_eq!(ctxovrflw::db::graph::count_entities(&conn).unwrap(), 1);
}

#[test]
fn test_entity_different_types() {
    let (conn, _tmp) = test_db();

    let e1 = ctxovrflw::db::graph::upsert_entity(&conn, "rust", "language", None).unwrap();
    let e2 = ctxovrflw::db::graph::upsert_entity(&conn, "rust", "game", None).unwrap();
    assert_ne!(e1.id, e2.id, "Same name, different type = different entity");
    assert_eq!(ctxovrflw::db::graph::count_entities(&conn).unwrap(), 2);
}

#[test]
fn test_entity_with_metadata() {
    let (conn, _tmp) = test_db();

    let meta = serde_json::json!({"port": 5432, "version": "15"});
    let entity = ctxovrflw::db::graph::upsert_entity(&conn, "PostgreSQL", "database", Some(&meta)).unwrap();
    assert_eq!(entity.metadata.unwrap()["port"], 5432);
}

#[test]
fn test_entity_validation() {
    let (conn, _tmp) = test_db();

    // Empty name should fail
    assert!(ctxovrflw::db::graph::upsert_entity(&conn, "", "service", None).is_err());
    // Empty type should fail
    assert!(ctxovrflw::db::graph::upsert_entity(&conn, "test", "", None).is_err());
}

#[test]
fn test_find_entity() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::graph::upsert_entity(&conn, "auth-service", "service", None).unwrap();
    ctxovrflw::db::graph::upsert_entity(&conn, "user-service", "service", None).unwrap();

    let found = ctxovrflw::db::graph::find_entity(&conn, "auth-service", Some("service")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "auth-service");

    let not_found = ctxovrflw::db::graph::find_entity(&conn, "nope", None).unwrap();
    assert!(not_found.is_empty());
}

#[test]
fn test_search_entities() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::graph::upsert_entity(&conn, "auth-service", "service", None).unwrap();
    ctxovrflw::db::graph::upsert_entity(&conn, "user-service", "service", None).unwrap();
    ctxovrflw::db::graph::upsert_entity(&conn, "PostgreSQL", "database", None).unwrap();

    let results = ctxovrflw::db::graph::search_entities(&conn, "service", None, 10).unwrap();
    assert_eq!(results.len(), 2);

    let results = ctxovrflw::db::graph::search_entities(&conn, "auth", Some("service"), 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_list_entities() {
    let (conn, _tmp) = test_db();

    for i in 0..5 {
        ctxovrflw::db::graph::upsert_entity(&conn, &format!("entity-{i}"), "test", None).unwrap();
    }

    let all = ctxovrflw::db::graph::list_entities(&conn, None, 10, 0).unwrap();
    assert_eq!(all.len(), 5);

    let limited = ctxovrflw::db::graph::list_entities(&conn, None, 3, 0).unwrap();
    assert_eq!(limited.len(), 3);

    let typed = ctxovrflw::db::graph::list_entities(&conn, Some("test"), 10, 0).unwrap();
    assert_eq!(typed.len(), 5);

    let empty = ctxovrflw::db::graph::list_entities(&conn, Some("nope"), 10, 0).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn test_delete_entity() {
    let (conn, _tmp) = test_db();

    let entity = ctxovrflw::db::graph::upsert_entity(&conn, "to-delete", "test", None).unwrap();
    assert!(ctxovrflw::db::graph::delete_entity(&conn, &entity.id).unwrap());
    assert_eq!(ctxovrflw::db::graph::count_entities(&conn).unwrap(), 0);

    // Double delete
    assert!(!ctxovrflw::db::graph::delete_entity(&conn, &entity.id).unwrap());
}

#[test]
fn test_create_relation() {
    let (conn, _tmp) = test_db();

    let auth = ctxovrflw::db::graph::upsert_entity(&conn, "auth-service", "service", None).unwrap();
    let db_entity = ctxovrflw::db::graph::upsert_entity(&conn, "PostgreSQL", "database", None).unwrap();

    let rel = ctxovrflw::db::graph::upsert_relation(
        &conn, &auth.id, &db_entity.id, "depends_on", 0.95, None, None,
    ).unwrap();

    assert_eq!(rel.source_id, auth.id);
    assert_eq!(rel.target_id, db_entity.id);
    assert_eq!(rel.relation_type, "depends_on");
    assert!((rel.confidence - 0.95).abs() < 0.001);
}

#[test]
fn test_upsert_relation_dedup() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();

    let r1 = ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 0.5, None, None).unwrap();
    let r2 = ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 0.9, None, None).unwrap();

    assert_eq!(r1.id, r2.id, "Same source+target+type should return same relation");
    assert!((r2.confidence - 0.9).abs() < 0.001, "Confidence should be updated");
    assert_eq!(ctxovrflw::db::graph::count_relations(&conn).unwrap(), 1);
}

#[test]
fn test_relation_validation() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();

    // Non-existent target
    assert!(ctxovrflw::db::graph::upsert_relation(&conn, &a.id, "fake-id", "uses", 1.0, None, None).is_err());

    // Invalid confidence
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    assert!(ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.5, None, None).is_err());
    assert!(ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", -0.1, None, None).is_err());

    // Empty relation type
    assert!(ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "", 1.0, None, None).is_err());
}

#[test]
fn test_get_relations() {
    let (conn, _tmp) = test_db();

    let auth = ctxovrflw::db::graph::upsert_entity(&conn, "auth", "service", None).unwrap();
    let pg = ctxovrflw::db::graph::upsert_entity(&conn, "postgres", "database", None).unwrap();
    let redis = ctxovrflw::db::graph::upsert_entity(&conn, "redis", "cache", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &auth.id, &pg.id, "depends_on", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &auth.id, &redis.id, "uses", 0.8, None, None).unwrap();

    // All relations for auth
    let rels = ctxovrflw::db::graph::get_relations(&conn, &auth.id, None, None).unwrap();
    assert_eq!(rels.len(), 2);

    // Filter by type
    let deps = ctxovrflw::db::graph::get_relations(&conn, &auth.id, Some("depends_on"), None).unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].2.name, "postgres");

    // Outgoing only
    let out = ctxovrflw::db::graph::get_relations(&conn, &auth.id, None, Some("outgoing")).unwrap();
    assert_eq!(out.len(), 2);

    // Incoming to postgres
    let inc = ctxovrflw::db::graph::get_relations(&conn, &pg.id, None, Some("incoming")).unwrap();
    assert_eq!(inc.len(), 1);
}

#[test]
fn test_delete_relation() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();

    let rel = ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.0, None, None).unwrap();
    assert!(ctxovrflw::db::graph::delete_relation(&conn, &rel.id).unwrap());
    assert_eq!(ctxovrflw::db::graph::count_relations(&conn).unwrap(), 0);
}

#[test]
fn test_cascade_delete_entity_removes_relations() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    let c = ctxovrflw::db::graph::upsert_entity(&conn, "C", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &b.id, &c.id, "uses", 1.0, None, None).unwrap();
    assert_eq!(ctxovrflw::db::graph::count_relations(&conn).unwrap(), 2);

    // Delete B — should cascade both relations
    ctxovrflw::db::graph::delete_entity(&conn, &b.id).unwrap();
    assert_eq!(ctxovrflw::db::graph::count_relations(&conn).unwrap(), 0);
    assert_eq!(ctxovrflw::db::graph::count_entities(&conn).unwrap(), 2);
}

#[test]
fn test_traverse_basic() {
    let (conn, _tmp) = test_db();

    // A -> B -> C
    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    let c = ctxovrflw::db::graph::upsert_entity(&conn, "C", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &b.id, &c.id, "uses", 1.0, None, None).unwrap();

    // Traverse from A, depth 2
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 2, None, 0.0).unwrap();
    assert_eq!(nodes.len(), 3, "Should reach A, B, C");
    assert_eq!(nodes[0].depth, 0);
    assert_eq!(nodes[0].entity.name, "A");
}

#[test]
fn test_traverse_depth_limit() {
    let (conn, _tmp) = test_db();

    // A -> B -> C -> D
    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    let c = ctxovrflw::db::graph::upsert_entity(&conn, "C", "test", None).unwrap();
    let d = ctxovrflw::db::graph::upsert_entity(&conn, "D", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &b.id, &c.id, "uses", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &c.id, &d.id, "uses", 1.0, None, None).unwrap();

    // Depth 1 — should only reach A and B
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 1, None, 0.0).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[test]
fn test_traverse_confidence_filter() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    let c = ctxovrflw::db::graph::upsert_entity(&conn, "C", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 0.9, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &c.id, "uses", 0.3, None, None).unwrap();

    // min_confidence 0.5 — should skip C
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 2, None, 0.5).unwrap();
    assert_eq!(nodes.len(), 2); // A + B only
}

#[test]
fn test_traverse_relation_type_filter() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();
    let c = ctxovrflw::db::graph::upsert_entity(&conn, "C", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "depends_on", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &c.id, "owns", 1.0, None, None).unwrap();

    // Only follow depends_on
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 2, Some("depends_on"), 0.0).unwrap();
    assert_eq!(nodes.len(), 2); // A + B only
}

#[test]
fn test_traverse_cycle() {
    let (conn, _tmp) = test_db();

    // A -> B -> A (cycle)
    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();

    ctxovrflw::db::graph::upsert_relation(&conn, &a.id, &b.id, "uses", 1.0, None, None).unwrap();
    ctxovrflw::db::graph::upsert_relation(&conn, &b.id, &a.id, "uses", 1.0, None, None).unwrap();

    // Should not infinite loop
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 5, None, 0.0).unwrap();
    assert_eq!(nodes.len(), 2, "Cycle should not cause duplicates");
}

#[test]
fn test_traverse_disconnected() {
    let (conn, _tmp) = test_db();

    let a = ctxovrflw::db::graph::upsert_entity(&conn, "A", "test", None).unwrap();
    let _b = ctxovrflw::db::graph::upsert_entity(&conn, "B", "test", None).unwrap();

    // No relations — only start node
    let nodes = ctxovrflw::db::graph::traverse(&conn, &a.id, 2, None, 0.0).unwrap();
    assert_eq!(nodes.len(), 1);
}

// ============================================================
// Webhook Tests
// ============================================================

#[test]
fn test_create_webhook() {
    let (conn, _tmp) = test_db();

    let hook = ctxovrflw::db::webhooks::create(
        &conn,
        "https://example.com/hook",
        &["memory.created".to_string(), "memory.deleted".to_string()],
        Some("my-secret"),
    ).unwrap();

    assert!(!hook.id.is_empty());
    assert_eq!(hook.url, "https://example.com/hook");
    assert_eq!(hook.events.len(), 2);
    assert!(hook.enabled);
    assert_eq!(hook.secret, Some("my-secret".to_string()));
}

#[test]
fn test_webhook_validation() {
    let (conn, _tmp) = test_db();

    // Empty URL
    assert!(ctxovrflw::db::webhooks::create(&conn, "", &["memory.created".to_string()], None).is_err());

    // Non-HTTP URL
    assert!(ctxovrflw::db::webhooks::create(&conn, "ftp://example.com", &["memory.created".to_string()], None).is_err());

    // Invalid event
    assert!(ctxovrflw::db::webhooks::create(&conn, "https://example.com", &["invalid.event".to_string()], None).is_err());
}

#[test]
fn test_list_webhooks() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::webhooks::create(&conn, "https://a.com/hook", &["memory.created".to_string()], None).unwrap();
    ctxovrflw::db::webhooks::create(&conn, "https://b.com/hook", &["entity.created".to_string()], None).unwrap();

    let hooks = ctxovrflw::db::webhooks::list(&conn).unwrap();
    assert_eq!(hooks.len(), 2);
}

#[test]
fn test_delete_webhook() {
    let (conn, _tmp) = test_db();

    let hook = ctxovrflw::db::webhooks::create(&conn, "https://a.com", &["memory.created".to_string()], None).unwrap();
    assert!(ctxovrflw::db::webhooks::delete(&conn, &hook.id).unwrap());
    assert!(!ctxovrflw::db::webhooks::delete(&conn, &hook.id).unwrap());
    assert!(ctxovrflw::db::webhooks::list(&conn).unwrap().is_empty());
}

#[test]
fn test_webhook_enable_disable() {
    let (conn, _tmp) = test_db();

    let hook = ctxovrflw::db::webhooks::create(&conn, "https://a.com", &["memory.created".to_string()], None).unwrap();
    assert!(hook.enabled);

    ctxovrflw::db::webhooks::update_enabled(&conn, &hook.id, false).unwrap();
    let updated = ctxovrflw::db::webhooks::get(&conn, &hook.id).unwrap().unwrap();
    assert!(!updated.enabled);

    ctxovrflw::db::webhooks::update_enabled(&conn, &hook.id, true).unwrap();
    let updated = ctxovrflw::db::webhooks::get(&conn, &hook.id).unwrap().unwrap();
    assert!(updated.enabled);
}

#[test]
fn test_get_webhooks_for_event() {
    let (conn, _tmp) = test_db();

    ctxovrflw::db::webhooks::create(&conn, "https://a.com", &["memory.created".to_string(), "memory.deleted".to_string()], None).unwrap();
    ctxovrflw::db::webhooks::create(&conn, "https://b.com", &["entity.created".to_string()], None).unwrap();
    let disabled = ctxovrflw::db::webhooks::create(&conn, "https://c.com", &["memory.created".to_string()], None).unwrap();
    ctxovrflw::db::webhooks::update_enabled(&conn, &disabled.id, false).unwrap();

    let memory_hooks = ctxovrflw::db::webhooks::get_for_event(&conn, "memory.created").unwrap();
    assert_eq!(memory_hooks.len(), 1, "Disabled hook should be excluded");
    assert_eq!(memory_hooks[0].url, "https://a.com");

    let entity_hooks = ctxovrflw::db::webhooks::get_for_event(&conn, "entity.created").unwrap();
    assert_eq!(entity_hooks.len(), 1);

    let none_hooks = ctxovrflw::db::webhooks::get_for_event(&conn, "relation.created").unwrap();
    assert!(none_hooks.is_empty());
}

// ============================================================
// Tier Gate Tests
// ============================================================

#[test]
fn test_knowledge_graph_tier_gate() {
    use ctxovrflw::config::Tier;

    assert!(!Tier::Free.knowledge_graph_enabled());
    assert!(!Tier::Standard.knowledge_graph_enabled());
    assert!(Tier::Pro.knowledge_graph_enabled());
}

#[test]
fn test_consolidation_tier_gate() {
    use ctxovrflw::config::Tier;

    assert!(!Tier::Free.consolidation_enabled());
    assert!(!Tier::Standard.consolidation_enabled());
    assert!(Tier::Pro.consolidation_enabled());
}
