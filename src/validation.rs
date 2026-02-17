//! Shared validation functions used by both HTTP routes and MCP tools.

use chrono::Utc;

/// Maximum memory content size (100 KB).
pub const MAX_CONTENT_SIZE: usize = 100 * 1024;
pub const MAX_TAG_LENGTH: usize = 200;
pub const MAX_TAGS: usize = 50;
pub const MAX_SUBJECT_LENGTH: usize = 500;

/// Parse a TTL string like "1h", "24h", "7d", "30m" into an expiry timestamp.
pub fn parse_ttl(ttl: &str) -> Result<String, String> {
    let ttl = ttl.trim().to_lowercase();
    let (num_str, multiplier) = if ttl.ends_with('d') {
        (&ttl[..ttl.len() - 1], 86400i64)
    } else if ttl.ends_with('h') {
        (&ttl[..ttl.len() - 1], 3600i64)
    } else if ttl.ends_with('m') {
        (&ttl[..ttl.len() - 1], 60i64)
    } else if ttl.ends_with('s') {
        (&ttl[..ttl.len() - 1], 1i64)
    } else {
        return Err(format!("Invalid TTL format: '{ttl}'. Use '1h', '24h', '7d', '30m'"));
    };
    let num: i64 = num_str.parse().map_err(|_| format!("Invalid TTL number: '{num_str}'"))?;
    if num <= 0 {
        return Err("TTL must be positive".into());
    }
    let expires = Utc::now() + chrono::Duration::seconds(num * multiplier);
    Ok(expires.to_rfc3339())
}

/// Resolve expiry from ttl or expires_at. Returns Ok(Some(timestamp)) or Ok(None).
pub fn resolve_expiry(ttl: Option<&str>, expires_at: Option<&str>) -> Result<Option<String>, String> {
    if let Some(t) = ttl {
        return Ok(Some(parse_ttl(t)?));
    }
    if let Some(e) = expires_at {
        chrono::DateTime::parse_from_rfc3339(e)
            .map_err(|_| "Invalid expires_at: must be ISO 8601 / RFC 3339".to_string())?;
        return Ok(Some(e.to_string()));
    }
    Ok(None)
}

/// Deduplicate and validate tags. Returns cleaned tags or an error message.
pub fn validate_tags(tags: &[String]) -> Result<Vec<String>, String> {
    if tags.len() > MAX_TAGS {
        return Err(format!("Too many tags ({}). Maximum is {}.", tags.len(), MAX_TAGS));
    }
    for tag in tags {
        if tag.len() > MAX_TAG_LENGTH {
            return Err(format!(
                "Tag too long ({} chars). Maximum is {} chars.",
                tag.len(),
                MAX_TAG_LENGTH
            ));
        }
    }
    let mut deduped: Vec<String> = tags.to_vec();
    deduped.sort();
    deduped.dedup();
    Ok(deduped)
}

/// Validate subject length.
pub fn validate_subject(subject: Option<&str>) -> Result<(), String> {
    if let Some(s) = subject {
        if s.len() > MAX_SUBJECT_LENGTH {
            return Err(format!(
                "Subject too long ({} chars). Maximum is {} chars.",
                s.len(),
                MAX_SUBJECT_LENGTH
            ));
        }
    }
    Ok(())
}

/// Sanitize error messages to avoid leaking internal paths or implementation details.
pub fn sanitize_error(e: &impl std::fmt::Display) -> String {
    let msg = e.to_string();
    if msg.contains('/') || msg.contains("\\\\") {
        return "Internal error".to_string();
    }
    msg
}

/// Shared hex encoding.
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Shared hex decoding.
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
