use anyhow::Result;
use serde_json::{json, Value};

use crate::config::{Config, Tier};
use crate::db;
use crate::validation::{self, validate_tags, validate_subject, validate_agent_id, MAX_CONTENT_SIZE};

const MEMORY_CHUNK_THRESHOLD_CHARS: usize = 2200;
const MEMORY_CHUNK_SIZE_CHARS: usize = 1800;
const MEMORY_CHUNK_OVERLAP_CHARS: usize = 220;

pub fn list_tools(cfg: &Config) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "remember",
            "description": "Store important context to shared memory. **Call this proactively** — don't wait for the user to say 'remember this'. Whenever you learn something worth knowing in a future session, store it immediately and silently.\n\nThis memory persists across sessions and is accessible by ALL connected AI tools (Cursor, Claude Code, Cline, etc.).\n\nUSE THIS WHEN:\n- The user shares a preference (\"I prefer tabs\", \"I use Fly.io\")\n- A decision is made (\"We're using Rust for the backend\")\n- Important project context comes up (\"The API is at api.example.com\")\n- The user explicitly asks you to remember something\n- You learn ANYTHING worth knowing in a future session — just store it silently\n\nBEST PRACTICES:\n- Store atomic facts, not paragraphs\n- Use tags for organization\n- Set subject to identify WHO or WHAT this memory is about\n- Choose the right type: preference for likes/config, semantic for facts, episodic for events, procedural for how-to steps\n- Don't announce that you're remembering — just do it",
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
                    "agent_id": {
                        "type": "string",
                        "description": "Self-identification of the AI agent storing this memory. Use your name or tool name (e.g., 'aldous', 'cursor', 'claude-code'). Enables cross-agent memory filtering."
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
            "description": "Search shared memory for relevant context. **Call this at the start of every conversation** and whenever past context would help. Don't wait for the user to ask 'do you remember' — check proactively.\n\nResults come from ALL connected AI tools — something stored by Cursor can be recalled by Claude Code.\n\nUSE THIS WHEN:\n- **At the START of every session** — recall context about the current project/topic\n- Before answering questions about the user's preferences, setup, or past decisions\n- The user asks \"do you remember...\" or \"what did I say about...\"\n- You need project context that might have been discussed in another tool\n- Before suggesting an approach — check if there's a stated preference\n\nTIPS:\n- Use natural language queries (\"coding preferences\" not just \"tabs\")\n- Semantic search finds conceptually related memories, not just keyword matches\n- Use subject filter to scope results (\"everything about project X\")\n- Use max_tokens to control context window usage",
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
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Filter results to memories stored by a specific agent (e.g., 'aldous', 'cursor')"
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

        json!({
            "name": "pin_memory",
            "description": "Pin a memory so it gets prioritized during future recalls and preflight checks. Adds tags like 'pinned' and optional policy/workflow tags.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Memory ID to pin" },
                    "policy": { "type": "boolean", "description": "Also mark as policy memory", "default": false },
                    "workflow": { "type": "boolean", "description": "Also mark as workflow memory", "default": false }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "unpin_memory",
            "description": "Remove pin/policy/workflow tags from a memory so it is no longer prioritized.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Memory ID to unpin" }
                },
                "required": ["id"]
            }
        }),
    ];

    // ── Knowledge Graph tools (Standard+ tier) ──
    if matches!(cfg.tier, Tier::Standard | Tier::Pro) {
        tools.push(json!({
            "name": "add_entity",
            "description": "Add an entity to the knowledge graph. Entities represent people, projects, services, concepts, etc. If an entity with the same name+type already exists, its metadata is updated.\n\n**Call this proactively** when you encounter named things worth tracking: people, repos, services, APIs, tools, companies, concepts.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Entity name (e.g., 'auth-service', 'Max', 'PostgreSQL', 'ctxovrflw')"
                    },
                    "type": {
                        "type": "string",
                        "description": "Entity type (e.g., 'person', 'service', 'database', 'project', 'tool', 'concept', 'file', 'api')",
                        "default": "generic"
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional structured metadata about the entity"
                    }
                },
                "required": ["name", "type"]
            }
        }));

        tools.push(json!({
            "name": "add_relation",
            "description": "Add a relationship between two entities in the knowledge graph. If the relation already exists, updates confidence.\n\n**Call this when you learn how things connect:** service A depends on B, person X owns project Y, tool Z reads from API W.\n\nEntities are resolved by name+type. If they don't exist yet, create them first with add_entity.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source entity name"
                    },
                    "source_type": {
                        "type": "string",
                        "description": "Source entity type",
                        "default": "generic"
                    },
                    "target": {
                        "type": "string",
                        "description": "Target entity name"
                    },
                    "target_type": {
                        "type": "string",
                        "description": "Target entity type",
                        "default": "generic"
                    },
                    "relation": {
                        "type": "string",
                        "description": "Relationship type (e.g., 'depends_on', 'owns', 'uses', 'created_by', 'part_of', 'connects_to', 'deployed_on', 'tested_by')"
                    },
                    "confidence": {
                        "type": "number",
                        "description": "Confidence 0.0-1.0 (default 1.0). Use lower values for inferred relationships.",
                        "default": 1.0
                    }
                },
                "required": ["source", "source_type", "target", "target_type", "relation"]
            }
        }));

        tools.push(json!({
            "name": "get_relations",
            "description": "Query relationships for an entity. Returns all connections (incoming and outgoing).\n\nUse this to understand how things connect: 'what does auth-service depend on?', 'who owns this project?', 'what uses PostgreSQL?'\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity name to query"
                    },
                    "entity_type": {
                        "type": "string",
                        "description": "Entity type (helps disambiguate)"
                    },
                    "relation_type": {
                        "type": "string",
                        "description": "Filter to specific relation type (e.g., 'depends_on')"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["outgoing", "incoming", "both"],
                        "description": "Direction filter. 'outgoing' = relations FROM this entity, 'incoming' = TO this entity",
                        "default": "both"
                    }
                },
                "required": ["entity"]
            }
        }));

        tools.push(json!({
            "name": "traverse",
            "description": "Traverse the knowledge graph from an entity up to N hops. Returns all reachable entities with the path taken.\n\nUse for impact analysis: 'what would break if I change this DB schema?' or discovery: 'show me everything connected to this project within 2 hops'.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Starting entity name"
                    },
                    "entity_type": {
                        "type": "string",
                        "description": "Starting entity type (helps disambiguate)"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Max hops to traverse (1-5, default 2)",
                        "default": 2
                    },
                    "relation_type": {
                        "type": "string",
                        "description": "Only follow edges of this type"
                    },
                    "min_confidence": {
                        "type": "number",
                        "description": "Minimum confidence threshold (0.0-1.0, default 0.0)",
                        "default": 0.0
                    }
                },
                "required": ["entity"]
            }
        }));

        tools.push(json!({
            "name": "list_entities",
            "description": "List all entities in the knowledge graph, optionally filtered by type.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "Filter by entity type"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search entities by name (substring match)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results (default 50)",
                        "default": 50
                    }
                }
            }
        }));

        tools.push(json!({
            "name": "delete_entity",
            "description": "Delete an entity and all its relations from the knowledge graph.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity name"
                    },
                    "entity_type": {
                        "type": "string",
                        "description": "Entity type (required to disambiguate)"
                    }
                },
                "required": ["entity", "entity_type"]
            }
        }));

        tools.push(json!({
            "name": "delete_relation",
            "description": "Delete a specific relation by ID.\n\nStandard+ tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Relation ID to delete"
                    }
                },
                "required": ["id"]
            }
        }));
    }

    // ── Webhook tools (Standard + Pro tier) ──
    #[cfg(feature = "pro")]
    tools.push(json!({
        "name": "manage_webhooks",
        "description": "Manage webhook subscriptions for memory and graph events. Webhooks fire HTTP POST to your URL when events occur.\n\nActions: 'list', 'create', 'delete', 'enable', 'disable'.\n\nValid events: memory.created, memory.updated, memory.deleted, entity.created, entity.updated, entity.deleted, relation.created, relation.updated, relation.deleted",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "delete", "enable", "disable"],
                    "description": "Webhook action"
                },
                "url": {
                    "type": "string",
                    "description": "Webhook URL (for 'create')"
                },
                "events": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Event types to subscribe to (for 'create')"
                },
                "secret": {
                    "type": "string",
                    "description": "HMAC secret for signing payloads (for 'create')"
                },
                "id": {
                    "type": "string",
                    "description": "Webhook ID (for 'delete', 'enable', 'disable')"
                }
            },
            "required": ["action"]
        }
    }));

    // ── Consolidation tool (Pro tier) ──
    #[cfg(feature = "pro")]
    if matches!(cfg.tier, Tier::Pro) {
        tools.push(json!({
            "name": "consolidate",
            "description": "Get related/duplicate memories for a subject or topic, so you can review and merge them. Returns candidate groups.\n\nWorkflow: call consolidate → review candidates → use update_memory to merge/deduplicate → use forget to remove redundant ones.\n\nPro tier only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "subject": {
                        "type": "string",
                        "description": "Subject entity to consolidate memories for"
                    },
                    "topic": {
                        "type": "string",
                        "description": "Topic to find related memories (uses semantic search)"
                    }
                }
            }
        }));

        tools.push(json!({
            "name": "maintenance",
            "description": "Run or plan autonomous memory maintenance workflows. Use this for background consolidation orchestration and OpenClaw-aware scheduling hints.\n\nPro tier only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["run_consolidation_now", "openclaw_schedule_hint"],
                        "description": "Maintenance action"
                    }
                },
                "required": ["action"]
            }
        }));
    }

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

    #[cfg(feature = "pro")]
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

