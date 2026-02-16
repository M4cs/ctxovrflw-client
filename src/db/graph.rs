use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Data types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_memory_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalNode {
    pub entity: Entity,
    pub depth: usize,
    pub path: Vec<TraversalEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalEdge {
    pub relation_id: String,
    pub relation_type: String,
    pub from_entity: String,
    pub to_entity: String,
    pub confidence: f64,
}

// ── Schema migration ────────────────────────────────────────

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS entities (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'generic',
            metadata    TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_name_type ON entities(name, type);
        CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);
        CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);

        CREATE TABLE IF NOT EXISTS relations (
            id                TEXT PRIMARY KEY,
            source_id         TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            target_id         TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            relation_type     TEXT NOT NULL,
            confidence        REAL NOT NULL DEFAULT 1.0,
            source_memory_id  TEXT REFERENCES memories(id) ON DELETE SET NULL,
            metadata          TEXT,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
        CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
        CREATE INDEX IF NOT EXISTS idx_relations_type ON relations(relation_type);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_relations_unique
            ON relations(source_id, target_id, relation_type);
        ",
    )?;
    Ok(())
}

// ── Entity CRUD ─────────────────────────────────────────────

/// Create or update an entity. If an entity with the same name+type exists,
/// update its metadata and return the existing record.
pub fn upsert_entity(
    conn: &Connection,
    name: &str,
    entity_type: &str,
    metadata: Option<&serde_json::Value>,
) -> Result<Entity> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("Entity name cannot be empty");
    }
    let entity_type = entity_type.trim().to_lowercase();
    if entity_type.is_empty() {
        anyhow::bail!("Entity type cannot be empty");
    }

    let now = Utc::now().to_rfc3339();
    let meta_json = metadata.map(|m| serde_json::to_string(m).unwrap_or_default());

    // Try to find existing
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM entities WHERE name = ?1 AND type = ?2",
            params![name, entity_type],
            |r| r.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE entities SET metadata = COALESCE(?1, metadata), updated_at = ?2 WHERE id = ?3",
            params![meta_json, now, id],
        )?;
        return get_entity(conn, &id)?
            .ok_or_else(|| anyhow::anyhow!("Entity vanished after update"));
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO entities (id, name, type, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, entity_type, meta_json, now],
    )?;

    Ok(Entity {
        id,
        name: name.to_string(),
        entity_type,
        metadata: metadata.cloned(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_entity(conn: &Connection, id: &str) -> Result<Option<Entity>> {
    let result = conn
        .query_row(
            "SELECT id, name, type, metadata, created_at, updated_at FROM entities WHERE id = ?1",
            params![id],
            |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    metadata: row
                        .get::<_, Option<String>>(3)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .ok();
    Ok(result)
}

pub fn find_entity(conn: &Connection, name: &str, entity_type: Option<&str>) -> Result<Vec<Entity>> {
    let query = if let Some(etype) = entity_type {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities WHERE name = ?1 AND type = ?2",
        )?;
        stmt.query_map(params![name, etype], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities WHERE name = ?1",
        )?;
        stmt.query_map(params![name], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    Ok(query)
}

pub fn search_entities(conn: &Connection, query: &str, entity_type: Option<&str>, limit: usize) -> Result<Vec<Entity>> {
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));

    let entities = if let Some(etype) = entity_type {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities WHERE name LIKE ?1 ESCAPE '\\' AND type = ?2
             ORDER BY name LIMIT ?3",
        )?;
        stmt.query_map(params![pattern, etype, limit], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities WHERE name LIKE ?1 ESCAPE '\\'
             ORDER BY name LIMIT ?2",
        )?;
        stmt.query_map(params![pattern, limit], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    Ok(entities)
}

pub fn list_entities(conn: &Connection, entity_type: Option<&str>, limit: usize, offset: usize) -> Result<Vec<Entity>> {
    let entities = if let Some(etype) = entity_type {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities WHERE type = ?1 ORDER BY name LIMIT ?2 OFFSET ?3",
        )?;
        stmt.query_map(params![etype, limit, offset], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, name, type, metadata, created_at, updated_at
             FROM entities ORDER BY name LIMIT ?1 OFFSET ?2",
        )?;
        stmt.query_map(params![limit, offset], row_to_entity)?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    Ok(entities)
}

pub fn delete_entity(conn: &Connection, id: &str) -> Result<bool> {
    // CASCADE will remove relations
    let changed = conn.execute("DELETE FROM entities WHERE id = ?1", params![id])?;
    Ok(changed > 0)
}

pub fn count_entities(conn: &Connection) -> Result<usize> {
    let count: usize = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
    Ok(count)
}

fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
    Ok(Entity {
        id: row.get(0)?,
        name: row.get(1)?,
        entity_type: row.get(2)?,
        metadata: row
            .get::<_, Option<String>>(3)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

// ── Relation CRUD ───────────────────────────────────────────

/// Create or update a relation. If a relation with the same source+target+type
/// already exists, update confidence and metadata.
pub fn upsert_relation(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
    confidence: f64,
    source_memory_id: Option<&str>,
    metadata: Option<&serde_json::Value>,
) -> Result<Relation> {
    let relation_type = relation_type.trim().to_lowercase();
    if relation_type.is_empty() {
        anyhow::bail!("Relation type cannot be empty");
    }
    if confidence < 0.0 || confidence > 1.0 {
        anyhow::bail!("Confidence must be between 0.0 and 1.0");
    }

    // Verify both entities exist
    let source_exists: bool = conn
        .query_row("SELECT COUNT(*) FROM entities WHERE id = ?1", params![source_id], |r| r.get::<_, i32>(0))
        .map(|c| c > 0)?;
    let target_exists: bool = conn
        .query_row("SELECT COUNT(*) FROM entities WHERE id = ?1", params![target_id], |r| r.get::<_, i32>(0))
        .map(|c| c > 0)?;

    if !source_exists {
        anyhow::bail!("Source entity {source_id} does not exist");
    }
    if !target_exists {
        anyhow::bail!("Target entity {target_id} does not exist");
    }

    let now = Utc::now().to_rfc3339();
    let meta_json = metadata.map(|m| serde_json::to_string(m).unwrap_or_default());

    // Check for existing relation
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM relations WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
            params![source_id, target_id, relation_type],
            |r| r.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE relations SET confidence = ?1, source_memory_id = COALESCE(?2, source_memory_id),
             metadata = COALESCE(?3, metadata), updated_at = ?4 WHERE id = ?5",
            params![confidence, source_memory_id, meta_json, now, id],
        )?;
        return get_relation(conn, &id)?
            .ok_or_else(|| anyhow::anyhow!("Relation vanished after update"));
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO relations (id, source_id, target_id, relation_type, confidence, source_memory_id, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, source_id, target_id, relation_type, confidence, source_memory_id, meta_json, now],
    )?;

    Ok(Relation {
        id,
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        relation_type,
        confidence,
        source_memory_id: source_memory_id.map(String::from),
        metadata: metadata.cloned(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_relation(conn: &Connection, id: &str) -> Result<Option<Relation>> {
    let result = conn
        .query_row(
            "SELECT id, source_id, target_id, relation_type, confidence, source_memory_id, metadata, created_at, updated_at
             FROM relations WHERE id = ?1",
            params![id],
            row_to_relation,
        )
        .ok();
    Ok(result)
}

/// Get all relations involving an entity (as source or target).
pub fn get_relations(
    conn: &Connection,
    entity_id: &str,
    relation_type: Option<&str>,
    direction: Option<&str>, // "outgoing", "incoming", or None for both
) -> Result<Vec<(Relation, Entity, Entity)>> {
    let base_query = "SELECT r.id, r.source_id, r.target_id, r.relation_type, r.confidence,
            r.source_memory_id, r.metadata, r.created_at, r.updated_at,
            s.id, s.name, s.type, s.metadata, s.created_at, s.updated_at,
            t.id, t.name, t.type, t.metadata, t.created_at, t.updated_at
         FROM relations r
         JOIN entities s ON r.source_id = s.id
         JOIN entities t ON r.target_id = t.id";

    let (where_clause, type_filter) = match (direction, relation_type) {
        (Some("outgoing"), Some(rt)) => (
            format!(" WHERE r.source_id = ?1 AND r.relation_type = ?2"),
            Some(rt),
        ),
        (Some("incoming"), Some(rt)) => (
            format!(" WHERE r.target_id = ?1 AND r.relation_type = ?2"),
            Some(rt),
        ),
        (Some("outgoing"), None) => (
            format!(" WHERE r.source_id = ?1"),
            None,
        ),
        (Some("incoming"), None) => (
            format!(" WHERE r.target_id = ?1"),
            None,
        ),
        (_, Some(rt)) => (
            format!(" WHERE (r.source_id = ?1 OR r.target_id = ?1) AND r.relation_type = ?2"),
            Some(rt),
        ),
        (_, None) => (
            format!(" WHERE r.source_id = ?1 OR r.target_id = ?1"),
            None,
        ),
    };

    let sql = format!("{base_query}{where_clause} ORDER BY r.confidence DESC");

    let mut stmt = conn.prepare(&sql)?;

    let results = if let Some(rt) = type_filter {
        stmt.query_map(params![entity_id, rt], |row| {
            Ok((
                row_to_relation(row)?,
                row_to_entity_at(row, 9)?,
                row_to_entity_at(row, 15)?,
            ))
        })?.collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![entity_id], |row| {
            Ok((
                row_to_relation(row)?,
                row_to_entity_at(row, 9)?,
                row_to_entity_at(row, 15)?,
            ))
        })?.collect::<std::result::Result<Vec<_>, _>>()?
    };

    Ok(results)
}

pub fn delete_relation(conn: &Connection, id: &str) -> Result<bool> {
    let changed = conn.execute("DELETE FROM relations WHERE id = ?1", params![id])?;
    Ok(changed > 0)
}

pub fn count_relations(conn: &Connection) -> Result<usize> {
    let count: usize = conn.query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))?;
    Ok(count)
}

// ── Graph traversal ─────────────────────────────────────────

/// BFS traversal from an entity up to `max_depth` hops.
/// Returns all reachable entities with their shortest path.
pub fn traverse(
    conn: &Connection,
    start_entity_id: &str,
    max_depth: usize,
    relation_type: Option<&str>,
    min_confidence: f64,
) -> Result<Vec<TraversalNode>> {
    let max_depth = max_depth.min(5); // Hard cap to prevent runaway queries

    let start = get_entity(conn, start_entity_id)?
        .ok_or_else(|| anyhow::anyhow!("Start entity not found"))?;

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(start_entity_id.to_string());

    let mut result = vec![TraversalNode {
        entity: start,
        depth: 0,
        path: vec![],
    }];

    let mut frontier: Vec<(String, usize, Vec<TraversalEdge>)> = vec![(
        start_entity_id.to_string(),
        0,
        vec![],
    )];

    while let Some((current_id, depth, path)) = frontier.pop() {
        if depth >= max_depth {
            continue;
        }

        // Get all relations from current entity
        let relations = get_entity_edges(conn, &current_id, relation_type, min_confidence)?;

        for (rel, neighbor_id, neighbor) in relations {
            if visited.contains(&neighbor_id) {
                continue;
            }
            visited.insert(neighbor_id.clone());

            let mut new_path = path.clone();
            new_path.push(TraversalEdge {
                relation_id: rel.id.clone(),
                relation_type: rel.relation_type.clone(),
                from_entity: current_id.clone(),
                to_entity: neighbor_id.clone(),
                confidence: rel.confidence,
            });

            result.push(TraversalNode {
                entity: neighbor,
                depth: depth + 1,
                path: new_path.clone(),
            });

            frontier.push((neighbor_id, depth + 1, new_path));
        }
    }

    Ok(result)
}

/// Get all edges from an entity (both directions), returning (relation, neighbor_id, neighbor_entity).
fn get_entity_edges(
    conn: &Connection,
    entity_id: &str,
    relation_type: Option<&str>,
    min_confidence: f64,
) -> Result<Vec<(Relation, String, Entity)>> {
    // Outgoing: source_id = entity_id → neighbor is target
    // Incoming: target_id = entity_id → neighbor is source
    let sql = if let Some(_) = relation_type {
        "SELECT r.id, r.source_id, r.target_id, r.relation_type, r.confidence,
                r.source_memory_id, r.metadata, r.created_at, r.updated_at,
                e.id, e.name, e.type, e.metadata, e.created_at, e.updated_at
         FROM relations r
         JOIN entities e ON (
            CASE WHEN r.source_id = ?1 THEN r.target_id ELSE r.source_id END = e.id
         )
         WHERE (r.source_id = ?1 OR r.target_id = ?1)
           AND r.relation_type = ?2
           AND r.confidence >= ?3
         ORDER BY r.confidence DESC"
    } else {
        "SELECT r.id, r.source_id, r.target_id, r.relation_type, r.confidence,
                r.source_memory_id, r.metadata, r.created_at, r.updated_at,
                e.id, e.name, e.type, e.metadata, e.created_at, e.updated_at
         FROM relations r
         JOIN entities e ON (
            CASE WHEN r.source_id = ?1 THEN r.target_id ELSE r.source_id END = e.id
         )
         WHERE (r.source_id = ?1 OR r.target_id = ?1)
           AND r.confidence >= ?2
         ORDER BY r.confidence DESC"
    };

    let mut stmt = conn.prepare(sql)?;
    let results = if let Some(rt) = relation_type {
        stmt.query_map(params![entity_id, rt, min_confidence], |row| {
            let rel = row_to_relation(row)?;
            let neighbor = row_to_entity_at(row, 9)?;
            let neighbor_id = if rel.source_id == entity_id {
                rel.target_id.clone()
            } else {
                rel.source_id.clone()
            };
            Ok((rel, neighbor_id, neighbor))
        })?.collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![entity_id, min_confidence], |row| {
            let rel = row_to_relation(row)?;
            let neighbor = row_to_entity_at(row, 9)?;
            let neighbor_id = if rel.source_id == entity_id {
                rel.target_id.clone()
            } else {
                rel.source_id.clone()
            };
            Ok((rel, neighbor_id, neighbor))
        })?.collect::<std::result::Result<Vec<_>, _>>()?
    };

    Ok(results)
}

// ── Helpers ─────────────────────────────────────────────────

fn row_to_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
    Ok(Relation {
        id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        relation_type: row.get(3)?,
        confidence: row.get(4)?,
        source_memory_id: row.get(5)?,
        metadata: row
            .get::<_, Option<String>>(6)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn row_to_entity_at(row: &rusqlite::Row, offset: usize) -> rusqlite::Result<Entity> {
    Ok(Entity {
        id: row.get(offset)?,
        name: row.get(offset + 1)?,
        entity_type: row.get(offset + 2)?,
        metadata: row
            .get::<_, Option<String>>(offset + 3)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get(offset + 4)?,
        updated_at: row.get(offset + 5)?,
    })
}
