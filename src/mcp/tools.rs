use anyhow::Result;
use serde_json::{json, Value};

use crate::config::{Config, Tier};
use crate::db;

pub fn list_tools(cfg: &Config) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "remember",
            "description": "Store important context to shared memory. This memory persists across sessions and is accessible by ALL connected AI tools (Cursor, Claude Code, Cline, etc.).\n\nUSE THIS WHEN:\n- The user shares a preference (\"I prefer tabs\", \"I use Fly.io\")\n- A decision is made (\"We're using Rust for the backend\")\n- Important project context comes up (\"The API is at api.example.com\")\n- The user explicitly asks you to remember something\n\nBEST PRACTICES:\n- Store atomic facts, not paragraphs\n- Use tags for organization\n- Set subject to identify WHO or WHAT this memory is about\n- Choose the right type: preference for likes/config, semantic for facts, episodic for events, procedural for how-to steps",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to remember. Keep it concise and atomic — one fact per memory. Example: \"Max prefers tabs over spaces\" not \"Max told me about his coding preferences and he said he likes tabs...\""
                    },
                    "type": {
                        "type": "string",
                        "enum": ["semantic", "episodic", "procedural", "preference"],
                        "description": "Memory type. semantic=facts/knowledge, episodic=events/experiences, procedural=how-to/steps, preference=likes/dislikes/config",
                        "default": "semantic"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for organization. Use namespace:value format. Examples: ['project:myapp', 'lang:rust', 'infra:deploy']"
                    },
                    "subject": {
                        "type": "string",
                        "description": "The entity this memory is about. Use format like 'user', 'project:myapp', 'person:sarah', 'agent:claude', 'team:backend'. Enables scoped recall — 'tell me everything about sarah'."
                    },
                    "ttl": {
                        "type": "string",
                        "description": "Time-to-live duration. Memory auto-expires after this. Examples: '1h', '24h', '7d', '30m'. Useful for temporary context like active debugging sessions, sprint goals, or short-lived tasks."
                    },
                    "expires_at": {
                        "type": "string",
                        "description": "Explicit expiry timestamp (ISO 8601 / RFC 3339). Mutually exclusive with ttl. Example: '2025-03-01T00:00:00Z'"
                    }
                },
                "required": ["content"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Search shared memory for relevant context using semantic similarity. Results come from ALL connected AI tools — something stored by Cursor can be recalled by Claude Code.\n\nUSE THIS WHEN:\n- Before answering questions about the user's preferences, setup, or past decisions\n- The user asks \"do you remember...\" or \"what did I say about...\"\n- You need project context that might have been discussed in another tool\n- Starting a new session and need to catch up on context\n\nTIPS:\n- Use natural language queries (\"coding preferences\" not just \"tabs\")\n- Semantic search finds conceptually related memories, not just keyword matches\n- Use subject filter to scope results (\"everything about project X\")\n- Use max_tokens to control context window usage",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query. Be descriptive — \"deployment configuration\" works better than \"deploy\""
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results to return (default 5)",
                        "default": 5
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Token budget — return as many results as fit within this limit (most relevant first). Approximate: 1 token ≈ 4 chars."
                    },
                    "subject": {
                        "type": "string",
                        "description": "Filter results to a specific subject entity (e.g., 'user', 'project:myapp', 'person:sarah')"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "forget",
            "description": "Delete a specific memory by ID. Always use dry_run=true first to confirm what will be deleted, then call again with dry_run=false to actually delete.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Memory ID to delete (UUID format)"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, preview what would be deleted without actually deleting. Always dry_run first.",
                        "default": true
                    }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "update_memory",
            "description": "Update an existing memory. Can change content, tags, subject, and expiry. Use to:\n- Add/remove/change expiry on a memory\n- Update content that has changed\n- Fix tags or subject\n- Make a temporary memory permanent (remove expiry)\n\nAll fields except id are optional — only provided fields are updated.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Memory ID to update (UUID format)"
                    },
                    "content": {
                        "type": "string",
                        "description": "New content (replaces existing)"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "New tags (replaces existing)"
                    },
                    "subject": {
                        "type": "string",
                        "description": "New subject entity. Use null/empty to clear."
                    },
                    "ttl": {
                        "type": "string",
                        "description": "Set new time-to-live from now. Examples: '1h', '24h', '7d'. Replaces any existing expiry."
                    },
                    "expires_at": {
                        "type": "string",
                        "description": "Set explicit expiry (ISO 8601). Use null to remove expiry (make permanent)."
                    },
                    "remove_expiry": {
                        "type": "boolean",
                        "description": "Set to true to remove any existing expiry, making the memory permanent."
                    }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "status",
            "description": "Check ctxovrflw status including memory count, current tier, usage limits, and feature availability. Use this to understand what capabilities are available.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
    ];

    // Context synthesis only for Pro+
    // Subjects tool — available to all tiers
    tools.push(json!({
        "name": "subjects",
        "description": "List all known subject entities and how many memories are stored about each. Use to discover what entities the memory system knows about.",
        "inputSchema": {
            "type": "object",
            "properties": {}
        }
    }));

    if matches!(cfg.tier, Tier::Pro) {
        tools.push(json!({
            "name": "context",
            "description": "Get a synthesized context briefing — pulls relevant memories and summarizes them into a coherent narrative within a token budget. More useful than raw recall when you need a quick overview.\n\nPro tier only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Topic or question to focus the briefing on"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Scope to a specific entity (e.g., 'user', 'project:myapp')"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Max tokens for the briefing (default 2000)",
                        "default": 2000
                    }
                }
            }
        }));
    }

    tools
}

