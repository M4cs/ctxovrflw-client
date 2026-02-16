use anyhow::Result;
use rusqlite::{params, Connection};

use super::memories::Memory;

/// Minimum cosine similarity score to include in semantic search results.
/// Below this threshold, results are considered noise and filtered out.
pub const MIN_SEMANTIC_SCORE: f64 = 0.15;

/// Indicates which search method produced the results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMethod {
    Keyword,
    Semantic,
}

impl std::fmt::Display for SearchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMethod::Keyword => write!(f, "keyword"),
            SearchMethod::Semantic => write!(f, "semantic"),
        }
    }
}

/// Sanitize a query string for FTS5.
/// FTS5 treats dots, colons, and special chars as syntax.
/// Wrap each token in double quotes to treat as literal.
fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keyword search via FTS5 (free tier)
pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<(Memory, f64)>> {
    let sanitized = sanitize_fts_query(query);
    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, m.type, m.tags, m.subject, m.source, m.expires_at, m.created_at, m.updated_at,
                rank
         FROM memories_fts fts
         JOIN memories m ON m.rowid = fts.rowid
         WHERE memories_fts MATCH ?1 AND m.deleted = 0
         AND (m.expires_at IS NULL OR m.expires_at > datetime('now'))
         ORDER BY rank
         LIMIT ?2",
    )?;

    let results = stmt
        .query_map(params![sanitized, limit], |row| {
            let rank: f64 = row.get(9)?;
            Ok((
                Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    memory_type: row
                        .get::<_, String>(2)?
                        .parse()
                        .unwrap_or_default(),
                    tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    subject: row.get(4)?,
                    source: row.get(5)?,
                    expires_at: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                },
                -rank, // FTS5 rank is negative (lower = better), flip for score
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(results)
}

/// Semantic (vector) search via sqlite-vec (paid tiers)
pub fn semantic_search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(Memory, f64)>> {
    let embedding_bytes: Vec<u8> = query_embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

    // sqlite-vec uses a KNN query via the virtual table's match syntax
    let mut stmt = conn.prepare(
        "SELECT v.id, v.distance, m.content, m.type, m.tags, m.subject, m.source, m.expires_at, m.created_at, m.updated_at
         FROM memory_vectors v
         JOIN memories m ON m.id = v.id
         WHERE v.embedding MATCH ?1 AND k = ?2
         AND m.deleted = 0
         AND (m.expires_at IS NULL OR m.expires_at > datetime('now'))",
    )?;

    // Fetch more candidates than requested to allow for score filtering.
    // sqlite-vec's k parameter limits the KNN search, so we need headroom.
    let k = (limit * 4).max(20).min(200);

    let results: Vec<(Memory, f64)> = stmt
        .query_map(params![embedding_bytes, k], |row| {
            let distance: f64 = row.get(1)?;
            let score = 1.0 - (distance * distance / 2.0);
            Ok((
                Memory {
                    id: row.get(0)?,
                    content: row.get(2)?,
                    memory_type: row
                        .get::<_, String>(3)?
                        .parse()
                        .unwrap_or_default(),
                    tags: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    subject: row.get(5)?,
                    source: row.get(6)?,
                    expires_at: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                },
                score,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Filter out low-relevance results (noise)
    let filtered: Vec<_> = results.into_iter()
        .filter(|(_, score)| *score >= MIN_SEMANTIC_SCORE)
        .take(limit)
        .collect();
    Ok(filtered)
}

/// List all memories about a specific subject
pub fn by_subject(conn: &Connection, subject: &str, limit: usize) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, expires_at, created_at, updated_at
         FROM memories WHERE subject = ?1 AND deleted = 0
         AND (expires_at IS NULL OR expires_at > datetime('now'))
         ORDER BY updated_at DESC LIMIT ?2",
    )?;

    let results = stmt
        .query_map(params![subject, limit], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: row.get::<_, String>(2)?.parse().unwrap_or_default(),
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                subject: row.get(4)?,
                source: row.get(5)?,
                expires_at: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(results)
}

/// List all distinct subjects
pub fn list_subjects(conn: &Connection) -> Result<Vec<(String, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT subject, COUNT(*) as cnt FROM memories
         WHERE subject IS NOT NULL AND deleted = 0
         GROUP BY subject ORDER BY cnt DESC",
    )?;

    let results = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(results)
}
