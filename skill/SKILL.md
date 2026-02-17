---
name: ctxovrflw
description: Shared AI memory layer — recall context from past sessions and other AI tools, store decisions and preferences automatically. Use when you need context aboutuser preferences, past decisions, project setup, or any information that may have been discussed in another AI tool. Also use proactively to store important context as it comes up. Cross-tool: what Cursor stores, Claude Code can recall.
metadata:
  author: ctxovrflw
  version: "0.5.1"
  website: https://ctxovrflw.dev
compatibility: Requires ctxovrflw daemon running locally (MCP server on port 7437)
---

# ctxovrflw — Shared Memory

You have access to **ctxovrflw**, a shared memory layer that persists across sessions and is
accessible by every AI tool the user runs (Cursor, Claude Code, Cline, Windsurf, etc.).

You interact with it via the MCP tools: `remember`, `recall`, `forget`, `status`, `consolidate`,
`add_entity`, `add_relation`, `traverse`, `get_relations`, `subjects`, and `manage_webhooks`.

## ⚠️ Prerequisites — ctxovrflw Must Be Installed & Running

This skill requires the **ctxovrflw daemon** running locally. If it's not installed or not running,
**none of the MCP tools will work.**

### Check if ctxovrflw installed

```bash
ctxovrflw --version
```

### Check if running

```bash
ctxovrflw status

# If running, returns response like this:
ctxovrflw v0.5.1

Version:         v0.5.1
Daemon:          running (systemd) ✓
  REST API:      http://localhost:7437/v1/
  MCP SSE:       http://localhost:7437/mcp/sse
Service:         installed

Tier:            Pro
Memories:        64/unlimited
Semantic search: enabled
Cloud sync:      enabled

Data dir:        /home/user/.ctxovrflw

# If not running, returns response like this:
ctxovrflw v0.5.1

Version:         v0.5.1
Daemon:          stopped # Notice the "stopped" status
Service:         installed

Tier:            Pro
Memories:        64/unlimited
Semantic search: enabled
Cloud sync:      enabled

Data dir:        /home/max/.ctxovrflw

💡 Start daemon: ctxovrflw start
```

### Install ctxovrflw

```bash
# Linux / macOS
curl -fsSL https://ctxovrflw.dev/install.sh | sh

# Windows (PowerShell)
irm ctxovrflw.dev/install.ps1 | iex
```

This installs the daemon binary, ONNX runtime (for semantic search), and the embedding model.
Supports Windows (x64), Linux (x64, arm64), and macOS (x64, arm64).

After install:

```bash
ctxovrflw init -y  # Non-interactive setup — auto-configures everything, starts daemon
ctxovrflw login    # Authenticate with cloud (optional, needed for sync)
```

The `-y` / `--yes` flag runs init non-interactively: creates config, downloads the model,
detects and configures all AI tools, installs agent rules, sets up the systemd service,
and starts the daemon — all without prompting. Ideal for agents and scripts.

For interactive setup with a TUI, omit the flag:

```bash
ctxovrflw init     # Interactive TUI with step-by-step wizard
```

The daemon runs on `http://127.0.0.1:7437` (localhost only) and exposes an MCP SSE endpoint at
`/mcp/sse` for AI tool integrations.

### Update ctxovrflw

```bash
ctxovrflw update   # Downloads and installs the latest version
```

## Tiers & Pricing

| Feature                    | Free ($0) | Standard ($10/mo) | Pro ($20/mo) |
| -------------------------- | --------- | ----------------- | ------------ |
| Memories                   | 100       | Unlimited         | Unlimited    |
| Devices                    | 1         | 3                 | Unlimited    |
| Semantic search            | ✅        | ✅                | ✅           |
| Cloud sync (E2E encrypted) | ❌        | ✅                | ✅           |
| Context synthesis          | ❌        | ❌                | ✅           |
| Consolidation              | ❌        | ❌                | ✅           |
| Knowledge graph            | ❌        | ❌                | ✅           |
| Webhooks                   | ✅        | ✅                | ✅           |

### How to Subscribe

