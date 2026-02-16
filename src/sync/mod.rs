use anyhow::Result;
use serde::Deserialize;

use crate::config::Config;
use crate::crypto;
use crate::db;

#[derive(Debug, Deserialize)]
struct PushResponse {
    synced: usize,
    #[allow(dead_code)]
    rejected: usize,
    over_limit: bool,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    memories: Vec<RemoteMemory>,
    #[allow(dead_code)]
    sync_timestamp: String,
}

#[derive(Debug, Deserialize)]
struct RemoteMemory {
    id: String,
    content: String,
    memory_type: String,
    tags: Vec<String>,
    #[serde(default)]
    subject: Option<String>,
    source: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    deleted: bool,
    created_at: String,
    updated_at: String,
}

/// Get the encryption key from config, or bail.
/// Sync REQUIRES encryption — no plaintext cloud storage allowed.
fn get_encryption_key(cfg: &Config) -> Result<[u8; 32]> {
    if !cfg.is_encrypted() {
        anyhow::bail!(
            "Encryption not configured. Run `ctxovrflw login` to set up your sync PIN.\n\
             Cloud sync requires end-to-end encryption — plaintext sync is not allowed."
        );
    }
    match cfg.get_cached_key() {
        Some(key) => Ok(key),
        None => anyhow::bail!(
            "Sync PIN expired. Run `ctxovrflw login` to re-enter your PIN."
        ),
    }
}

/// Run a full sync cycle: push local changes, then pull remote changes
pub async fn run(cfg: &Config) -> Result<()> {
    if !cfg.is_logged_in() {
        println!("Not logged in. Run `ctxovrflw login` first.");
        return Ok(());
    }

    let api_key = cfg.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no API key"))?;
    let device_id = cfg.device_id.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no device ID"))?;
    let enc_key = get_encryption_key(cfg)?;

    let pushed = push(cfg, api_key, device_id, &enc_key).await?;
    let pulled = pull(cfg, api_key, device_id, &enc_key).await?;
    let purged = purge_tombstones()?;

    println!("✓ Sync complete — pushed {pushed}, pulled {pulled}");
    if purged > 0 {
        println!("  🗑️  Purged {purged} old tombstones");
    }
    println!("  🔐 End-to-end encrypted");
    Ok(())
}

/// Run sync silently (for auto-sync in daemon). Returns (pushed, pulled).
pub async fn run_silent(cfg: &Config) -> Result<(usize, usize)> {
    if !cfg.is_logged_in() {
        return Ok((0, 0));
    }

    let api_key = cfg.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no API key"))?;
    let device_id = cfg.device_id.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no device ID"))?;
    let enc_key = match get_encryption_key(cfg) {
        Ok(key) => key,
        Err(e) => {
            tracing::warn!("Sync skipped: {e}");
            return Ok((0, 0));
        }
    };

    let pushed = push(cfg, api_key, device_id, &enc_key).await?;
    let pulled = pull(cfg, api_key, device_id, &enc_key).await?;
    let _ = purge_tombstones(); // Best-effort cleanup

    Ok((pushed, pulled))
}

/// Purge tombstones (soft-deleted memories) that have been synced and are older than 7 days.
/// This permanently removes them from the local DB to reclaim space.
/// Cloud-side cleanup happens separately via the cloud API's purge endpoint.
fn purge_tombstones() -> Result<usize> {
    let conn = db::open()?;

    // Delete vectors first (FK-like cleanup)
    conn.execute(
        "DELETE FROM memory_vectors WHERE id IN (
            SELECT id FROM memories
            WHERE deleted = 1
              AND synced_at IS NOT NULL
              AND updated_at <= datetime('now', '-7 days')
        )",
        [],
    )?;

    // Then permanently remove the tombstones
    let purged = conn.execute(
        "DELETE FROM memories
         WHERE deleted = 1
           AND synced_at IS NOT NULL
           AND updated_at <= datetime('now', '-7 days')",
        [],
    )?;

    if purged > 0 {
        tracing::info!("Purged {purged} tombstones older than 7 days");
    }

    Ok(purged)
}