/// Parse a TTL string like "1h", "24h", "7d", "30m" into an expiry timestamp.
fn parse_ttl(ttl: &str) -> Result<String> {
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
        anyhow::bail!("Invalid TTL format: '{ttl}'. Use format like '1h', '24h', '7d', '30m'");
    };
    let num: i64 = num_str.parse().map_err(|_| anyhow::anyhow!("Invalid TTL number: '{num_str}'"))?;
    if num <= 0 {
        anyhow::bail!("TTL must be positive");
    }
    let expires = chrono::Utc::now() + chrono::Duration::seconds(num * multiplier);
    Ok(expires.to_rfc3339())
}

/// Resolve expiry from ttl or expires_at args. Returns Ok(Some(timestamp)) or Ok(None).
fn resolve_expiry(args: &Value) -> Result<Option<String>> {
    if let Some(ttl) = args["ttl"].as_str() {
        return Ok(Some(parse_ttl(ttl)?));
    }
    if let Some(exp) = args["expires_at"].as_str() {
        // Validate it parses as a datetime
        let _ = chrono::DateTime::parse_from_rfc3339(exp)
            .map_err(|_| anyhow::anyhow!("Invalid expires_at: must be ISO 8601 / RFC 3339"))?;
        return Ok(Some(exp.to_string()));
    }
    Ok(None)
}

pub async fn call_tool(cfg: &Config, params: &Value) -> Result<Value> {
    let tool_name = params["name"].as_str().unwrap_or("");
    let arguments = &params["arguments"];

    match tool_name {
        "remember" => handle_remember(cfg, arguments).await,
        "recall" => handle_recall(cfg, arguments).await,
        "forget" => handle_forget(cfg, arguments).await,
        "update_memory" => handle_update_memory(cfg, arguments).await,
        "status" => handle_status(cfg).await,
        "subjects" => handle_subjects().await,
        "context" => handle_context(cfg, arguments).await,
        _ => Ok(json!({
            "content": [{ "type": "text", "text": format!("Unknown tool: {tool_name}") }],
            "isError": true
        })),
    }
}

async fn handle_remember(cfg: &Config, args: &Value) -> Result<Value> {
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("content is required"))?;
    let memory_type = args["type"]
        .as_str()
        .unwrap_or("semantic")
        .parse()
        .unwrap_or_default();
    let tags: Vec<String> = args["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = db::open()?;

    // Check memory limit
    let count = db::memories::count(&conn)?;
    if let Some(max) = cfg.tier.max_memories() {
        if count >= max {
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Memory limit reached ({max}). Upgrade to store more: https://ctxovrflw.dev/pricing")
                }],
                "isError": true
            }));
        }
    }

    // Generate embedding if semantic search is available
    let embedding = if cfg.tier.semantic_search_enabled() {
        match crate::embed::Embedder::new() {
            Ok(mut embedder) => embedder.embed(content).ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    let subject = args["subject"].as_str();

    let expires_at = match resolve_expiry(args) {
        Ok(e) => e,
        Err(e) => return Ok(json!({
            "content": [{ "type": "text", "text": format!("Invalid expiry: {e}") }],
            "isError": true
        })),
    };

    let memory = db::memories::store_with_expiry(
        &conn,
        content,
        &memory_type,
        &tags,
        subject,
        Some("mcp"),
        embedding.as_deref(),
        expires_at.as_deref(),
    )?;

    // Immediate push to cloud
    if cfg.is_logged_in() {
        let id = memory.id.clone();
        let cfg2 = cfg.clone();
        tokio::spawn(async move {
            let _ = crate::sync::push_one(&cfg2, &id).await;
        });
    }

    let expiry_note = match &memory.expires_at {
        Some(e) => format!(" (expires: {e})"),
        None => String::new(),
    };

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Remembered: {} (id: {}){}", content, memory.id, expiry_note)
        }]
    }))
}

