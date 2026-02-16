use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Valid webhook event types.
pub const VALID_EVENTS: &[&str] = &[
    "memory.created",
    "memory.updated",
    "memory.deleted",
    "entity.created",
    "entity.updated",
    "entity.deleted",
    "relation.created",
    "relation.updated",
    "relation.deleted",
];

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS webhooks (
            id          TEXT PRIMARY KEY,
            url         TEXT NOT NULL,
            secret      TEXT,
            events      TEXT NOT NULL DEFAULT '[]',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )?;
    Ok(())
}

pub fn create(conn: &Connection, url: &str, events: &[String], secret: Option<&str>) -> Result<Webhook> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("Webhook URL cannot be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("Webhook URL must start with http:// or https://");
    }

    // Validate events
    for event in events {
        if !VALID_EVENTS.contains(&event.as_str()) {
            anyhow::bail!("Invalid event type: '{}'. Valid: {:?}", event, VALID_EVENTS);
        }
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let events_json = serde_json::to_string(events)?;

    conn.execute(
        "INSERT INTO webhooks (id, url, secret, events, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![id, url, secret, events_json, now],
    )?;

    Ok(Webhook {
        id,
        url: url.to_string(),
        secret: secret.map(String::from),
        events: events.to_vec(),
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<Webhook>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, secret, events, enabled, created_at, updated_at FROM webhooks ORDER BY created_at",
    )?;
    let hooks = stmt
        .query_map([], |row| {
            Ok(Webhook {
                id: row.get(0)?,
                url: row.get(1)?,
                secret: row.get(2)?,
                events: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                enabled: row.get::<_, i32>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(hooks)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Webhook>> {
    let result = conn
        .query_row(
            "SELECT id, url, secret, events, enabled, created_at, updated_at FROM webhooks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Webhook {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    secret: row.get(2)?,
                    events: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    enabled: row.get::<_, i32>(4)? != 0,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .ok();
    Ok(result)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    let changed = conn.execute("DELETE FROM webhooks WHERE id = ?1", params![id])?;
    Ok(changed > 0)
}

pub fn update_enabled(conn: &Connection, id: &str, enabled: bool) -> Result<bool> {
    let now = Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE webhooks SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
        params![enabled as i32, now, id],
    )?;
    Ok(changed > 0)
}

/// Get all enabled webhooks that are subscribed to a given event.
pub fn get_for_event(conn: &Connection, event: &str) -> Result<Vec<Webhook>> {
    let hooks = list(conn)?;
    Ok(hooks
        .into_iter()
        .filter(|h| h.enabled && h.events.iter().any(|e| e == event))
        .collect())
}