/// Encrypt a memory's content + tags for cloud storage.
/// Returns (encrypted_content, encrypted_tags_json, content_hash).
fn encrypt_memory(
    key: &[u8; 32],
    content: &str,
    tags: &[String],
) -> Result<(String, String, String)> {
    let enc_content = crypto::encrypt_string(key, content)?;
    let tags_json = serde_json::to_string(tags)?;
    let enc_tags = crypto::encrypt_string(key, &tags_json)?;
    let hash = crypto::content_hash(content);
    Ok((enc_content, enc_tags, hash))
}

/// Max memories to fetch at once (pre-batching)
const FETCH_BATCH_SIZE: usize = 200;
/// Target max payload size per push request (leave headroom below 1MB cloud limit)
const MAX_PAYLOAD_BYTES: usize = 800 * 1024; // 800KB

/// Estimate the JSON size of a serialized memory value
fn estimate_size(mem: &serde_json::Value) -> usize {
    serde_json::to_string(mem).map(|s| s.len()).unwrap_or(1024)
}

/// Push unsynced local memories to cloud (incremental, size-aware batching)
async fn push(
    cfg: &Config,
    api_key: &str,
    device_id: &str,
    enc_key: &[u8; 32],
) -> Result<usize> {
    let conn = db::open()?;
    let client = reqwest::Client::new();
    let mut total_synced: usize = 0;

    loop {
        let all_unsynced = get_unsynced_memories(&conn, enc_key, FETCH_BATCH_SIZE)?;
        if all_unsynced.is_empty() {
            break;
        }

        let fetched_count = all_unsynced.len();

        // Split into size-aware batches
        let mut batch: Vec<serde_json::Value> = Vec::new();
        let mut batch_size: usize = 100; // base JSON overhead
        let mut remaining: std::collections::VecDeque<serde_json::Value> = all_unsynced.into();

        while let Some(mem) = remaining.pop_front() {
            let mem_size = estimate_size(&mem);

            // Skip memories that are individually too large (>500KB) — log and mark as synced to avoid infinite loop
            if mem_size > 500 * 1024 {
                let id = mem.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                tracing::warn!("Skipping oversized memory {} ({} bytes) — too large for cloud sync", id, mem_size);
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let _ = conn.execute(
                    "UPDATE memories SET synced_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, id],
                );
                continue;
            }

            // If adding this memory would exceed the limit, push what we have first
            if !batch.is_empty() && batch_size + mem_size > MAX_PAYLOAD_BYTES {
                remaining.push_front(mem);
                break;
            }

            batch_size += mem_size;
            batch.push(mem);
        }

        if batch.is_empty() {
            // All remaining were oversized — check if we had any
            if fetched_count < FETCH_BATCH_SIZE {
                break;
            }
            continue;
        }

        let batch_ids: Vec<String> = batch.iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();

        let resp = client
            .post(format!("{}/v1/sync/push", cfg.cloud_url))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&serde_json::json!({
                "device_id": device_id,
                "memories": batch,
                "encrypted": true,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Push failed ({}): {}", status, body);
        }

        let result: PushResponse = resp.json().await?;

        // Mark successfully pushed memories with synced_at timestamp
        if result.synced > 0 {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            for id in &batch_ids {
                let _ = conn.execute(
                    "UPDATE memories SET synced_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, id],
                );
            }
        }

        total_synced += result.synced;

        if result.over_limit {
            tracing::warn!("Memory limit reached on cloud. Upgrade your plan.");
            break;
        }

        // If we fetched fewer than the limit and processed everything, we're done
        if fetched_count < FETCH_BATCH_SIZE && remaining.is_empty() {
            break;
        }
    }

    Ok(total_synced)
}

/// Pull remote changes and merge into local DB
async fn pull(
    cfg: &Config,
    api_key: &str,
    device_id: &str,
    enc_key: &[u8; 32],
) -> Result<usize> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/sync/pull", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "device_id": device_id,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Pull failed ({}): {}", status, body);
    }

    let result: PullResponse = resp.json().await?;
    let count = result.memories.len();

    if count > 0 {
        let conn = db::open()?;
        merge_remote_memories(&conn, &result.memories, enc_key)?;
    }

    Ok(count)
}