async fn handle_recall(cfg: &Config, args: &Value) -> Result<Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("query is required"))?;
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;
    let max_tokens = args["max_tokens"].as_u64().map(|t| t as usize);
    let subject_filter = args["subject"].as_str();

    // Sync before recall to get latest from other devices
    if cfg.is_logged_in() {
        let _ = crate::sync::run_silent(cfg).await;
    }

    use crate::db::search::SearchMethod;

    let conn = db::open()?;

    // If subject filter is set, get subject-scoped results
    if let Some(subj) = subject_filter {
        let memories = db::search::by_subject(&conn, subj, limit)?;
        if memories.is_empty() {
            return Ok(json!({
                "content": [{ "type": "text", "text": format!("No memories found for subject: {subj}") }]
            }));
        }
        let mut text = format!("Memories about '{subj}':\n\n");
        let mut token_count = 0usize;
        for memory in &memories {
            let line = format!(
                "- [{}] ({}) {}{}\n",
                memory.id, memory.memory_type, memory.content,
                memory.subject.as_deref().map(|s| format!(" [{}]", s)).unwrap_or_default()
            );
            let line_tokens = line.len() / 4; // ~4 chars per token
            if let Some(budget) = max_tokens {
                if token_count + line_tokens > budget { break; }
            }
            token_count += line_tokens;
            text.push_str(&line);
        }
        return Ok(json!({
            "content": [{ "type": "text", "text": text }]
        }));
    }

    // Fetch more results than needed if we have a token budget (to fill it optimally)
    let fetch_limit = if max_tokens.is_some() { limit.max(20) } else { limit };

    let (results, method) = if cfg.tier.semantic_search_enabled() {
        match crate::embed::Embedder::new() {
            Ok(mut embedder) => match embedder.embed(query) {
                Ok(embedding) => {
                    let sem = db::search::semantic_search(&conn, &embedding, fetch_limit)?;
                    if !sem.is_empty() {
                        (sem, SearchMethod::Semantic)
                    } else {
                        (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword)
                    }
                }
                Err(_) => (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword),
            },
            Err(_) => (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword),
        }
    } else {
        (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword)
    };

    if results.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No memories found." }]
        }));
    }

    let mut text = format!("Found memories (search: {method}):\n\n");
    let mut token_count = 0usize;
    let mut included = 0usize;
    for (memory, score) in &results {
        let line = format!(
            "- [{}] ({}, score: {:.2}) {}{}\n",
            memory.id, memory.memory_type, score, memory.content,
            memory.subject.as_deref().map(|s| format!(" [{}]", s)).unwrap_or_default()
        );
        let line_tokens = line.len() / 4;
        if let Some(budget) = max_tokens {
            if token_count + line_tokens > budget { break; }
        }
        if max_tokens.is_none() && included >= limit { break; }
        token_count += line_tokens;
        included += 1;
        text.push_str(&line);
    }

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn handle_subjects() -> Result<Value> {
    let conn = db::open()?;
    let subjects = db::search::list_subjects(&conn)?;

    if subjects.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No subject entities found. Use the 'subject' field when storing memories to organize them by entity." }]
        }));
    }

    let mut text = String::from("Known subjects:\n\n");
    for (subject, count) in &subjects {
        text.push_str(&format!("- {} ({} memories)\n", subject, count));
    }
    text.push_str("\nUse recall with subject filter to get memories about a specific entity.");

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn handle_forget(_cfg: &Config, args: &Value) -> Result<Value> {
    let id = args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("id is required"))?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(true);

    let conn = db::open()?;

    if dry_run {
        if let Some(memory) = db::memories::get(&conn, id)? {
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Would delete: [{}] {}\nRun with dry_run=false to confirm.", memory.id, memory.content)
                }]
            }));
        }
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Memory {id} not found.") }]
        }));
    }

    let deleted = db::memories::delete(&conn, id)?;
    let msg = if deleted {
        format!("Deleted memory {id}.")
    } else {
        format!("Memory {id} not found.")
    };

    Ok(json!({
        "content": [{ "type": "text", "text": msg }]
    }))
}