**For humans (recommended):**
Direct them to **https://ctxovrflw.dev/pricing** to pick a plan. Stripe checkout handles payment.
After subscribing, run `ctxovrflw account` to sync the tier locally.

**For agents (x402 crypto payments):**
Agents can subscribe programmatically via USDC on Base:

```
POST https://api.ctxovrflw.dev/v1/agent/subscribe/standard
POST https://api.ctxovrflw.dev/v1/agent/subscribe/pro
```

These endpoints use the x402 payment protocol. The agent pays with USDC and receives an API key.
Subscriptions last 30 days and auto-downgrade on expiry.

**Check current tier:**
Use the `status` MCP tool, or:

```bash
ctxovrflw account   # Shows tier, usage, sync status
ctxovrflw status    # Quick daemon status
```

## Core Principle

**What you store, every other AI tool can recall.** Context shared in Cursor is available in
Claude Code. Decisions made in Cline are visible to Windsurf. Act accordingly.

## When to RECALL (search memory)

Do this **automatically** — don't wait to be asked:

- **Start of every session**: Do a broad recall for the current project/topic to load context
- **Before answering** questions about user preferences, past decisions, project setup, or conventions
- **When the user says** "do you remember", "what did I say about", "we decided", etc.
- **When you lack context** about a project, tool, API, or workflow the user references
- **Before suggesting** a technology, pattern, or approach — check if there's a stated preference

### Examples

```
recall("project setup and conventions")
recall("deployment preferences")
recall("coding style preferences")
recall("what stack are we using")
```

## When to REMEMBER (store memory)

Do this **proactively** whenever important information comes up:

- User states a **preference** ("I prefer tabs", "use Railway not Fly.io", "always use Rust for CLIs")
- A **decision is made** ("we're going with Postgres", "the API will be REST not GraphQL")
- **Project context** is established ("the API is at api.example.com", "we use pnpm workspaces")
- **Architectural choices** ("auth uses JWT with refresh tokens", "the monorepo has 3 packages")
- User explicitly says **"remember this"** or similar
- **Endpoints and services** are shared (API URLs, service names)
- **Debugging insights** ("that error was caused by X", "the fix for Y is Z")

### Examples

```
remember("Max prefers Railway for all hosting", type: "preference", tags: ["infra:railway"])
remember("Auth API is at api.example.com/v1/auth", type: "semantic", tags: ["project:myapp", "api"])
remember("The staging deploy requires VPN access first", type: "procedural", tags: ["infra:deploy"])
```

## Subjects (Entity Tracking)

Use the `subject` field to track WHO or WHAT a memory is about:

```
remember("Prefers dark mode", subject: "user", type: "preference")
remember("Uses React + TypeScript", subject: "project:webapp", type: "semantic")
remember("Sarah handles the backend deployment", subject: "person:sarah", type: "semantic")
```

Then recall scoped to a subject:

```
recall("", subject: "user")           // everything about the user
recall("preferences", subject: "user") // user preferences only
```

Use `subjects` tool to list all known entities.

## Knowledge Graph (Pro Tier)

Build and traverse a knowledge graph of entities and relationships:

```
add_entity("ctxovrflw", type: "project", metadata: {language: "rust", repo: "M4cs/ctxovrflw"})
add_entity("railway", type: "service", metadata: {purpose: "hosting"})
add_relation("ctxovrflw", "hosted_on", "railway")
traverse("ctxovrflw", max_depth: 2)        // see what connects to ctxovrflw
get_relations("ctxovrflw")                  // direct relationships only
```

Do this silently as you work — don't announce graph operations to the user.

## Consolidation (Pro Tier)

Periodically deduplicate and clean up memories:

```
consolidate(subject: "user")  // review all memories about the user, merge duplicates
```

Use `update_memory` to merge and `forget` to remove redundant entries.

## Webhooks (All Tiers)

Subscribe to memory events:

```
manage_webhooks(action: "create", url: "https://example.com/hook", events: ["memory.created", "memory.updated"])
```

Available events: `memory.created`, `memory.updated`, `memory.deleted`, `memory.recalled`,
`entity.created`, `entity.updated`, `relation.created`, `relation.deleted`, `consolidation.completed`.

## Token Budgets