/// Resolve expiry from JSON args using shared validation.
fn resolve_expiry_from_args(args: &Value) -> Result<Option<String>> {
    let ttl = args["ttl"].as_str();
    let expires_at = args["expires_at"].as_str();
    validation::resolve_expiry(ttl, expires_at).map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn call_tool(cfg: &Config, params: &Value) -> Result<Value> {
    let tool_name = params["name"].as_str().unwrap_or("");
    let arguments = &params["arguments"];

    // Knowledge graph tools (Standard+ tier, runtime check)
    if cfg.tier.knowledge_graph_enabled() {
        match tool_name {
            "add_entity" => return handle_add_entity(arguments).await,
            "add_relation" => return handle_add_relation(arguments).await,
            "get_relations" => return handle_get_relations(arguments).await,
            "traverse" => return handle_traverse(arguments).await,
            "list_entities" => return handle_list_entities(arguments).await,
            "delete_entity" => return handle_delete_entity(arguments).await,
            "delete_relation" => return handle_delete_relation(arguments).await,
            _ => {}
        }
    }

    // Pro-tier tools dispatched when feature is enabled
    #[cfg(feature = "pro")]
    match tool_name {
        "context" => return handle_context(cfg, arguments).await,
        "manage_webhooks" => return handle_manage_webhooks(arguments).await,
        "consolidate" => return handle_consolidate(cfg, arguments).await,
        "maintenance" => return handle_maintenance(cfg, arguments).await,
        _ => {}
    }

    match tool_name {
        "remember" => handle_remember(cfg, arguments).await,
        "recall" => handle_recall(cfg, arguments).await,
        "forget" => handle_forget(cfg, arguments).await,
        "update_memory" => handle_update_memory(cfg, arguments).await,
        "status" => handle_status(cfg).await,
        "subjects" => handle_subjects().await,
        "pin_memory" => handle_pin_memory(cfg, arguments).await,
        "unpin_memory" => handle_unpin_memory(cfg, arguments).await,
        _ => Ok(json!({
            "content": [{ "type": "text", "text": format!("Unknown tool: {tool_name}") }],
            "isError": true
        })),
    }
}

// Validation functions and constants imported from crate::validation

async fn handle_remember(cfg: &Config, args: &Value) -> Result<Value> {
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("content is required"))?;
    if content.trim().is_empty() {
        anyhow::bail!("content cannot be empty");
    }
    if content.len() > MAX_CONTENT_SIZE {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Content too large ({} bytes). Maximum is {} bytes.", content.len(), MAX_CONTENT_SIZE) }],
            "isError": true
        }));
    }
    let memory_type = args["type"]
        .as_str()
        .unwrap_or("semantic")
        .parse()
        .unwrap_or_default();
    let raw_tags: Vec<String> = args["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let tags = match validate_tags(&raw_tags) {
        Ok(t) => t,
        Err(e) => return Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        })),
    };

    let conn = db::open()?;

    // Check memory limit
    let count = db::memories::count(&conn)?;
    if let Some(max) = cfg.effective_max_memories() {
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

    let subject = args["subject"].as_str();
    if let Err(e) = validate_subject(subject) {
        return Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }));
    }

    let agent_id = args["agent_id"].as_str();
    if let Err(e) = validate_agent_id(agent_id) {
        return Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }));
    }

    let expires_at = match resolve_expiry_from_args(args) {
        Ok(e) => e,
        Err(e) => return Ok(json!({
            "content": [{ "type": "text", "text": format!("Invalid expiry: {e}") }],
            "isError": true
        })),
    };

    let chunks = if content.chars().count() > MEMORY_CHUNK_THRESHOLD_CHARS {
        crate::chunking::split_text_with_overlap(content, MEMORY_CHUNK_SIZE_CHARS, MEMORY_CHUNK_OVERLAP_CHARS)
    } else {
        vec![content.to_string()]
    };

    let chunk_parent = if chunks.len() > 1 {
        Some(format!("chunkset:{}", uuid::Uuid::new_v4()))
    } else {
        None
    };

    let mut stored: Vec<db::memories::Memory> = Vec::new();
    for (idx, chunk) in chunks.iter().enumerate() {
        let mut chunk_tags = tags.clone();
        if let Some(parent) = &chunk_parent {
            chunk_tags.push("chunked".to_string());
            chunk_tags.push(parent.clone());
            chunk_tags.push(format!("chunk_index:{}", idx + 1));
            chunk_tags.push(format!("chunk_total:{}", chunks.len()));
        }
        let chunk_tags = validate_tags(&chunk_tags).unwrap_or(chunk_tags);

        // Generate embedding per chunk if semantic search is available
        let embedding = if cfg.tier.semantic_search_enabled() {
            match crate::embed::get_or_init() {
                Ok(emb_arc) => emb_arc.lock().unwrap_or_else(|e| e.into_inner()).embed(chunk).ok(),
                Err(_) => None,
            }
        } else {
            None
        };

        let mem = db::memories::store_with_expiry(
            &conn,
            chunk,
            &memory_type,
            &chunk_tags,
            subject,
            Some("mcp"),
            embedding.as_deref(),
            expires_at.as_deref(),
            agent_id,
        )?;

        // Immediate push to cloud
        if cfg.is_logged_in() {
            let id = mem.id.clone();
            let cfg2 = cfg.clone();
            tokio::spawn(async move {
                let _ = crate::sync::push_one(&cfg2, &id).await;
            });
        }

        { #[cfg(feature = "pro")] crate::webhooks::fire("memory.created", json!({ "memory": mem })); }

        // Auto-extract entities from memory into knowledge graph (Standard+ tier, best-effort)
        if cfg.tier.knowledge_graph_enabled() {
            let _ = auto_extract_graph_from_memory(&conn, &mem);
        }

        stored.push(mem);
    }

    if stored.len() == 1 {
        let memory = &stored[0];
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
    } else {
        let ids: Vec<String> = stored.iter().map(|m| m.id.clone()).collect();
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Remembered as {} linked chunks ({}). First id: {}",
                    stored.len(),
                    chunk_parent.unwrap_or_default(),
                    ids.first().cloned().unwrap_or_default()
                )
            }],
            "details": {
                "chunked": true,
                "count": stored.len(),
                "ids": ids
            }
        }))
    }
}

