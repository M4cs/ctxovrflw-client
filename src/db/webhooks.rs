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

/// Validate a webhook URL to prevent SSRF attacks.
/// Rejects private/reserved IP ranges.
pub fn validate_webhook_url(url: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("Webhook URL cannot be empty");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("Webhook URL must start with http:// or https://");
    }

    // Parse the URL and resolve the hostname
    let parsed: url::Url = url.parse().map_err(|_| anyhow::anyhow!("Invalid URL"))?;
    let host = parsed.host_str().ok_or_else(|| anyhow::anyhow!("URL has no host"))?;

    // Check for obvious private hostnames
    if host == "localhost" || host == "0.0.0.0" || host.ends_with(".local") {
        anyhow::bail!("Webhook URL must not point to localhost or private hosts");
    }

    // Try to parse as IP directly
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_ip(&ip) {
            anyhow::bail!("Webhook URL must not point to a private/reserved IP address");
        }
    } else {
        // Resolve hostname and check all IPs
        use std::net::ToSocketAddrs;
        let port = parsed.port().unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
        if let Ok(addrs) = (host, port).to_socket_addrs() {
            for addr in addrs {
                if is_private_ip(&addr.ip()) {
                    anyhow::bail!("Webhook URL resolves to a private/reserved IP address");
                }
            }
        }
    }

    Ok(())
}

fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()                          // 127.x.x.x
                || v4.is_private()                    // 10.x, 172.16-31.x, 192.168.x
                || v4.is_link_local()                 // 169.254.x.x
                || v4.is_unspecified()                // 0.0.0.0
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64  // 100.64-127.x (CGNAT)
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()                          // ::1
                || v6.is_unspecified()                // ::
                || v6.segments()[0] == 0xfd00         // fd00::/8 (ULA)
                || v6.segments()[0] == 0xfe80         // fe80::/10 (link-local)
                || v6.segments()[0] == 0xfc00         // fc00::/7
        }
    }
}

/// Hash a webhook secret using SHA-256 for storage.
pub fn hash_secret(secret: &str) -> String {
    use ring::digest;
    let hash = digest::digest(&digest::SHA256, secret.as_bytes());
    hash.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
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