Control context window usage with `max_tokens`:

```
recall("project setup", max_tokens: 2000)  // fit within 2K tokens
```

Returns the most relevant results that fit within the budget. Use this to avoid stuffing your context window.

## Best Practices

- **Be atomic**: One fact per memory. "Max prefers tabs" not "Max told me about his preferences..."
- **Tag well**: Use `project:name`, `lang:rust`, `tool:docker`, `infra:railway` format
- **Set subjects**: Always set `subject` when the memory is clearly about a specific entity
- **Use types**: `preference` for likes/config, `semantic` for facts, `procedural` for how-to, `episodic` for events
- **Don't duplicate**: Recall first to check if you already know something before storing
- **Never store secrets**: No passwords, API keys, tokens, or private keys
- **Don't announce memory ops**: Just remember/recall silently — don't tell the user "I'll remember that"

## Recall Is Free — Use It Liberally

Lookups are **local, fast, and free** — they hit a local SQLite database with ONNX embeddings,
not an external API. There is zero cost to recalling. When in doubt, recall. Better to check and
find nothing than to miss context that exists.

**Recall aggressively:**

- At the start of every session (broad query)
- Before making any suggestion or recommendation
- When you're about to do something you've done before
- When the user mentions any project, person, tool, or concept by name
- Before writing code — check for conventions, patterns, and past decisions
- Multiple times per conversation if the topic shifts

**Don't be conservative with recall.** Five recalls that return nothing useful cost less than one
wrong answer that ignores stored context.

## Learn From Corrections

When the user **corrects you**, that's a high-signal learning moment. Always store the correction:

```
remember("User corrected: don't use X, use Y instead because Z",
         type: "preference", tags: ["correction"], subject: "user")
```

**Examples of corrections to store:**

- "No, we use pnpm not npm" → remember the package manager preference
- "That's wrong, the API is at /v2 not /v1" → remember the correct endpoint
- "Don't suggest that approach, it doesn't work because..." → remember the constraint
- "I told you before, always use..." → remember the preference AND recall first next time
- Style/formatting corrections → remember as coding conventions
- "That's not how we deploy" → remember the correct deployment process

**The pattern:** When corrected → `remember` the correction → `recall` it next time the topic comes up.
Corrections tagged with `correction` can be recalled later to avoid repeating the same mistake.

**If the user says "I already told you"** — that means you failed to recall. Immediately:

1. `recall` the topic to find what you missed
2. `remember` the correction with `tags: ["correction"]`
3. Apologize briefly and move on with the right information

## Memory Types

| Type         | Use for                           | Example                                         |
| ------------ | --------------------------------- | ----------------------------------------------- |
| `preference` | User likes, config choices, style | "Prefers Rust for backend services"             |
| `semantic`   | Facts, knowledge, project info    | "The API uses PostgreSQL with pgvector"         |
| `procedural` | How-to, steps, processes          | "To deploy: push to main, Railway auto-deploys" |
| `episodic`   | Events, things that happened      | "Migrated from Fly.io to Railway on Feb 10"     |

## Tag Conventions

Use namespaced tags for organization:

- `project:ctxovrflw` — project name
- `lang:rust` — programming language
- `infra:railway` — infrastructure/hosting
- `tool:docker` — tooling
- `api:auth` — API domain
- `decision` — architectural/business decisions
- `bug` — known issues and fixes

## Troubleshooting

| Problem                | Solution                                                        |
| ---------------------- | --------------------------------------------------------------- |
| MCP tools not working  | Check daemon: `curl http://127.0.0.1:7437/health`               |
| "Not logged in"        | Run `ctxovrflw login`                                           |
| "Memory limit reached" | Upgrade tier or `forget` old memories                           |
| "Sync PIN expired"     | Run `ctxovrflw login` to re-enter PIN                           |
| Slow semantic search   | First query loads ONNX model (~2s), subsequent queries are fast |
| Cloud sync not working | Check tier with `ctxovrflw account` — Free tier is local-only   |

## CLI Alternative (When MCP Is Unavailable)

If the MCP connection isn't working or your environment doesn't support MCP, you can use the
ctxovrflw CLI and HTTP API directly. These provide the same functionality.

