use anyhow::Result;

use crate::db;
use crate::db::graph;

/// Build knowledge graph from existing memories by extracting entities
/// from subject fields and namespaced tags.
pub fn build() -> Result<()> {
    let conn = db::open()?;
    let memories = load_all_memories(&conn)?;

    let total = memories.len();
    let mut entities_created = 0usize;
    let mut relations_created = 0usize;
    let mut skipped = 0usize;

    println!("Scanning {} memories for graph entities...\n", total);

    for (i, mem) in memories.iter().enumerate() {
        let mut extracted = false;

        // Extract from subject field
        if let Some(subject) = &mem.subject {
            let (entity_type, entity_name) = if let Some((t, n)) = subject.split_once(':') {
                (t.trim().to_lowercase(), n.trim().to_string())
            } else {
                ("generic".to_string(), subject.trim().to_string())
            };

            if !entity_name.is_empty() {
                match graph::upsert_entity(&conn, &entity_name, &entity_type, None) {
                    Ok(entity) => {
                        entities_created += 1;
                        extracted = true;

                        // Create a "memory" entity for this memory and link them
                        let short_content = if mem.content.len() > 50 {
                            format!("{}...", &mem.content[..50])
                        } else {
                            mem.content.clone()
                        };
                        let meta = serde_json::json!({ "preview": short_content });
                        if let Ok(mem_entity) = graph::upsert_entity(
                            &conn,
                            &mem.id,
                            "memory",
                            Some(&meta),
                        ) {
                            if graph::upsert_relation(
                                &conn,
                                &entity.id,
                                &mem_entity.id,
                                "mentioned_in",
                                1.0,
                                Some(&mem.id),
                                None,
                            ).is_ok() {
                                relations_created += 1;
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        // Extract from namespaced tags
        for tag in &mem.tags {
            if let Some((ns, value)) = tag.split_once(':') {
                let ns = ns.trim().to_lowercase();
                let value = value.trim().to_string();
                if !value.is_empty() && !ns.is_empty() {
                    if graph::upsert_entity(&conn, &value, &ns, None).is_ok() {
                        entities_created += 1;
                        extracted = true;
                    }
                }
            }
        }

        if !extracted {
            skipped += 1;
        }

        // Progress indicator every 50 memories
        if (i + 1) % 50 == 0 || i + 1 == total {
            print!("\r  Processed {}/{}", i + 1, total);
        }
    }

    println!("\n");

    // Deduplicate counts (upsert means some are updates, not creates)
    let entity_count = graph::count_entities(&conn).unwrap_or(0);
    let relation_count = graph::count_relations(&conn).unwrap_or(0);

    println!("✓ Graph built from {} memories", total);
    println!("  Entities: {} (total in graph: {})", entities_created, entity_count);
    println!("  Relations: {} (total in graph: {})", relations_created, relation_count);
    println!("  Skipped: {} (no subject or tags to extract)", skipped);

    Ok(())
}

/// Show knowledge graph statistics.
pub fn stats() -> Result<()> {
    let conn = db::open()?;

    let entity_count = graph::count_entities(&conn)?;
    let relation_count = graph::count_relations(&conn)?;

    println!("Knowledge Graph Statistics:");
    println!("  Entities:  {}", entity_count);
    println!("  Relations: {}", relation_count);

    if entity_count > 0 {
        // Show top entity types
        let entities = graph::list_entities(&conn, None, 1000, 0)?;
        let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in &entities {
            *type_counts.entry(e.entity_type.clone()).or_default() += 1;
        }
        let mut sorted: Vec<_> = type_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        println!("\n  Entity types:");
        for (t, count) in sorted.iter().take(10) {
            println!("    {}: {}", t, count);
        }
    }

    Ok(())
}

struct MemoryRecord {
    id: String,
    content: String,
    subject: Option<String>,
    tags: Vec<String>,
}

fn load_all_memories(conn: &rusqlite::Connection) -> Result<Vec<MemoryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, subject, tags FROM memories WHERE deleted = 0
         AND (expires_at IS NULL OR expires_at > datetime('now'))"
    )?;

    let results = stmt
        .query_map([], |row| {
            Ok(MemoryRecord {
                id: row.get(0)?,
                content: row.get(1)?,
                subject: row.get(2)?,
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(results)
}
