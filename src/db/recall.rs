//! Recall tracking and importance scoring for adaptive memory
//! Phase 2: Tracks every recall to build importance scores
//! Phase 3: Decay calculation and rehydration

use anyhow::Result;
use rusqlite::{params, Connection};

/// Log a recall event for importance tracking
pub fn log_recall(
    conn: &Connection,
    memory_id: &str,
    agent_id: Option<&str>,
    query: Option<&str>,
    score: Option<f64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO recall_logs (memory_id, agent_id, query, score)
         VALUES (?1, ?2, ?3, ?4)",
        params![memory_id, agent_id, query, score],
    )?;
    Ok(())
}

/// Update importance scores for all memories
/// Call this periodically (e.g., hourly) via background task
pub fn update_importance_scores(conn: &Connection) -> Result<usize> {
    // Get recall counts per memory
    let mut stmt = conn.prepare(
        "SELECT memory_id, COUNT(*) as cnt, MAX(recalled_at) as last_recalled
         FROM recall_logs
         WHERE recalled_at > datetime('now', '-90 days')
         GROUP BY memory_id"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    
    let mut updated = 0;
    for row in rows {
        let (memory_id, count, last_recalled) = row?;
        
        // Calculate decay factor (decays over time since last recall)
        let decay = if let Some(ref last) = last_recalled {
            calculate_decay(last)
        } else {
            0.1 // Default low decay for old memories
        };
        
        // Importance = recall_count * decay_factor
        let importance = (count as f64) * decay;
        
        conn.execute(
            "INSERT INTO memory_scores (memory_id, recall_count, last_recalled, decay_factor, importance, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(memory_id) DO UPDATE SET
                recall_count = excluded.recall_count,
                last_recalled = excluded.last_recalled,
                decay_factor = excluded.decay_factor,
                importance = excluded.importance,
                updated_at = excluded.updated_at",
            params![memory_id, count, last_recalled, decay, importance],
        )?;
        updated += 1;
    }
    
    Ok(updated)
}

/// Calculate decay factor based on days since last recall
/// Formula: 1.0 / (1.0 + days * 0.1) - decays ~10% per week
fn calculate_decay(last_recalled: &str) -> f64 {
    let days = days_since(last_recalled).unwrap_or(90) as f64;
    1.0 / (1.0 + days * 0.1)
}

/// Get days since a datetime string
fn days_since(datetime: &str) -> Option<i64> {
    use chrono::NaiveDateTime;
    
    let last = NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S").ok()?;
    let now = chrono::Utc::now().naive_utc();
    let diff = now.signed_duration_since(last);
    Some(diff.num_days())
}

/// Get importance-boosted memories that should be rehydrated
/// Returns memories with low decay (important) or high decay but semantically relevant
pub fn get_rehydration_candidates(
    conn: &Connection,
    _concept_keywords: &[String],
    limit: usize,
) -> Result<Vec<(String, f64)>> {
    // Find decayed memories that might still be relevant
    let mut stmt = conn.prepare(
        "SELECT ms.memory_id, ms.decay_factor, ms.importance
         FROM memory_scores ms
         JOIN memories m ON ms.memory_id = m.id
         WHERE m.deleted = 0
         AND (m.expires_at IS NULL OR m.expires_at > datetime('now'))
         ORDER BY ms.importance DESC
         LIMIT ?1"
    )?;
    
    let rows = stmt.query_map(params![limit * 3], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    
    let mut candidates = Vec::new();
    for row in rows {
        let (id, decay, importance) = row?;
        // Include if: high importance OR recently decayed but still relevant
        if importance > 1.0 || (decay > 0.2 && decay < 0.5) {
            candidates.push((id, importance));
        }
    }
    
    candidates.truncate(limit);
    Ok(candidates)
}

/// Get agent-specific importance boost
/// Memories recalled frequently by this agent get boosted
pub fn get_agent_importance(
    conn: &Connection,
    memory_id: &str,
    agent_id: &str,
) -> Result<f64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM recall_logs 
         WHERE memory_id = ?1 AND agent_id = ?2
         AND recalled_at > datetime('now', '-30 days')",
        params![memory_id, agent_id],
        |row| row.get(0),
    )?;
    
    // Boost based on agent-specific recall frequency
    Ok(1.0 + (count as f64 * 0.1).min(2.0)) // Max 3x boost
}

/// Clean old recall logs (keep 90 days)
pub fn cleanup_old_logs(conn: &Connection) -> Result<usize> {
    let deleted = conn.execute(
        "DELETE FROM recall_logs WHERE recalled_at < datetime('now', '-90 days')",
        [],
    )?;
    Ok(deleted)
}

/// Get top important memories for an agent
/// Used for personality synthesis (Phase 4)
pub fn get_important_memories(
    conn: &Connection,
    agent_id: Option<&str>,
    limit: usize,
) -> Result<Vec<String>> {
    let query = if agent_id.is_some() {
        // Agent-specific: prioritize memories this agent recalls
        "SELECT ms.memory_id
         FROM memory_scores ms
         JOIN recall_logs rl ON ms.memory_id = rl.memory_id
         WHERE rl.agent_id = ?1
         GROUP BY ms.memory_id
         ORDER BY ms.importance DESC, COUNT(rl.id) DESC
         LIMIT ?2"
    } else {
        // Global: most important across all agents
        "SELECT memory_id FROM memory_scores
         ORDER BY importance DESC
         LIMIT ?1"
    };
    
    let mut stmt = conn.prepare(query)?;

    if let Some(agent) = agent_id {
        stmt.query_map(params![agent, limit], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    } else {
        stmt.query_map(params![limit], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