async fn handle_recall(cfg: &Config, args: &Value) -> Result<Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("query is required"))?;
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;
    let max_tokens = args["max_tokens"].as_u64().map(|t| t as usize);
    let subject_filter = args["subject"].as_str();
    let agent_id_filter = args["agent_id"].as_str();

    // Sync happens on its own schedule (auto-sync daemon task).
    // Don't trigger a full sync before every recall — it adds latency.

    use crate::db::search::SearchMethod;

    let conn = db::open()?;

    // If subject filter is set, use it as a boost signal (not a hard filter).
    // Try exact → fuzzy → fall through to semantic/hybrid search.
    if let Some(subj) = subject_filter {
        // 1. Exact match
        let mut subject_memories = db::search::by_subject(&conn, subj, limit)?;

        // 2. If exact match found nothing, try fuzzy
        if subject_memories.is_empty() {
            subject_memories = db::search::by_subject_fuzzy(&conn, subj, limit)?;
        }

        // 3. Also do a semantic/hybrid search on the query to find more relevant results
        let extra_results = {
            let fetch_extra = limit.saturating_sub(subject_memories.len()).max(3);
            if cfg.tier.semantic_search_enabled() {
                match crate::embed::get_or_init() {
                    Ok(emb_arc) => match emb_arc.lock().unwrap_or_else(|e| e.into_inner()).embed(query) {
                        Ok(embedding) => {
                            #[cfg(feature = "pro")]
                            { db::search::hybrid_search(&conn, query, &embedding, fetch_extra).unwrap_or_default() }
                            #[cfg(not(feature = "pro"))]
                            { db::search::semantic_search(&conn, &embedding, fetch_extra).unwrap_or_default() }
                        }
                        Err(_) => db::search::keyword_search(&conn, query, fetch_extra).unwrap_or_default(),
                    },
                    Err(_) => db::search::keyword_search(&conn, query, fetch_extra).unwrap_or_default(),
                }
            } else {
                db::search::keyword_search(&conn, query, fetch_extra).unwrap_or_default()
            }
        };

        // 4. Merge: subject-matched first, then extra (deduped)
        let subject_ids: std::collections::HashSet<String> = subject_memories.iter().map(|m| m.id.clone()).collect();
        let mut all_memories: Vec<(db::memories::Memory, Option<f64>)> = subject_memories.into_iter().map(|m| (m, None)).collect();
        for (mem, score) in extra_results {
            if !subject_ids.contains(&mem.id) && all_memories.len() < limit {
                all_memories.push((mem, Some(score)));
            }
        }

        if all_memories.is_empty() {
            return Ok(json!({
                "content": [{ "type": "text", "text": format!("No memories found for subject: {subj}") }]
            }));
        }

        let mut text = format!("Memories about '{subj}':\n\n");
        let mut token_count = 0usize;
        for (memory, score) in &all_memories {
            let score_str = score.map(|s| format!(", score: {:.2}", s)).unwrap_or_default();
            let line = format!(
                "- [{}] ({}{}){} {}\n",
                memory.id, memory.memory_type, score_str,
                memory.subject.as_deref().map(|s| format!(" [{}]", s)).unwrap_or_default(),
                memory.content,
            );
            let line_tokens = line.len() / 4;
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

    // Agent-scoped search
    if let Some(agent_id) = agent_id_filter {
        let memories = db::search::by_agent(&conn, agent_id, limit)?;
        if memories.is_empty() {
            return Ok(json!({
                "content": [{ "type": "text", "text": format!("No memories found for agent: {agent_id}") }]
            }));
        }
        let mut text = format!("Memories from agent '{agent_id}':\n\n");
        let mut token_count = 0usize;
        for memory in &memories {
            let line = format!(
                "- [{}] ({}){} {}\n",
                memory.id, memory.memory_type,
                memory.subject.as_deref().map(|s| format!(" [{}]", s)).unwrap_or_default(),
                memory.content,
            );
            let line_tokens = line.len() / 4;
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
        match crate::embed::get_or_init() {
            Ok(emb_arc) => match emb_arc.lock().unwrap_or_else(|e| e.into_inner()).embed(query) {
                Ok(embedding) => {
                    #[cfg(feature = "pro")]
                    {
                        let hybrid = db::search::hybrid_search(&conn, query, &embedding, fetch_limit)?;
                        if !hybrid.is_empty() {
                            (hybrid, SearchMethod::Hybrid)
                        } else {
                            (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword)
                        }
                    }
                    #[cfg(not(feature = "pro"))]
                    {
                        let sem = db::search::semantic_search(&conn, &embedding, fetch_limit)?;
                        if !sem.is_empty() {
                            (sem, SearchMethod::Semantic)
                        } else {
                            (db::search::keyword_search(&conn, query, fetch_limit)?, SearchMethod::Keyword)
                        }
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

    // Graph-boosted results: find memories related via knowledge graph entities
    let results = if cfg.tier.knowledge_graph_enabled() {
        let mut results = results;
        let result_ids: std::collections::HashSet<String> = results.iter().map(|(m, _)| m.id.clone()).collect();
        if let Ok(entities) = db::graph::search_entities(&conn, query, None, 3) {
            for entity in &entities {
                if let Ok(relations) = db::graph::get_relations(&conn, &entity.id, None, None) {
                    for (_rel, _source, target) in &relations {
                        if let Ok(related_mems) = db::search::by_subject_fuzzy(&conn, &target.name, 3) {
                            for mem in related_mems {
                                if !result_ids.contains(&mem.id) && results.len() < fetch_limit {
                                    results.push((mem, 0.01)); // low score = graph-boosted
                                }
                            }
                        }
                    }
                }
            }
        }
        results
    } else {
        results
    };

    let mut text = format!("Found memories (search: {method}):\n\n");
    let mut token_count = 0usize;
    let mut included = 0usize;
    let min_score = results.iter().map(|(_, s)| *s).fold(f64::INFINITY, f64::min);
    let max_score = results.iter().map(|(_, s)| *s).fold(f64::NEG_INFINITY, f64::max);
    let score_band = (max_score - min_score).abs().max(1e-9);

    for (memory, score) in &results {
        let percentile = ((*score - min_score) / score_band).clamp(0.0, 1.0);
        let confidence = if percentile >= 0.75 {
            "high"
        } else if percentile >= 0.40 {
            "medium"
        } else {
            "low"
        };

        let line = format!(
            "- [{}] ({}, score: {:.2}, conf: {}, pct: {:.0}%) {}{}\n",
            memory.id,
            memory.memory_type,
            score,
            confidence,
            percentile * 100.0,
            memory.content,
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

    // Graph context: enrich results with entity relationships
    if cfg.tier.knowledge_graph_enabled() {
        let mut seen_entities: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut graph_lines: Vec<String> = Vec::new();
        for (memory, _) in &results {
            if let Some(subj) = &memory.subject {
                let entity_name = if let Some((_t, n)) = subj.split_once(':') { n } else { subj.as_str() };
                if seen_entities.contains(entity_name) { continue; }
                seen_entities.insert(entity_name.to_string());
                if let Ok(found) = db::graph::find_entity(&conn, entity_name, None) {
                    if let Some(entity) = found.first() {
                        if let Ok(rels) = db::graph::get_relations(&conn, &entity.id, None, None) {
                            let rel_strs: Vec<String> = rels.iter().take(3).map(|(r, _s, t)| {
                                format!("{} ({})", t.name, r.relation_type)
                            }).collect();
                            if !rel_strs.is_empty() {
                                graph_lines.push(format!(
                                    "'{}' ({}): connected to {}",
                                    entity.name, entity.entity_type, rel_strs.join(", ")
                                ));
                            }
                        }
                    }
                }
            }
        }
        if !graph_lines.is_empty() {
            text.push_str("\n--- Graph Context ---\n");
            for line in &graph_lines {
                text.push_str(&format!("{}\n", line));
            }
        }
    }

    #[cfg(feature = "pro")]
    if matches!(cfg.tier, Tier::Pro) {
        text.push_str("\n--- Pro Workflow Tip ---\n");
        text.push_str("To keep memory quality high while working: run `maintenance` with action `run_consolidation_now` after major recall sessions, and use `maintenance` with `openclaw_schedule_hint` to set autonomous OpenClaw cron workflows.\n");
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
        { #[cfg(feature = "pro")] crate::webhooks::fire("memory.deleted", json!({ "memory_id": id })); }
        format!("Deleted memory {id}.")
    } else {
        format!("Memory {id} not found.")
    };

    Ok(json!({
        "content": [{ "type": "text", "text": msg }]
    }))
}


async fn handle_pin_memory(cfg: &Config, args: &Value) -> Result<Value> {
    let id = args["id"].as_str().ok_or_else(|| anyhow::anyhow!("id is required"))?;
    let policy = args["policy"].as_bool().unwrap_or(false);
    let workflow = args["workflow"].as_bool().unwrap_or(false);

    let conn = db::open()?;
    let existing = match db::memories::get(&conn, id)? {
        Some(m) => m,
        None => return Ok(json!({ "content": [{ "type": "text", "text": format!("Memory {id} not found.") }], "isError": true })),
    };

    let mut tags = existing.tags.clone();
    for t in ["pinned", if policy { "policy" } else { "" }, if workflow { "workflow" } else { "" }] {
        if !t.is_empty() && !tags.iter().any(|x| x == t) {
            tags.push(t.to_string());
        }
    }

    let tags = validate_tags(&tags).unwrap_or(tags);
    let updated = db::memories::update(&conn, id, None, Some(&tags), None, None, None)?;
    match updated {
        Some(mem) => {
            if cfg.is_logged_in() {
                let mid = mem.id.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move { let _ = crate::sync::push_one(&cfg2, &mid).await; });
            }
            Ok(json!({ "content": [{ "type": "text", "text": format!("Pinned memory {id} with tags: {}", mem.tags.join(", ")) }] }))
        }
        None => Ok(json!({ "content": [{ "type": "text", "text": format!("Memory {id} not found.") }], "isError": true })),
    }
}

async fn handle_unpin_memory(cfg: &Config, args: &Value) -> Result<Value> {
    let id = args["id"].as_str().ok_or_else(|| anyhow::anyhow!("id is required"))?;

    let conn = db::open()?;
    let existing = match db::memories::get(&conn, id)? {
        Some(m) => m,
        None => return Ok(json!({ "content": [{ "type": "text", "text": format!("Memory {id} not found.") }], "isError": true })),
    };

    let remove = ["pinned", "policy", "workflow", "critical"];
    let tags: Vec<String> = existing.tags.into_iter().filter(|t| !remove.contains(&t.as_str())).collect();

    let updated = db::memories::update(&conn, id, None, Some(&tags), None, None, None)?;
    match updated {
        Some(mem) => {
            if cfg.is_logged_in() {
                let mid = mem.id.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move { let _ = crate::sync::push_one(&cfg2, &mid).await; });
            }
            Ok(json!({ "content": [{ "type": "text", "text": format!("Unpinned memory {id}.") }] }))
        }
        None => Ok(json!({ "content": [{ "type": "text", "text": format!("Memory {id} not found.") }], "isError": true })),
    }
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
    let tags: Option<Vec<String>> = match args["tags"].as_array() {
        Some(a) => {
            let raw: Vec<String> = a.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            match validate_tags(&raw) {
                Ok(t) => Some(t),
                Err(e) => return Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true
                })),
            }
        }
        None => None,
    };
    if let Err(e) = validate_subject(args["subject"].as_str()) {
        return Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }));
    }
    let subject = if args.get("subject").is_some() {
        Some(args["subject"].as_str()) // Some(None) = clear, Some(Some(x)) = set
    } else {
        None
    };

    // Resolve expiry: remove_expiry > ttl > expires_at > no change
    let expires_at = if args["remove_expiry"].as_bool().unwrap_or(false) {
        Some(None) // explicitly remove
    } else if args["ttl"].as_str().is_some() || args["expires_at"].as_str().is_some() {
        match resolve_expiry_from_args(args) {
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
            crate::embed::get_or_init()
                .ok()
                .and_then(|arc| arc.lock().unwrap_or_else(|e| e.into_inner()).embed(new_content).ok())
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

            { #[cfg(feature = "pro")] crate::webhooks::fire("memory.updated", json!({ "memory": mem })); }

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
                if cfg.effective_cloud_sync() { "enabled" } else { "disabled" }
            )
        }]
    }))
}

