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
| `context` | Synthesized context briefing (Pro) |
| `status` | Check tier, usage, and feature availability |

## CLI

```bash
ctxovrflw init              # First-time setup
ctxovrflw start             # Start the daemon
ctxovrflw status            # Check daemon status
ctxovrflw remember "text"   # Store a memory
ctxovrflw recall "query"    # Search memories
ctxovrflw login             # Authenticate for cloud sync
ctxovrflw account           # View cloud account status
ctxovrflw update            # Self-update to latest version
ctxovrflw version           # Check current version
```

## Architecture

```
~/.ctxovrflw/
├── config.toml          # Configuration
├── memories.db          # SQLite database (memories + FTS5 + sqlite-vec)
└── models/
    ├── all-MiniLM-L6-v2-q8.onnx   # Embedding model (~23MB)
    └── tokenizer.json
```

- **Storage:** SQLite with FTS5 (keyword search) and sqlite-vec (vector search)
- **Embeddings:** ONNX Runtime with `all-MiniLM-L6-v2` quantized model, loaded dynamically
- **Transport:** MCP over SSE (Server-Sent Events) at `http://127.0.0.1:7437/mcp/sse`
- **HTTP API:** REST API at `http://127.0.0.1:7437/v1/`

## Tiers

| | Free | Standard | Pro |
|---|---|---|---|
| **Price** | $0 | $10/mo | $20/mo |
| **Memories** | 100 | Unlimited | Unlimited |
| **Devices** | 1 | 3 | Unlimited |
| **Semantic search** | ✓ | ✓ | ✓ |
| **Cloud sync** | — | ✓ (E2E encrypted) | ✓ (E2E encrypted) |
| **Context synthesis** | — | — | ✓ |

## Building from Source

```bash
# Prerequisites: Rust 1.70+, ONNX Runtime 1.23.0

# Clone
git clone https://github.com/M4cs/ctxovrflw-client.git
cd ctxovrflw-client

# Build without ONNX (keyword search only)
cargo build --release

# Build with ONNX (semantic search)
export ORT_DYLIB_PATH=/path/to/libonnxruntime.so
cargo build --release --features onnx
```

## License

Business Source License 1.1 — see [LICENSE](LICENSE).

You can read, audit, and build the source for personal use. You cannot use it to create a competing commercial product or service. After 4 years, the code converts to Apache 2.0.

## Links

- **Website:** [ctxovrflw.dev](https://ctxovrflw.dev)
- **Docs:** [docs.ctxovrflw.dev](https://docs.ctxovrflw.dev)
