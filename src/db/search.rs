#[cfg(feature = "pro")]
use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, Connection};

use super::memories::Memory;

/// Minimum cosine similarity score to include in semantic search results.
/// Below this threshold, results are considered noise and filtered out.
pub const MIN_SEMANTIC_SCORE: f64 = 0.15;

/// RRF constant (k=60 is standard). Higher k reduces the impact of rank position.
#[cfg(feature = "pro")]
const RRF_K: f64 = 60.0;

/// Indicates which search method produced the results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMethod {
    Keyword,
    Semantic,
    Hybrid,
}

impl std::fmt::Display for SearchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMethod::Keyword => write!(f, "keyword"),
            SearchMethod::Semantic => write!(f, "semantic"),
            SearchMethod::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Common English stopwords to exclude from FTS queries
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "to", "of", "in", "for",
    "on", "with", "at", "by", "from", "as", "into", "about", "between",
    "through", "during", "before", "after", "above", "below", "and", "but",
    "or", "not", "no", "if", "then", "than", "that", "this", "these",
    "those", "it", "its", "i", "me", "my", "we", "our", "you", "your",
    "he", "she", "they", "them", "his", "her", "what", "which", "who",
    "how", "when", "where", "why", "all", "each", "every", "both", "few",
    "more", "most", "some", "any", "so", "up", "out",
];

/// Sanitize a query string for FTS5.
/// Removes stopwords, wraps tokens in quotes, uses OR for broader matching.
fn sanitize_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_lowercase().replace('"', "").replace('?', "").replace('.', "").replace(',', ""))
        .filter(|t| t.len() > 1 && !STOPWORDS.contains(&t.as_str()))
        .map(|t| format!("\"{}\"", t))
        .collect();

    if tokens.is_empty() {
        // Fallback: use original query as-is
        return format!("\"{}\"", query.replace('"', ""));
    }

    // Use OR to match any token (broader recall)
    tokens.join(" OR ")
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

#[cfg(feature = "pro")]
/// Hybrid search: combines semantic (vector) and keyword (FTS5) results using
/// Reciprocal Rank Fusion (RRF). This dramatically improves recall quality by
/// catching results that one method misses but the other finds.
///
/// RRF score = sum(1 / (k + rank_i)) for each result list the item appears in.
/// Items appearing in both lists get boosted; items in only one still appear.
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(Memory, f64)>> {
    // Fetch more candidates from each source for better fusion
    let fetch_limit = (limit * 3).max(15);

    // Get semantic results
    let semantic_results = semantic_search(conn, query_embedding, fetch_limit).unwrap_or_default();

    // Get keyword results — also try expanded query for better recall
    let keyword_results = keyword_search(conn, query, fetch_limit).unwrap_or_default();

    // Subject-based boost: if query mentions a known subject, include those
    let subject_results = extract_subject_matches(conn, query, fetch_limit);

    // If one source is empty, return the other directly
    if semantic_results.is_empty() && keyword_results.is_empty() {
        return Ok(vec![]);
    }
    if semantic_results.is_empty() {
        return Ok(keyword_results.into_iter().take(limit).collect());
    }
    if keyword_results.is_empty() {
        return Ok(semantic_results.into_iter().take(limit).collect());
    }

    // Build RRF scores
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut memories: HashMap<String, Memory> = HashMap::new();

    // Semantic results contribute RRF score based on their rank
    for (rank, (mem, _score)) in semantic_results.into_iter().enumerate() {
        let rrf = 1.0 / (RRF_K + rank as f64 + 1.0);
        *scores.entry(mem.id.clone()).or_default() += rrf;
        memories.entry(mem.id.clone()).or_insert(mem);
    }

    // Keyword results contribute RRF score based on their rank
    for (rank, (mem, _score)) in keyword_results.into_iter().enumerate() {
        let rrf = 1.0 / (RRF_K + rank as f64 + 1.0);
        *scores.entry(mem.id.clone()).or_default() += rrf;
        memories.entry(mem.id.clone()).or_insert(mem);
    }

    // Subject/tag-match results get full RRF weight — exact metadata matches
    // are strong relevance signals
    for (rank, mem) in subject_results.into_iter().enumerate() {
        let rrf = 1.0 / (RRF_K + rank as f64 + 1.0);
        *scores.entry(mem.id.clone()).or_default() += rrf;
        memories.entry(mem.id.clone()).or_insert(mem);
    }

    // Sort by combined RRF score (highest first)
    let mut fused: Vec<(Memory, f64)> = scores
        .into_iter()
        .filter_map(|(id, score)| {
            memories.remove(&id).map(|mem| (mem, score))
        })
        .collect();

    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused.truncate(limit);

    Ok(fused)
}

#[cfg(feature = "pro")]
/// Extract potential subject/tag matches from a query.
/// Looks for known subjects and tags that appear as words in the query.
fn extract_subject_matches(conn: &Connection, query: &str, limit: usize) -> Vec<Memory> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();

    let mut results = Vec::new();

    // Check if any known subjects appear in the query.
    // Skip subjects that are too generic (appear in too many memories)
    // — they'd flood results without adding signal.
    if let Ok(subjects) = list_subjects(conn) {
        for (subject, count) in subjects {
            // Skip subjects with too many memories (generic catch-alls)
            if count > 15 { continue; }

            let subj_lower = subject.to_lowercase();
            let subj_words: Vec<&str> = subj_lower.split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .collect();

            if subj_words.iter().any(|sw| query_words.contains(sw)) {
                if let Ok(mems) = by_subject(conn, &subject, limit) {
                    results.extend(mems);
                }
            }
        }
    }

    // Also search by tags that match query words
    if let Ok(tag_mems) = search_by_tags(conn, &query_words, limit) {
        results.extend(tag_mems);
    }

    results
}

#[cfg(feature = "pro")]
/// Search memories that have matching tags
fn search_by_tags(conn: &Connection, query_words: &[&str], limit: usize) -> Result<Vec<Memory>> {
    // SQLite JSON: tags are stored as JSON arrays like '["tag1","tag2"]'
    // Search for memories where any tag matches any query word
    let mut all_results = Vec::new();

    for word in query_words {
        if STOPWORDS.contains(word) || word.len() < 3 { continue; }

        let pattern = format!("%\"{}\"%", word);
        let mut stmt = conn.prepare(
            "SELECT id, content, type, tags, subject, source, expires_at, created_at, updated_at
             FROM memories WHERE tags LIKE ?1 AND deleted = 0
             AND (expires_at IS NULL OR expires_at > datetime('now'))
             ORDER BY updated_at DESC LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![pattern, limit], |row| {
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

        all_results.extend(results);
    }

    Ok(all_results)
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