#[cfg(feature = "pro")]
async fn handle_context(cfg: &Config, args: &Value) -> Result<Value> {
    if !cfg.feature_enabled("context_synthesis") {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Context synthesis requires Pro tier ($20/mo). Upgrade at https://ctxovrflw.dev/pricing" }]
        }));
    }

    let topic = args["topic"].as_str();
    let subject_filter = args["subject"].as_str();
    let max_tokens = args["max_tokens"].as_u64().unwrap_or(2000) as usize;

    let conn = db::open()?;

    // Gather memories — by subject if specified, by topic search if specified, or all recent
    let mut all_memories: Vec<db::memories::Memory> = Vec::new();

    if let Some(subj) = subject_filter {
        all_memories.extend(db::search::by_subject(&conn, subj, 50)?);
    }

    if let Some(q) = topic {
        if cfg.tier.semantic_search_enabled() {
            if let Ok(emb_arc) = crate::embed::get_or_init() { let mut embedder = emb_arc.lock().unwrap_or_else(|e| e.into_inner());
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

// ── Knowledge Graph handlers (Standard+ tier) ─────────────────────

async fn handle_add_entity(args: &Value) -> Result<Value> {
    let name = args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("name is required"))?;
    let entity_type = args["type"]
        .as_str()
        .unwrap_or("generic");
    let metadata = args.get("metadata").filter(|v| !v.is_null());

    let conn = db::open()?;
    let entity = db::graph::upsert_entity(&conn, name, entity_type, metadata)?;

    { #[cfg(feature = "pro")] crate::webhooks::fire("entity.created", json!({ "entity": entity })); }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Entity created: {} ({}) [id: {}]", entity.name, entity.entity_type, entity.id)
        }]
    }))
}