/// Push a single memory to the cloud immediately.
pub async fn push_one(cfg: &Config, memory_id: &str) -> Result<bool> {
    if !cfg.is_logged_in() {
        return Ok(false);
    }

    let api_key = cfg.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no API key"))?;
    let device_id = cfg.device_id.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in — no device ID"))?;
    let enc_key = get_encryption_key(cfg)?;
    let conn = db::open()?;

    let mem: Option<serde_json::Value> = conn
        .query_row(
            "SELECT id, content, type, tags, subject, source, deleted, created_at, updated_at, expires_at
             FROM memories WHERE id = ?1",
            rusqlite::params![memory_id],
            |row| {
                let tags_str: String = row.get(3)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content: String = row.get(1)?;
                let deleted: bool = row.get::<_, i32>(6)? != 0;

                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "content": content,
                    "memory_type": row.get::<_, String>(2)?,
                    "tags": tags,
                    "subject": row.get::<_, Option<String>>(4)?,
                    "source": row.get::<_, Option<String>>(5)?,
                    "expires_at": row.get::<_, Option<String>>(9)?,
                    "deleted": deleted,
                    "created_at": row.get::<_, String>(7)?,
                    "updated_at": row.get::<_, String>(8)?,
                }))
            },
        )
        .ok();

    let mut mem = match mem {
        Some(m) => m,
        None => return Ok(false),
    };

    // Encrypt content + tags before pushing
    {
        let content = mem["content"].as_str().unwrap_or("");
        let tags: Vec<String> = mem["tags"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let (enc_content, enc_tags, hash) = encrypt_memory(&enc_key, content, &tags)?;
        mem["content"] = serde_json::Value::String(enc_content);
        mem["tags"] = serde_json::json!([enc_tags]);
        mem["content_hash"] = serde_json::Value::String(hash);
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/sync/push", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "memories": [mem],
            "encrypted": true,
        }))
        .send()
        .await?;

    if resp.status().is_success() {
        // Mark as synced
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = conn.execute(
            "UPDATE memories SET synced_at = ?1 WHERE id = ?2",
            rusqlite::params![now, memory_id],
        );
        return Ok(true);
    }

    Ok(false)
}