async fn handle_update_memory(cfg: &Config, args: &Value) -> Result<Value> {
    let id = args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("id is required"))?;

    let conn = db::open()?;

    // Check memory exists
    let existing = db::memories::get(&conn, id)?;
    if existing.is_none() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Memory {id} not found.") }],
            "isError": true
        }));
    }

    let content = args["content"].as_str();
    let tags: Option<Vec<String>> = args["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect());
    let subject = if args.get("subject").is_some() {
        Some(args["subject"].as_str()) // Some(None) = clear, Some(Some(x)) = set
    } else {
        None
    };

    // Resolve expiry: remove_expiry > ttl > expires_at > no change
    let expires_at = if args["remove_expiry"].as_bool().unwrap_or(false) {
        Some(None) // explicitly remove
    } else if args["ttl"].as_str().is_some() || args["expires_at"].as_str().is_some() {
        match resolve_expiry(args) {
            Ok(Some(e)) => Some(Some(e)),
            Ok(None) => None,
            Err(e) => return Ok(json!({
                "content": [{ "type": "text", "text": format!("Invalid expiry: {e}") }],
                "isError": true
            })),
        }
    } else {
        None
    };

    // Re-embed if content changed
    let embedding = if let Some(new_content) = content {
        if cfg.tier.semantic_search_enabled() {
            crate::embed::Embedder::new()
                .ok()
                .and_then(|mut e| e.embed(new_content).ok())
        } else {
            None
        }
    } else {
        None
    };

    let expires_ref = expires_at.as_ref().map(|e| e.as_deref());

    let updated = db::memories::update(
        &conn,
        id,
        content,
        tags.as_deref(),
        subject,
        expires_ref,
        embedding.as_deref(),
    )?;

    match updated {
        Some(mem) => {
            // Push update to cloud
            if cfg.is_logged_in() {
                let mid = mem.id.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move {
                    let _ = crate::sync::push_one(&cfg2, &mid).await;
                });
            }

            let mut changes = Vec::new();
            if content.is_some() { changes.push("content"); }
            if tags.is_some() { changes.push("tags"); }
            if subject.is_some() { changes.push("subject"); }
            if expires_at.is_some() { changes.push("expiry"); }

            let expiry_info = match &mem.expires_at {
                Some(e) => format!(" | expires: {e}"),
                None => " | no expiry".to_string(),
            };

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Updated memory {} (changed: {}){}", id, changes.join(", "), expiry_info)
                }]
            }))
        }
        None => Ok(json!({
            "content": [{ "type": "text", "text": format!("Memory {id} not found.") }],
            "isError": true
        })),
    }
}

async fn handle_status(cfg: &Config) -> Result<Value> {
    let conn = db::open()?;
    let count = db::memories::count(&conn)?;
    let max = cfg
        .tier
        .max_memories()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "unlimited".to_string());

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "ctxovrflw v{}\nTier: {:?}\nMemories: {}/{}\nSemantic search: {}\nCloud sync: {}",
                env!("CARGO_PKG_VERSION"),
                cfg.tier,
                count,
                max,
                if cfg.tier.semantic_search_enabled() { "enabled" } else { "keyword only" },
                if cfg.tier.cloud_sync_enabled() { "enabled" } else { "disabled" }
            )
        }]
    }))
}