async fn handle_add_relation(args: &Value) -> Result<Value> {
    let source_name = args["source"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source is required"))?;
    let source_type = args["source_type"].as_str().unwrap_or("generic");
    let target_name = args["target"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("target is required"))?;
    let target_type = args["target_type"].as_str().unwrap_or("generic");
    let relation_type = args["relation"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("relation is required"))?;
    let confidence = args["confidence"].as_f64().unwrap_or(1.0);

    let conn = db::open()?;

    // Auto-create entities if they don't exist
    let source = db::graph::upsert_entity(&conn, source_name, source_type, None)?;
    let target = db::graph::upsert_entity(&conn, target_name, target_type, None)?;

    let relation = db::graph::upsert_relation(
        &conn,
        &source.id,
        &target.id,
        relation_type,
        confidence,
        None,
        None,
    )?;

    { #[cfg(feature = "pro")] crate::webhooks::fire("relation.created", json!({ "relation": relation, "source": source, "target": target })); }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Relation: {} ({}) —[{}]→ {} ({}) [confidence: {:.1}, id: {}]",
                source.name, source.entity_type,
                relation.relation_type,
                target.name, target.entity_type,
                relation.confidence, relation.id
            )
        }]
    }))
}

async fn handle_get_relations(args: &Value) -> Result<Value> {
    let entity_name = args["entity"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("entity is required"))?;
    let entity_type = args["entity_type"].as_str();
    let relation_type = args["relation_type"].as_str();
    let direction = args["direction"].as_str();

    let conn = db::open()?;

    // Find entity by name
    let entities = db::graph::find_entity(&conn, entity_name, entity_type)?;
    if entities.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Entity '{}' not found.", entity_name) }]
        }));
    }

    let entity = &entities[0];
    let dir = match direction {
        Some("outgoing") => Some("outgoing"),
        Some("incoming") => Some("incoming"),
        _ => None,
    };
    let relations = db::graph::get_relations(&conn, &entity.id, relation_type, dir)?;

    if relations.is_empty() {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("No relations found for '{}' ({}).", entity.name, entity.entity_type)
            }]
        }));
    }

    let mut text = format!("Relations for '{}' ({}):\n\n", entity.name, entity.entity_type);
    for (rel, source, target) in &relations {
        text.push_str(&format!(
            "- {} ({}) —[{}]→ {} ({})  [confidence: {:.1}, id: {}]\n",
            source.name, source.entity_type,
            rel.relation_type,
            target.name, target.entity_type,
            rel.confidence, rel.id
        ));
    }

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn handle_traverse(args: &Value) -> Result<Value> {
    let entity_name = args["entity"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("entity is required"))?;
    let entity_type = args["entity_type"].as_str();
    let max_depth = args["max_depth"].as_u64().unwrap_or(2) as usize;
    let relation_type = args["relation_type"].as_str();
    let min_confidence = args["min_confidence"].as_f64().unwrap_or(0.0);

    let conn = db::open()?;

    let entities = db::graph::find_entity(&conn, entity_name, entity_type)?;
    if entities.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Entity '{}' not found.", entity_name) }]
        }));
    }

    let entity = &entities[0];
    let nodes = db::graph::traverse(&conn, &entity.id, max_depth, relation_type, min_confidence)?;

    if nodes.len() <= 1 {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("No connections found from '{}' within {} hops.", entity.name, max_depth)
            }]
        }));
    }

    let mut text = format!(
        "Graph traversal from '{}' ({}) — {} nodes reached, max {} hops:\n\n",
        entity.name, entity.entity_type, nodes.len(), max_depth
    );

    for node in &nodes {
        let indent = "  ".repeat(node.depth);
        let path_str = if node.path.is_empty() {
            "(start)".to_string()
        } else {
            node.path
                .iter()
                .map(|e| format!("—[{}]→", e.relation_type))
                .collect::<Vec<_>>()
                .join(" ")
        };
        text.push_str(&format!(
            "{}{} ({}) — depth {} {}\n",
            indent, node.entity.name, node.entity.entity_type, node.depth, path_str
        ));
    }

    // Build structured JSON for programmatic use
    let json_nodes: Vec<Value> = nodes.iter().map(|n| {
        let incoming = if let Some(edge) = n.path.last() {
            json!({
                "type": edge.relation_type,
                "from": edge.from_entity,
                "confidence": edge.confidence,
            })
        } else {
            Value::Null
        };
        json!({
            "name": n.entity.name,
            "type": n.entity.entity_type,
            "id": n.entity.id,
            "depth": n.depth,
            "incoming_relation": incoming,
        })
    }).collect();

    let structured = json!({
        "nodes": json_nodes,
        "total": nodes.len(),
        "max_depth": max_depth,
    });

    Ok(json!({
        "content": [
            { "type": "text", "text": text },
            { "type": "text", "text": structured.to_string() }
        ]
    }))
}