/// Get memories that need to be pushed (never synced, or updated after last sync).
/// Returns at most `limit` memories, encrypting content if key is provided.
fn get_unsynced_memories(
    conn: &rusqlite::Connection,
    enc_key: &[u8; 32],
    limit: usize,
) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, deleted, created_at, updated_at, expires_at
         FROM memories
         WHERE synced_at IS NULL OR updated_at > synced_at
         ORDER BY updated_at ASC
         LIMIT ?1"
    )?;

    let memories = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            let tags_str: String = row.get(3)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content: String = row.get(1)?;
            let deleted: bool = row.get::<_, i32>(6)? != 0;

            Ok((
                row.get::<_, String>(0)?,  // id
                content,
                row.get::<_, String>(2)?,  // type
                tags,
                row.get::<_, Option<String>>(4)?, // subject
                row.get::<_, Option<String>>(5)?, // source
                deleted,
                row.get::<_, String>(7)?,  // created_at
                row.get::<_, String>(8)?,  // updated_at
                row.get::<_, Option<String>>(9)?, // expires_at
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut result = Vec::with_capacity(memories.len());
    for (id, content, mtype, tags, subject, source, deleted, created_at, updated_at, expires_at) in memories {
        let (enc_content, enc_tags, hash) = encrypt_memory(enc_key, &content, &tags)
            .map_err(|e| anyhow::anyhow!("Encryption failed for {id}: {e}"))?;
        let mem = serde_json::json!({
            "id": id,
            "content": enc_content,
            "memory_type": mtype,
            "tags": [enc_tags],
            "subject": subject,
            "source": source,
            "expires_at": expires_at,
            "deleted": deleted,
            "created_at": created_at,
            "updated_at": updated_at,
            "content_hash": hash,
        });
        result.push(mem);
    }

    Ok(result)
}


/// Merge remote memories into local DB, decrypting if key is provided.
fn merge_remote_memories(
    conn: &rusqlite::Connection,
    memories: &[RemoteMemory],
    enc_key: &[u8; 32],
) -> Result<()> {
    // Use the global singleton embedder (loaded once at startup, shared everywhere)
    let embedder = crate::embed::get_or_init().ok();

    for mem in memories {
        // Decrypt content (all cloud data must be encrypted)
        let decrypted_content = crypto::decrypt_string(enc_key, &mem.content)
            .unwrap_or_else(|_| mem.content.clone()); // Fallback for legacy plaintext data

        let decrypted_tags = if let Some(enc_tags) = mem.tags.first() {
            // Tags are stored as a single encrypted JSON string in the array
            match crypto::decrypt_string(enc_key, enc_tags) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => mem.tags.clone(), // Fallback for legacy plaintext
            }
        } else {
            vec![]
        };

        let (content, tags) = (decrypted_content, decrypted_tags);

        // Check if the memory exists locally and whether it's deleted
        let local_state: Option<(bool,)> = conn
            .query_row(
                "SELECT deleted FROM memories WHERE id = ?1",
                rusqlite::params![mem.id],
                |r| Ok((r.get::<_, i32>(0)? != 0,)),
            )
            .ok();

        let exists = local_state.is_some();
        let locally_deleted = local_state.map(|(d,)| d).unwrap_or(false);

        if mem.deleted {
            if exists {
                conn.execute(
                    "UPDATE memories SET deleted = 1, updated_at = ?1, synced_at = ?1 WHERE id = ?2",
                    rusqlite::params![mem.updated_at, mem.id],
                )?;
                // Remove embedding for deleted memory
                let _ = conn.execute(
                    "DELETE FROM memory_vectors WHERE id = ?1",
                    rusqlite::params![mem.id],
                );
            }
            continue;
        }

        // If locally deleted, don't resurrect — local deletion wins
        if locally_deleted {
            continue;
        }

        let tags_json = serde_json::to_string(&tags)?;

        if exists {
            let rows = conn.execute(
                "UPDATE memories SET content = ?1, type = ?2, tags = ?3, subject = ?4, source = ?5,
                 expires_at = ?6, updated_at = ?7, synced_at = ?7, deleted = 0
                 WHERE id = ?8 AND updated_at < ?7",
                rusqlite::params![content, mem.memory_type, tags_json, mem.subject, mem.source, mem.expires_at, mem.updated_at, mem.id],
            )?;
            // Re-embed if content was actually updated
            if rows > 0 {
                if let Some(ref emb) = embedder { let mut emb = emb.blocking_lock();
                    if let Ok(embedding) = emb.embed(&content) {
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
                            rusqlite::params![mem.id, crate::db::memories::bytemuck_cast_pub(&embedding)],
                        );
                    }
                }
            }
        } else {
            conn.execute(
                "INSERT INTO memories (id, content, type, tags, subject, source, expires_at, deleted, created_at, updated_at, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?9)",
                rusqlite::params![mem.id, content, mem.memory_type, tags_json, mem.subject, mem.source, mem.expires_at, mem.created_at, mem.updated_at],
            )?;

            // Generate embedding for the new memory
            if let Some(ref emb) = embedder { let mut emb = emb.blocking_lock();
                if let Ok(embedding) = emb.embed(&content) {
                    let _ = conn.execute(
                        "INSERT OR REPLACE INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
                        rusqlite::params![mem.id, crate::db::memories::bytemuck_cast_pub(&embedding)],
                    );
                }
            }
        }
    }

    // Mark all pulled memory IDs as synced (catch echoed-back pushes that
    // didn't match the UPDATE condition but are still in sync with cloud)
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    for mem in memories {
        let _ = conn.execute(
            "UPDATE memories SET synced_at = ?1 WHERE id = ?2 AND (synced_at IS NULL OR synced_at < ?1)",
            rusqlite::params![now, mem.id],
        );
    }

    Ok(())
}