### Remember (Store a Memory)

```bash
# Basic
ctxovrflw remember "Max prefers Railway for hosting"

# With type, tags, and subject
ctxovrflw remember "Auth API uses JWT with 15min access tokens" \
  --type semantic \
  --tags "project:myapp,api:auth,decision" \
  --subject "project:myapp"

# Types: semantic, episodic, procedural, preference
```

### Recall (Search Memories)

```bash
# Basic search
ctxovrflw recall "deployment preferences"

# Limit results
ctxovrflw recall "coding conventions" --limit 5
```

### Forget (Delete a Memory)

```bash
# Dry run first (see what would be deleted)
ctxovrflw forget <memory-id> --dry-run

# Actually delete
ctxovrflw forget <memory-id>
```

### Browse Memories (Interactive TUI)

```bash
ctxovrflw memories
```

Opens an interactive terminal UI where you can browse, search, view details, delete,
and see knowledge graph connections (`g` key) for any memory.

**TUI keybindings:**

- `j`/`k` or arrows — navigate
- `/` — search
- `Enter` — view details
- `d` — delete (with confirmation)
- `g` — graph view (Pro tier, shows entity relationships)
- `Space` — multi-select
- `q` — quit

### Reindex (Rebuild Embeddings)

```bash
ctxovrflw reindex
```

Re-embeds all memories with the current model. Use after switching models or if semantic search
seems degraded.

### Model Management

```bash
# List available embedding models with dimensions, size, and descriptions
ctxovrflw model list

# Show current model details
ctxovrflw model current

# Switch to a different model (downloads, re-embeds everything)
ctxovrflw model switch bge-base-en-v1.5
```

Available models:

- `all-MiniLM-L6-v2` — 384d, ~23MB. Fast, lightweight, good for English. (Default)
- `bge-small-en-v1.5` — 384d, ~33MB. Better quality, same speed. English only.
- `bge-base-en-v1.5` — 768d, ~110MB. Significant quality bump.
- `snowflake-arctic-embed-m-v2.0` — 768d, ~113MB. Multilingual, 8192 token context.
- `bge-m3` — 1024d, ~570MB. Top-tier multilingual, 100+ languages.

Switching models exports all data, recreates the database with the new vector dimensions,
reimports everything, and re-embeds all memories. No data loss.

### Knowledge Graph (CLI)

```bash
# Build graph from existing memories (extracts entities from subjects and tags)
ctxovrflw graph build

# Show graph statistics
ctxovrflw graph stats
```

### Sync & Account

```bash
ctxovrflw sync       # Manually trigger cloud sync
ctxovrflw account    # Show tier, usage, sync status
ctxovrflw login      # Authenticate for cloud features
ctxovrflw logout     # Disconnect cloud
```

### Daemon Management

```bash
ctxovrflw start              # Start the daemon
ctxovrflw start --foreground # Run in foreground (for debugging)
ctxovrflw stop               # Stop the daemon
ctxovrflw status             # Show daemon status, memory count, tier
ctxovrflw service install    # Install as systemd user service
ctxovrflw service status     # Check service status
```

### HTTP API (Alternative to CLI)

If you can make HTTP requests but can't run the CLI binary, use the REST API directly.
The daemon listens on `http://127.0.0.1:7437`. All `/v1/*` endpoints require an auth token
(found in `~/.ctxovrflw/config.toml` as `auth_token`).

```bash
TOKEN="<auth_token from config.toml>"

# Remember
curl -s -X POST "http://127.0.0.1:7437/v1/memories" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "content": "Max prefers Railway for hosting",
    "type": "preference",
    "tags": ["infra:railway"],
    "subject": "user",
    "agent_id": "my-agent"
  }'

# Recall (semantic search)
curl -s -X POST "http://127.0.0.1:7437/v1/search" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"query": "hosting preferences", "limit": 5}'

# Recall with subject filter
curl -s -X POST "http://127.0.0.1:7437/v1/search" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"query": "preferences", "subject": "user", "limit": 5}'

# Forget (delete)
curl -s -X DELETE "http://127.0.0.1:7437/v1/memories/<memory-id>" \
  -H "Authorization: Bearer $TOKEN"

# Update a memory
curl -s -X PATCH "http://127.0.0.1:7437/v1/memories/<memory-id>" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"content": "updated content", "tags": ["new-tag"]}'

# Health check (no auth needed)
curl -s http://127.0.0.1:7437/health

# Status
curl -s "http://127.0.0.1:7437/v1/status" \
  -H "Authorization: Bearer $TOKEN"
```