async fn handle_list_entities(args: &Value) -> Result<Value> {
    let entity_type = args["type"].as_str();
    let query = args["query"].as_str();
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;

    let conn = db::open()?;

    let entities = if let Some(q) = query {
        db::graph::search_entities(&conn, q, entity_type, limit)?
    } else {
        db::graph::list_entities(&conn, entity_type, limit, 0)?
    };

    if entities.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No entities found." }]
        }));
    }

    let entity_count = db::graph::count_entities(&conn)?;
    let relation_count = db::graph::count_relations(&conn)?;

    let mut text = format!("Knowledge graph: {} entities, {} relations\n\n", entity_count, relation_count);
    for e in &entities {
        text.push_str(&format!("- {} ({}) [id: {}]\n", e.name, e.entity_type, e.id));
    }

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn handle_delete_entity(args: &Value) -> Result<Value> {
    let entity_name = args["entity"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("entity is required"))?;
    let entity_type = args["entity_type"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("entity_type is required"))?;

    let conn = db::open()?;

    let entities = db::graph::find_entity(&conn, entity_name, Some(entity_type))?;
    if entities.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("Entity '{}' ({}) not found.", entity_name, entity_type) }],
            "isError": true
        }));
    }

    let entity = &entities[0];
    db::graph::delete_entity(&conn, &entity.id)?;

    { #[cfg(feature = "pro")] crate::webhooks::fire("entity.deleted", json!({ "entity_id": entity.id, "name": entity.name, "type": entity.entity_type })); }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Deleted entity '{}' ({}) and all its relations.", entity.name, entity.entity_type)
        }]
    }))
}

