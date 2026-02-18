# ctxovrflw Memory Plugin for OpenClaw

Replace OpenClaw's built-in memory with **ctxovrflw** — a local-first, privacy-focused AI memory layer with semantic search, knowledge graph, and cross-tool recall.

## What It Does

- **Replaces `memory_search` / `memory_get`** with ctxovrflw's semantic search (hybrid keyword + vector)
- **Adds `memory_store`** to persist important context across sessions
- **Auto-recall**: Automatically injects relevant memories into agent context before each turn
- **Auto-capture**: Optionally stores important user messages as memories
- **Knowledge graph**: Graph-boosted recall surfaces related entities and connections
- **Cross-tool**: Memories stored by Cursor, Claude Code, Cline, or any MCP-connected tool are available in OpenClaw
- **`/ctxovrflw` command**: Quick status check without invoking the LLM

## Prerequisites

1. **Install ctxovrflw**: `curl -fsSL https://ctxovrflw.dev/install.sh | bash`
2. **Run init**: `ctxovrflw init`
3. **Start daemon**: `ctxovrflw start`

## Install

```bash
openclaw plugins install @ctxovrflw/memory-ctxovrflw
```

> **Tip:** `ctxovrflw init` detects OpenClaw automatically and offers three integration paths via an interactive TUI:
> 1. **Plugin + Skill + Agent Rules** (recommended) — full integration
> 2. **Plugin only** — just native memory tools
> 3. **Skill + Agent Rules only** — CLI-based access without the plugin

## Configure

Add to your OpenClaw config:

```json5
{
  plugins: {
    slots: {
      memory: "memory-ctxovrflw"  // Replace built-in memory
    },
    entries: {
      "memory-ctxovrflw": {
        enabled: true,
        config: {
          authToken: "<from ~/.ctxovrflw/config.toml>",
          // Optional:
          daemonUrl: "http://127.0.0.1:7437",  // default
          agentId: "openclaw",                   // default
          autoRecall: true,                      // default
          autoRecallMode: "smart",              // smart|always (default: smart)
          autoCapture: false,                    // default
          recallLimit: 5,                        // default
          preflightRecallLimit: 8,               // default (high-impact preflight turns)
          recallMinScore: 0.3,                   // default
          telemetryEnabled: true,                // default (structured auto-recall telemetry)
          captureMaxChars: 500,                  // default
        }
      }
    }
  }
}
```

Then restart the gateway:

```bash
openclaw gateway restart
```

## How It Works

### Auto-Recall (default: on)

Before each agent turn, the plugin searches ctxovrflw for relevant memories and injects them as context. In `autoRecallMode: "smart"`, high-impact prompts (deploy/release/auth/security/delete/public side effects) trigger a broader preflight recall sweep. Use `"always"` to apply broad recall every turn.

### Auto-Capture (default: off)

After each agent turn, the plugin scans user messages for important content (preferences, decisions, facts) and stores them automatically. Enable with `autoCapture: true`.

### Tool-Based Access

The agent can also use tools explicitly:

- `memory_search` — Semantic search with optional subject filter
- `memory_store` — Store new memories with type, tags, subject
- `memory_forget` — Delete a memory by ID
- `memory_pin` — Pin/prioritize a memory (supports policy/workflow tags)
- `memory_unpin` — Remove pin/policy/workflow priority tags
- `memory_status` — Check daemon status, memory count, tier

### CLI

```bash
openclaw memory status    # Daemon status
openclaw memory search "deployment preferences"
openclaw memory store "Max prefers Railway" --type preference
openclaw memory subjects  # List all subjects
```

## vs Built-in Memory

| Feature | memory-core | memory-lancedb | memory-ctxovrflw |
|---|---|---|---|
| Storage | Markdown files | LanceDB + OpenAI | SQLite + local ONNX |
| Embeddings | None | OpenAI API ($) | Local ONNX (free) |
| Privacy | Files on disk | API calls to OpenAI | Everything local |
| Cross-tool | No | No | Yes (MCP server) |
| Knowledge graph | No | No | Yes (Pro) |
| Hybrid search | No | Vector only | Semantic + keyword |
| Auto-recall | No | Yes | Yes |
| Auto-capture | No | Yes | Yes |
| Cloud sync | No | No | Yes (E2E encrypted) |

## License

MIT
