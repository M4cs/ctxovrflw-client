pub mod tools;
pub mod transport;
pub mod sse;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;

// ── Shared JSON-RPC types ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

// ── Shared message handler (used by both stdio and SSE) ──────

pub async fn handle_message(cfg: &Config, raw: &str) -> Result<Option<String>> {
    let request: JsonRpcRequest = serde_json::from_str(raw)?;

    let response = match request.method.as_str() {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "ctxovrflw",
                    "version": env!("CARGO_PKG_VERSION"),
                    "description": "Universal AI context layer — shared memory across all your AI tools. What you tell one tool, every tool knows."
                },
                "instructions": "ctxovrflw is a shared memory layer. Use 'remember' to store important context (preferences, decisions, facts, project details) and 'recall' before answering questions that might benefit from prior context. Memories persist across sessions and are shared with other AI tools the user has connected."
            });
            Some(make_response(request.id, Some(result), None))
        }
        "notifications/initialized" => None,
        "tools/list" => {
            let tool_list = tools::list_tools(cfg);
            Some(make_response(
                request.id,
                Some(serde_json::json!({ "tools": tool_list })),
                None,
            ))
        }
        "tools/call" => {
            let params = request.params.unwrap_or(Value::Null);
            let result = tools::call_tool(cfg, &params).await?;
            Some(make_response(request.id, Some(result), None))
        }
        "resources/list" => {
            Some(make_response(request.id, Some(serde_json::json!({ "resources": [] })), None))
        }
        "resources/templates/list" => {
            Some(make_response(request.id, Some(serde_json::json!({ "resourceTemplates": [] })), None))
        }
        "prompts/list" => {
            Some(make_response(request.id, Some(serde_json::json!({
                "prompts": [{
                    "name": "ctxovrflw-context",
                    "description": "Get instructions on how to use ctxovrflw shared memory effectively",
                    "arguments": []
                }]
            })), None))
        }
        "prompts/get" => {
            let name = request.params
                .as_ref()
                .and_then(|p| p["name"].as_str())
                .unwrap_or("");
            match name {
                "ctxovrflw-context" => {
                    Some(make_response(request.id, Some(serde_json::json!({
                        "description": "Instructions for using ctxovrflw shared memory",
                        "messages": [{
                            "role": "user",
                            "content": {
                                "type": "text",
                                "text": concat!(
                                    "You have access to ctxovrflw — a shared memory layer that persists across sessions and is shared between ALL connected AI tools (Cursor, Claude Code, Cline, VS Code, etc.).\n\n",
                                    "## When to use RECALL:\n",
                                    "- At the START of every conversation, recall general context about the user and project\n",
                                    "- Before answering questions about preferences, past decisions, or project setup\n",
                                    "- When the user says \"do you remember\" or \"what did I say about\"\n",
                                    "- When you need context that might have been shared in another tool\n\n",
                                    "## When to use REMEMBER:\n",
                                    "- When the user shares a preference (\"I prefer X over Y\")\n",
                                    "- When a decision is made (\"We're going with Rust\")\n",
                                    "- When important project context comes up (API endpoints, deploy targets, tech stack)\n",
                                    "- When the user explicitly asks you to remember something\n",
                                    "- When you learn something important about the user or project\n\n",
                                    "## Best practices:\n",
                                    "- Store ATOMIC facts — one concept per memory, not paragraphs\n",
                                    "- Use descriptive tags with namespace:value format (e.g., project:myapp, lang:rust)\n",
                                    "- Choose the right type: preference, semantic (facts), episodic (events), procedural (how-to)\n",
                                    "- Use natural language for recall queries — semantic search understands meaning, not just keywords\n",
                                    "- Don't store sensitive data (passwords, tokens, keys)\n\n",
                                    "## The magic:\n",
                                    "Memories are shared across tools. If the user tells Cursor their deploy target is Fly.io, you can recall that here. This is the key value — cross-tool context continuity."
                                )
                            }
                        }]
                    })), None))
                }
                _ => {
                    Some(make_response(request.id, None, Some(JsonRpcError {
                        code: -32602,
                        message: format!("Unknown prompt: {name}"),
                    })))
                }
            }
        }
        _ => Some(make_response(
            request.id,
            None,
            Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
            }),
        )),
    };

    match response {
        Some(resp) => Ok(Some(serde_json::to_string(&resp)?)),
        None => Ok(None),
    }
}

pub fn make_response(
    id: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.unwrap_or(Value::Null),
        result,
        error,
    }
}

// ── Stdio transport ──────────────────────────────────────────

pub async fn serve_stdio(cfg: &Config) -> Result<()> {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::stdout();

    // Debug log to file (won't interfere with stdio protocol)
    let log_path = Config::data_dir().ok().map(|d| d.join("mcp-debug.log"));
    let log = |msg: &str| {
        if let Some(ref path) = log_path {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "[{}] {}", chrono::Utc::now().format("%H:%M:%S%.3f"), msg)
                });
        }
    };

    log("MCP stdio server starting");

    loop {
        match transport::read_message(&mut reader).await {
            Ok(Some(msg)) => {
                log(&format!("← {}", &msg[..msg.len().min(200)]));
                let response = handle_message(cfg, &msg).await?;
                if let Some(resp) = response {
                    log(&format!("→ {}", &resp[..resp.len().min(200)]));
                    transport::write_message(&mut writer, &resp).await?;
                } else {
                    log("→ (no response — notification)");
                }
            }
            Ok(None) => {
                log("EOF — shutting down");
                break;
            }
            Err(e) => {
                log(&format!("ERROR: {e}"));
                eprintln!("MCP stdio error: {e}");
                break;
            }
        }
    }

    Ok(())
}
