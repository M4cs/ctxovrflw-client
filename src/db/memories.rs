use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    #[default]
    Semantic,
    Episodic,
    Procedural,
    Preference,
    /// Agent-specific personality traits and preferences
    AgentPersonality,
    /// Agent-specific response patterns and rules
    AgentRules,
    /// Agent-private channel - only visible to specific agent
    ChannelPrivate,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Semantic => write!(f, "semantic"),
            MemoryType::Episodic => write!(f, "episodic"),
            MemoryType::Procedural => write!(f, "procedural"),
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::AgentPersonality => write!(f, "agent_personality"),
            MemoryType::AgentRules => write!(f, "agent_rules"),
            MemoryType::ChannelPrivate => write!(f, "channel_private"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "semantic" => Ok(MemoryType::Semantic),
            "episodic" => Ok(MemoryType::Episodic),
            "procedural" => Ok(MemoryType::Procedural),
            "preference" => Ok(MemoryType::Preference),
            "agent_personality" | "agentpersonality" => Ok(MemoryType::AgentPersonality),
            "agent_rules" | "agentrules" => Ok(MemoryType::AgentRules),
            "channel_private" | "channelprivate" | "private" => Ok(MemoryType::ChannelPrivate),
            _ => anyhow::bail!("Unknown memory type: {s}"),
        }
    }
}

pub fn store(
    conn: &Connection,
    content: &str,
    memory_type: &MemoryType,
    tags: &[String],
    subject: Option<&str>,
    source: Option<&str>,
    embedding: Option<&[f32]>,
    agent_id: Option<&str>,
) -> Result<Memory> {
    store_with_expiry(conn, content, memory_type, tags, subject, source, embedding, None, agent_id)
}

pub fn store_with_expiry(
    conn: &Connection,
    content: &str,
    memory_type: &MemoryType,
    tags: &[String],
    subject: Option<&str>,
    source: Option<&str>,
    embedding: Option<&[f32]>,
    expires_at: Option<&str>,
    agent_id: Option<&str>,
) -> Result<Memory> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(tags)?;

    conn.execute(
        "INSERT INTO memories (id, content, type, tags, subject, source, embedding, expires_at, agent_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            content,
            memory_type.to_string(),
            tags_json,
            subject,
            source,
            embedding.map(|e| bytemuck_cast(e)),
            expires_at,
            agent_id,
            now,
            now,
        ],
    )?;

    // If we have an embedding, also store in vec table
    if let Some(emb) = embedding {
        let _ = conn.execute(
            "INSERT INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
            params![id, bytemuck_cast(emb)],
        );
    }

    Ok(Memory {
        id,
        content: content.to_string(),
        memory_type: memory_type.clone(),
        tags: tags.to_vec(),
        subject: subject.map(|s| s.to_string()),
        source: source.map(|s| s.to_string()),
        agent_id: agent_id.map(|s| s.to_string()),
        expires_at: expires_at.map(|s| s.to_string()),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at
         FROM memories WHERE id = ?1 AND deleted = 0",
    )?;

    let result = stmt
        .query_row(params![id], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or_default(),
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                subject: row.get(4)?,
                source: row.get(5)?,
                agent_id: row.get(6)?,
                expires_at: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .ok();

    Ok(result)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    let changed = conn.execute(
        "UPDATE memories SET deleted = 1, updated_at = ?1 WHERE id = ?2 AND deleted = 0",
        params![Utc::now().to_rfc3339(), id],
    )?;
    Ok(changed > 0)
}

pub fn count(conn: &Connection) -> Result<usize> {
    let count: usize =
        conn.query_row("SELECT COUNT(*) FROM memories WHERE deleted = 0", [], |r| {
            r.get(0)
        })?;
    Ok(count)
}

pub fn list(conn: &Connection, limit: usize, offset: usize) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at
         FROM memories WHERE deleted = 0
         AND (expires_at IS NULL OR expires_at > datetime('now'))
         ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
    )?;

    let memories = stmt
        .query_map(params![limit, offset], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or_default(),
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                subject: row.get(4)?,
                source: row.get(5)?,
                agent_id: row.get(6)?,
                expires_at: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(memories)
}

/// Update a memory's mutable fields. Only non-None fields are updated.
pub fn update(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
    tags: Option<&[String]>,
    subject: Option<Option<&str>>,  // Some(None) = clear, Some(Some(x)) = set, None = no change
    expires_at: Option<Option<&str>>,  // Some(None) = remove expiry, Some(Some(x)) = set, None = no change
    embedding: Option<&[f32]>,
) -> Result<Option<Memory>> {
    let now = Utc::now().to_rfc3339();

    // Build dynamic UPDATE
    let mut sets = vec!["updated_at = ?1".to_string()];
    let mut param_idx = 2u32;
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now.clone())];

    if let Some(c) = content {
        sets.push(format!("content = ?{param_idx}"));
        params_vec.push(Box::new(c.to_string()));
        param_idx += 1;
    }
    if let Some(t) = tags {
        sets.push(format!("tags = ?{param_idx}"));
        params_vec.push(Box::new(serde_json::to_string(t)?));
        param_idx += 1;
    }
    if let Some(s) = subject {
        sets.push(format!("subject = ?{param_idx}"));
        params_vec.push(Box::new(s.map(|v| v.to_string())));
        param_idx += 1;
    }
    if let Some(e) = expires_at {
        sets.push(format!("expires_at = ?{param_idx}"));
        params_vec.push(Box::new(e.map(|v| v.to_string())));
        param_idx += 1;
    }
    if let Some(emb) = embedding {
        sets.push(format!("embedding = ?{param_idx}"));
        params_vec.push(Box::new(bytemuck_cast(emb)));
        param_idx += 1;
    }

    // ID is the last param
    let id_param_idx = param_idx;
    params_vec.push(Box::new(id.to_string()));

    let sql = format!(
        "UPDATE memories SET {} WHERE id = ?{} AND deleted = 0",
        sets.join(", "),
        id_param_idx
    );

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let changed = conn.execute(&sql, params_refs.as_slice())?;

    if changed == 0 {
        return Ok(None);
    }

    // Update vec table if embedding provided
    if let Some(emb) = embedding {
        let _ = conn.execute(
            "DELETE FROM memory_vectors WHERE id = ?1",
            params![id],
        );
        let _ = conn.execute(
            "INSERT INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
            params![id, bytemuck_cast(emb)],
        );
    }

    get(conn, id)
}

/// Delete memories that have expired. Returns count of cleaned up memories.
pub fn cleanup_expired(conn: &Connection) -> Result<usize> {
    let count = conn.execute(
        "UPDATE memories SET deleted = 1, updated_at = ?1
         WHERE deleted = 0 AND expires_at IS NOT NULL AND expires_at <= datetime('now')",
        params![Utc::now().to_rfc3339()],
    )?;
    Ok(count)
}

/// Cast f32 slice to bytes for SQLite BLOB storage
fn bytemuck_cast(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Public version for use by sync module
pub fn bytemuck_cast_pub(floats: &[f32]) -> Vec<u8> {
    bytemuck_cast(floats)
}