async fn handle_delete_relation(args: &Value) -> Result<Value> {
    let id = args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("id is required"))?;

    let conn = db::open()?;
    let deleted = db::graph::delete_relation(&conn, id)?;

    if deleted {
        { #[cfg(feature = "pro")] crate::webhooks::fire("relation.deleted", json!({ "relation_id": id })); }
        Ok(json!({
            "content": [{ "type": "text", "text": format!("Deleted relation {id}.") }]
        }))
    } else {
        Ok(json!({
            "content": [{ "type": "text", "text": format!("Relation {id} not found.") }],
            "isError": true
        }))
    }
}

// ── Webhook handler (Standard + Pro tier) ────────────────────

#[cfg(feature = "pro")]
async fn handle_manage_webhooks(args: &Value) -> Result<Value> {
    let action = args["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("action is required"))?;

    let conn = db::open()?;

    match action {
        "list" => {
            let hooks = db::webhooks::list(&conn)?;
            if hooks.is_empty() {
                return Ok(json!({
                    "content": [{ "type": "text", "text": "No webhooks configured." }]
                }));
            }
            let mut text = String::from("Webhooks:\n\n");
            for h in &hooks {
                text.push_str(&format!(
                    "- [{}] {} → {} (events: {}) {}\n",
                    h.id, if h.enabled { "✓" } else { "✗" },
                    h.url,
                    h.events.join(", "),
                    if h.secret.is_some() { "[signed]" } else { "" }
                ));
            }
            Ok(json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        "create" => {
            let url = args["url"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("url is required for create"))?;
            let events: Vec<String> = args["events"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("events array is required for create"))?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            let secret = args["secret"].as_str();

            let hook = db::webhooks::create(&conn, url, &events, secret)?;
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Webhook created: {} → {} (events: {}) [id: {}]",
                        if hook.enabled { "✓" } else { "✗" },
                        hook.url, hook.events.join(", "), hook.id)
                }]
            }))
        }
        "delete" => {
            let id = args["id"].as_str()
                .ok_or_else(|| anyhow::anyhow!("id is required for delete"))?;
            let deleted = db::webhooks::delete(&conn, id)?;
            let msg = if deleted { format!("Deleted webhook {id}.") } else { format!("Webhook {id} not found.") };
            Ok(json!({ "content": [{ "type": "text", "text": msg }] }))
        }
        "enable" => {
            let id = args["id"].as_str()
                .ok_or_else(|| anyhow::anyhow!("id is required for enable"))?;
            db::webhooks::update_enabled(&conn, id, true)?;
            Ok(json!({ "content": [{ "type": "text", "text": format!("Webhook {id} enabled.") }] }))
        }
        "disable" => {
            let id = args["id"].as_str()
                .ok_or_else(|| anyhow::anyhow!("id is required for disable"))?;
            db::webhooks::update_enabled(&conn, id, false)?;
            Ok(json!({ "content": [{ "type": "text", "text": format!("Webhook {id} disabled.") }] }))
        }
        _ => Ok(json!({
            "content": [{ "type": "text", "text": format!("Unknown webhook action: {action}") }],
            "isError": true
        })),
    }
}