async fn handle_context(cfg: &Config, args: &Value) -> Result<Value> {
    if !cfg.tier.context_synthesis_enabled() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Context synthesis requires Pro tier ($20/mo). Upgrade at https://ctxovrflw.dev/pricing" }]
        }));
    }

    let topic = args["topic"].as_str();
    let subject_filter = args["subject"].as_str();
    let max_tokens = args["max_tokens"].as_u64().unwrap_or(2000) as usize;

    // Sync first
    if cfg.is_logged_in() {
        let _ = crate::sync::run_silent(cfg).await;
    }

    let conn = db::open()?;

    // Gather memories — by subject if specified, by topic search if specified, or all recent
    let mut all_memories: Vec<db::memories::Memory> = Vec::new();

    if let Some(subj) = subject_filter {
        all_memories.extend(db::search::by_subject(&conn, subj, 50)?);
    }

    if let Some(q) = topic {
        if cfg.tier.semantic_search_enabled() {
            if let Ok(mut embedder) = crate::embed::Embedder::new() {
                if let Ok(embedding) = embedder.embed(q) {
                    let sem = db::search::semantic_search(&conn, &embedding, 20).unwrap_or_default();
                    for (mem, _score) in sem {
                        if !all_memories.iter().any(|m| m.id == mem.id) {
                            all_memories.push(mem);
                        }
                    }
                }
            }
        }
    }

    // If we still have few results, add recent memories
    if all_memories.len() < 10 {
        let recent = db::memories::list(&conn, 20, 0)?;
        for mem in recent {
            if !all_memories.iter().any(|m| m.id == mem.id) {
                all_memories.push(mem);
            }
        }
    }

    if all_memories.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No memories found for context synthesis." }]
        }));
    }

    // Group by subject, then by type within each group
    let mut by_subject: std::collections::BTreeMap<String, Vec<&db::memories::Memory>> = std::collections::BTreeMap::new();
    let mut no_subject: Vec<&db::memories::Memory> = Vec::new();

    for mem in &all_memories {
        match &mem.subject {
            Some(s) => by_subject.entry(s.clone()).or_default().push(mem),
            None => no_subject.push(mem),
        }
    }

    // Build the briefing within token budget
    let mut briefing = String::new();
    let mut token_count = 0usize;

    // Header
    let header = match (topic, subject_filter) {
        (Some(t), Some(s)) => format!("# Context Briefing: {} ({})\n\n", t, s),
        (Some(t), None) => format!("# Context Briefing: {}\n\n", t),
        (None, Some(s)) => format!("# Context Briefing: {}\n\n", s),
        (None, None) => "# Context Briefing\n\n".to_string(),
    };
    briefing.push_str(&header);
    token_count += header.len() / 4;

    // Subjects first
    for (subject, mems) in &by_subject {
        if token_count >= max_tokens { break; }

        let section = format!("## {}\n", subject);
        briefing.push_str(&section);
        token_count += section.len() / 4;

        // Group by type within subject
        let mut preferences: Vec<&str> = Vec::new();
        let mut facts: Vec<&str> = Vec::new();
        let mut procedures: Vec<&str> = Vec::new();
        let mut events: Vec<&str> = Vec::new();

        for mem in mems {
            match mem.memory_type {
                db::memories::MemoryType::Preference => preferences.push(&mem.content),
                db::memories::MemoryType::Semantic => facts.push(&mem.content),
                db::memories::MemoryType::Procedural => procedures.push(&mem.content),
                db::memories::MemoryType::Episodic => events.push(&mem.content),
            }
        }

        for (label, items) in [
            ("Preferences", &preferences),
            ("Facts", &facts),
            ("Procedures", &procedures),
            ("Events", &events),
        ] {
            if items.is_empty() || token_count >= max_tokens { continue; }
            let sub = format!("**{}:** ", label);
            briefing.push_str(&sub);
            token_count += sub.len() / 4;

            for (i, item) in items.iter().enumerate() {
                let line = if i < items.len() - 1 {
                    format!("{} · ", item)
                } else {
                    format!("{}\n", item)
                };
                if token_count + line.len() / 4 > max_tokens { break; }
                briefing.push_str(&line);
                token_count += line.len() / 4;
            }
            briefing.push('\n');
        }
    }

    // Ungrouped memories
    if !no_subject.is_empty() && token_count < max_tokens {
        briefing.push_str("## General\n");
        token_count += 12;
        for mem in &no_subject {
            if token_count >= max_tokens { break; }
            let line = format!("- ({}) {}\n", mem.memory_type, mem.content);
            if token_count + line.len() / 4 > max_tokens { break; }
            briefing.push_str(&line);
            token_count += line.len() / 4;
        }
    }

    // Footer
    let footer = format!(
        "\n---\n*{} memories synthesized, ~{} tokens*",
        all_memories.len(),
        token_count
    );
    briefing.push_str(&footer);

    Ok(json!({
        "content": [{ "type": "text", "text": briefing }]
    }))
}