**Note on `agent_id`:** When storing memories via the API, include `"agent_id": "your-agent-name"`
to track which agent stored each memory. This helps with cross-agent recall and debugging.
The `agent_id` is a free-form string — use something identifiable like `"cursor"`, `"claude-code"`,
`"cline"`, or your agent's name.

### Knowledge Graph (HTTP API)

These have no CLI equivalent — use the HTTP API directly or MCP tools.

```bash
TOKEN="<auth_token from config.toml>"
BASE="http://127.0.0.1:7437"

# List unique subjects across all memories
curl -s "$BASE/v1/subjects" -H "Authorization: Bearer $TOKEN"

# Add an entity
curl -s -X POST "$BASE/v1/entities" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"name": "ctxovrflw", "entity_type": "project", "metadata": {}}'

# Get entity details
curl -s "$BASE/v1/entities/<entity-id>" \
  -H "Authorization: Bearer $TOKEN"

# Delete an entity
curl -s -X DELETE "$BASE/v1/entities/<entity-id>" \
  -H "Authorization: Bearer $TOKEN"

# Add a relation between entities
curl -s -X POST "$BASE/v1/relations" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "source_entity_id": "<entity-id>",
    "target_entity_id": "<entity-id>",
    "relation_type": "depends_on"
  }'

# Get all relations for an entity
curl -s "$BASE/v1/relations/<entity-id>" \
  -H "Authorization: Bearer $TOKEN"

# Delete a relation
curl -s -X DELETE "$BASE/v1/relations/<relation-id>/delete" \
  -H "Authorization: Bearer $TOKEN"

# Traverse the graph from an entity (returns connected entities + relations)
curl -s "$BASE/v1/graph/traverse/<entity-id>" \
  -H "Authorization: Bearer $TOKEN"
```

**Note:** Knowledge graph features require Pro tier. The `graph build` and `graph stats` CLI
commands are available for bulk operations.

### Webhooks (HTTP API)

Webhooks let you subscribe to memory events (create, update, delete) and route them to
external services (Slack, Zapier, n8n, etc.). Pro tier only.

```bash
# List webhooks
curl -s "$BASE/v1/webhooks" -H "Authorization: Bearer $TOKEN"

# Create a webhook
curl -s -X POST "$BASE/v1/webhooks" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "url": "https://hooks.slack.com/services/...",
    "events": ["memory.created", "memory.updated", "memory.deleted"],
    "secret": "optional-hmac-secret"
  }'

# Delete a webhook
curl -s -X DELETE "$BASE/v1/webhooks/<webhook-id>" \
  -H "Authorization: Bearer $TOKEN"
```

### Consolidate (MCP Only)

The `consolidate` tool finds duplicate/related memories for a subject so you can merge them.
There's no CLI or HTTP equivalent — use it via MCP or manually:

```bash
# Manual consolidation workflow:
# 1. Search for a subject's memories
ctxovrflw recall "project:myapp" --limit 20

# 2. Review the results for duplicates
# 3. Delete redundant ones
ctxovrflw forget <duplicate-id>
```

### When to Use CLI/API vs MCP

| Scenario                                                     | Use                                   |
| ------------------------------------------------------------ | ------------------------------------- |
| AI tool with native MCP support (Cursor, Claude Code, Cline) | MCP tools                             |
| AI tool without MCP support                                  | HTTP API                              |
| Scripts and automation                                       | CLI commands                          |
| Debugging or manual memory management                        | CLI or TUI (`ctxovrflw memories`)     |
| OpenClaw agents                                              | CLI via `exec`, or MCP via `mcporter` |
| CI/CD pipelines                                              | CLI commands with `--yes` flag        |