// ── Maintenance + consolidation handlers (Pro tier) ─────────

#[cfg(feature = "pro")]
async fn handle_maintenance(cfg: &Config, args: &Value) -> Result<Value> {
    if !cfg.feature_enabled("consolidation") {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Maintenance workflows require Pro tier. Upgrade at https://ctxovrflw.dev/pricing" }],
            "isError": true
        }));
    }

    let action = args["action"].as_str().unwrap_or("");
    match action {
        "run_consolidation_now" => {
            let report = crate::maintenance::run_consolidation_pass()?;
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!(
                        "Consolidation pass complete: scanned {} subjects / {} memories, removed {} exact duplicates.",
                        report.subjects_scanned,
                        report.memories_scanned,
                        report.duplicates_removed
                    )
                }]
            }))
        }
        "openclaw_schedule_hint" => {
            let home = std::env::var("HOME").unwrap_or_default();
            let openclaw_workspace_exists = std::path::Path::new(&home).join(".openclaw/workspace").exists();
            let text = if openclaw_workspace_exists {
                "OpenClaw workspace detected. Recommended autonomous workflow:\n\n1) Create a recurring OpenClaw cron job (isolated agentTurn) every 6-12h.\n2) Agent turn prompt should call: maintenance(action=run_consolidation_now), then consolidate(subject=...) for top noisy subjects, then update_memory/forget for cleanup.\n3) Keep delivery=announce so you get concise run summaries.\n\nSuggested agent-turn prompt:\n\"Run memory maintenance for ctxovrflw: execute maintenance run_consolidation_now, then consolidate on top 3 noisy subjects, merge obvious duplicates with update_memory, remove stale duplicates with forget, and summarize changes.\""
            } else {
                "OpenClaw workspace not detected on this machine. Use maintenance(action=run_consolidation_now) from any connected agent after major recall sessions, or schedule equivalent periodic tasks in your orchestration platform."
            };

            Ok(json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        _ => Ok(json!({
            "content": [{ "type": "text", "text": "Unknown action. Use: run_consolidation_now or openclaw_schedule_hint" }],
            "isError": true
        })),
    }
}

#[cfg(feature = "pro")]
async fn handle_consolidate(cfg: &Config, args: &Value) -> Result<Value> {
    if !cfg.feature_enabled("consolidation") {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Consolidation requires Pro tier. Upgrade at https://ctxovrflw.dev/pricing" }],
            "isError": true
        }));
    }

    let subject = args["subject"].as_str();
    let topic = args["topic"].as_str();

    if subject.is_none() && topic.is_none() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Provide 'subject' or 'topic' to find candidates for consolidation." }],
            "isError": true
        }));
    }

    let conn = db::open()?;
    let mut candidates: Vec<db::memories::Memory> = Vec::new();

    // Get by subject
    if let Some(subj) = subject {
        candidates.extend(db::search::by_subject(&conn, subj, 50)?);
    }

    // Get by topic (semantic search)
    if let Some(q) = topic {
        if let Ok(emb_arc) = crate::embed::get_or_init() { let mut embedder = emb_arc.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(embedding) = embedder.embed(q) {
                let sem = db::search::semantic_search(&conn, &embedding, 30).unwrap_or_default();
                for (mem, _score) in sem {
                    if !candidates.iter().any(|m| m.id == mem.id) {
                        candidates.push(mem);
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No memories found for consolidation." }]
        }));
    }

    // Group by approximate similarity (same subject, overlapping tags)
    let mut text = format!("Found {} candidate memories for consolidation:\n\n", candidates.len());
    for mem in &candidates {
        let tags_str = if mem.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", mem.tags.join(", "))
        };
        text.push_str(&format!(
            "- [{}] ({}) {}{}{}\n",
            mem.id, mem.memory_type, mem.content,
            mem.subject.as_deref().map(|s| format!(" {{subject: {s}}}")).unwrap_or_default(),
            tags_str,
        ));
    }
    text.push_str("\nReview these memories. Use update_memory to merge content and forget to remove duplicates.");

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

/// Auto-extract entities from a memory into the knowledge graph.
/// Best-effort: errors are silently ignored.
fn auto_extract_graph_from_memory(conn: &rusqlite::Connection, memory: &db::memories::Memory) -> Result<()> {
    use db::graph::upsert_entity;

    // 1. Extract entity from subject field
    if let Some(subject) = &memory.subject {
        let (entity_type, entity_name) = if let Some((t, n)) = subject.split_once(':') {
            (t.to_string(), n.to_string())
        } else {
            ("generic".to_string(), subject.clone())
        };
        let entity = upsert_entity(conn, &entity_name, &entity_type, None)?;

        // Create a self-referencing "memory" entity and link via mentioned_in
        let mem_entity = upsert_entity(conn, &memory.id, "memory", None)?;
        let _ = db::graph::upsert_relation(
            conn,
            &entity.id,
            &mem_entity.id,
            "mentioned_in",
            1.0,
            Some(&memory.id),
            None,
        );
    }

    // 2. Extract entities from namespaced tags (e.g., lang:rust, infra:aws)
    for tag in &memory.tags {
        if let Some((ns, value)) = tag.split_once(':') {
            let _ = upsert_entity(conn, value, ns, None);
        }
    }

    Ok(())
}
