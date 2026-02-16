---
name: ctxovrflw
description: >
  Shared AI memory layer — recall context from past sessions and other AI tools,
  store decisions and preferences automatically. Use when you need context about
  user preferences, past decisions, project setup, or any information that may
  have been discussed in another AI tool. Also use proactively to store important
  context as it comes up. Cross-tool: what Cursor stores, Claude Code can recall.
metadata:
  author: ctxovrflw
  version: "2.0"
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

### Check if running

```bash
curl -s http://127.0.0.1:7437/health
```

If this returns a JSON response, ctxovrflw is running. If it fails, follow the install steps below.

### Install ctxovrflw

```bash
curl -fsSL https://ctxovrflw.dev/install.sh | sh
```

This installs the daemon binary, ONNX runtime (for semantic search), and the embedding model.
Supports Linux (x64, arm64) and macOS (x64, arm64).

After install:
```bash
ctxovrflw init     # Interactive setup — creates config, starts daemon
ctxovrflw login    # Authenticate with cloud (optional, needed for sync)
ctxovrflw start    # Start the daemon as a background service
```

The daemon runs on `http://127.0.0.1:7437` (localhost only) and exposes an MCP SSE endpoint at
`/mcp/sse` for AI tool integrations.

### Update ctxovrflw

```bash
ctxovrflw update   # Downloads and installs the latest version
```

## Tiers & Pricing

| Feature | Free ($0) | Standard ($10/mo) | Pro ($20/mo) |
|---------|-----------|-------------------|--------------|
| Memories | 100 | Unlimited | Unlimited |
| Devices | 1 | 3 | Unlimited |
| Semantic search | ✅ | ✅ | ✅ |
| Cloud sync (E2E encrypted) | ❌ | ✅ | ✅ |
| Context synthesis | ❌ | ❌ | ✅ |
| Consolidation | ❌ | ❌ | ✅ |
| Knowledge graph | ❌ | ❌ | ✅ |
| Webhooks | ✅ | ✅ | ✅ |

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

## Requirements

- ctxovrflw Daemon

## Installation

```
curl -fsSL https://ctxovrflw.dev/install.sh | sh
```

This will install the ctxovrflw daemon onto your machine. You can start the daemon with `ctxovrflw start`

If you want to enable cloud sync, the user will need to have an active subscription and login. You can use `ctxovrflw login` to do so.

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

## Memory Types

| Type | Use for | Example |
|------|---------|---------|
| `preference` | User likes, config choices, style | "Prefers Rust for backend services" |
| `semantic` | Facts, knowledge, project info | "The API uses PostgreSQL with pgvector" |
| `procedural` | How-to, steps, processes | "To deploy: push to main, Railway auto-deploys" |
| `episodic` | Events, things that happened | "Migrated from Fly.io to Railway on Feb 10" |

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

| Problem | Solution |
|---------|----------|
| MCP tools not working | Check daemon: `curl http://127.0.0.1:7437/health` |
| "Not logged in" | Run `ctxovrflw login` |
| "Memory limit reached" | Upgrade tier or `forget` old memories |
| "Sync PIN expired" | Run `ctxovrflw login` to re-enter PIN |
| Slow semantic search | First query loads ONNX model (~2s), subsequent queries are fast |
| Cloud sync not working | Check tier with `ctxovrflw account` — Free tier is local-only |
