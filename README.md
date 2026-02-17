# ctxovrflw

**Shared memory for every AI agent.**

ctxovrflw is a local-first daemon that gives every AI agent on your machine a shared, persistent memory layer. It speaks [MCP (Model Context Protocol)](https://modelcontextprotocol.io) — the open standard for AI tool integrations.

What you tell Cursor, Claude knows. What Claude learns, Copilot remembers.

## Install

```bash
curl -fsSL ctxovrflw.dev/install.sh | sh
ctxovrflw init
```

**Windows (PowerShell):**
```powershell
irm ctxovrflw.dev/install.ps1 | iex
```

## How It Works

1. **Install** — single binary, one command
2. **Connect** — auto-detects 19+ AI agents and configures MCP
3. **Remember** — any agent stores memories, every agent recalls them

```
┌──────────┐     ┌──────────┐     ┌──────────┐
│  Cursor  │     │  Claude  │     │  Cline   │
│          │     │   Code   │     │          │
└────┬─────┘     └────┬─────┘     └────┬─────┘
     │                │                │
     └────────┬───────┘────────┬───────┘
              │                │
         ┌────▼────────────────▼────┐
         │       ctxovrflw          │
         │   local daemon (MCP)     │
         │                          │
         │  SQLite + ONNX embeddings│
         │  semantic search         │
         │  persistent memory       │
         └──────────────────────────┘
```

## Supported Agents

Claude Code · Cursor · Cline · Windsurf · Claude Desktop · Copilot CLI · Gemini CLI · OpenClaw · Roo Code · Continue · Codex CLI · Goose · Amp · Kiro · Trae · OpenCode · Factory · Antigravity · Kilo Code

Any MCP-compatible agent works out of the box.

## Features

- **Semantic search** — finds memories by meaning, not just keywords (local ONNX embeddings)
- **MCP native** — speaks Model Context Protocol, zero custom integration
- **Memory expiry** — set TTL on temporary context (`"ttl": "24h"`)
- **E2E encrypted sync** — optional cross-device sync, zero-knowledge encryption
- **Sub-millisecond queries** — SQLite + sqlite-vec, no network latency
- **Single binary** — written in Rust, ~15MB, runs as a lightweight daemon
- **Privacy first** — runs entirely locally by default, cloud is opt-in

## MCP Tools

| Tool | Description |
|------|-------------|
| `remember` | Store a memory with optional tags, subject, type, and TTL |
| `recall` | Semantic search across all memories |
| `update_memory` | Update content, tags, subject, or expiry on existing memories |
| `forget` | Delete a memory (with dry-run preview) |
| `subjects` | List all known entities and memory counts |
| `consolidate` | Deduplicate and clean up memories for a subject (Pro) |
| `context` | Synthesized context briefing (Pro) |
| `status` | Check tier, usage, and feature availability |
| `manage_webhooks` | Create, list, and delete webhook subscriptions |
| **Knowledge Graph (Pro)** | |
| `add_entity` | Add a named entity with type and metadata |
| `add_relation` | Create a relationship between two entities |
| `traverse` | Walk the graph from an entity up to N hops |
| `get_relations` | Get direct relationships for an entity |
| `search_entities` | Search entities by name, type, or metadata |

## CLI

```bash
ctxovrflw init              # First-time setup (interactive TUI)
ctxovrflw start             # Start the daemon
ctxovrflw status            # Check daemon status
ctxovrflw remember "text"   # Store a memory
ctxovrflw recall "query"    # Search memories
ctxovrflw memories          # Interactive memory browser (TUI)
ctxovrflw model             # Embedding model manager (TUI)
ctxovrflw model list        # List available embedding models
ctxovrflw model current     # Show active model
ctxovrflw model switch <n>  # Switch embedding model (hotswap)
ctxovrflw graph build       # Build knowledge graph from memories (Pro)
ctxovrflw graph stats       # Knowledge graph statistics (Pro)
ctxovrflw login             # Authenticate for cloud sync
ctxovrflw account           # View cloud account status
ctxovrflw update            # Self-update (with SHA256 verification)
ctxovrflw version           # Check current version
```

## Architecture

```
~/.ctxovrflw/
├── config.toml          # Configuration
├── memories.db          # SQLite database (memories + FTS5 + sqlite-vec)
└── models/
    └── <model-name>/    # Per-model subdirectory
        ├── model.onnx   # Quantized ONNX embedding model
        └── tokenizer.json
```

- **Storage:** SQLite with FTS5 (keyword search) and sqlite-vec (vector search)
- **Search:** Hybrid semantic + FTS5 keyword search with Reciprocal Rank Fusion (RRF)
- **Embeddings:** ONNX Runtime with 12 available models — hotswap via `ctxovrflw model switch`
  - Default: `all-MiniLM-L6-v2` | Also available: `bge-small-en-v1.5`, `gte-small`, `e5-small-v2`, `jina-v2-small-en`, `bge-base-en-v1.5`, `gte-base`, `jina-v2-base-en`, `snowflake-arctic-embed-m-v2.0`, `multilingual-e5-small`, `multilingual-e5-base`, `bge-m3`
- **Transport:** MCP over SSE (Server-Sent Events) at `http://127.0.0.1:7437/mcp/sse`
- **HTTP API:** REST API at `http://127.0.0.1:7437/v1/`
- **Platforms:** linux-x64, linux-arm64, darwin-x64, darwin-arm64, windows-x64
- **Updates:** SHA256 binary verification on self-update

## OpenClaw Integration

ctxovrflw integrates with [OpenClaw](https://openclaw.dev) via the `@ctxovrflw/memory-ctxovrflw` plugin:

```bash
openclaw plugins install @ctxovrflw/memory-ctxovrflw
```

The plugin replaces OpenClaw's built-in memory with ctxovrflw's semantic search, providing `memory_search`, `memory_store`, `memory_forget`, and `memory_status` tools natively. `ctxovrflw init` detects OpenClaw and offers to install the plugin automatically.

## Tiers

| | Free | Standard | Pro |
|---|---|---|---|
| **Price** | $0 | $10/mo | $20/mo |
| **Memories** | 100 | Unlimited | Unlimited |
| **Devices** | 1 | 3 | Unlimited |
| **Semantic search** | ✓ | ✓ | ✓ |
| **Hybrid search (RRF)** | ✓ | ✓ | ✓ (boosted) |
| **Cloud sync** | — | ✓ (E2E encrypted) | ✓ (E2E encrypted) |
| **Knowledge graph** | — | — | ✓ |
| **Consolidation** | — | — | ✓ |
| **Context synthesis** | — | — | ✓ |

## Source Available

This source code is published for **transparency and trust**. ctxovrflw runs as a daemon on your machine with access to your AI tools — you deserve to see exactly what it does.

You are free to read, inspect, and audit the code. Official binaries are the supported way to run ctxovrflw.

See [LICENSE.md](LICENSE.md) for full terms.

## Links

- **Website:** [ctxovrflw.dev](https://ctxovrflw.dev)
- **Docs:** [docs.ctxovrflw.dev](https://docs.ctxovrflw.dev)
