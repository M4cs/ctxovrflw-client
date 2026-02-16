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
  version: "1.0"
  website: https://ctxovrflw.dev
compatibility: Requires ctxovrflw daemon running locally (MCP server)
---

# ctxovrflw — Shared Memory

You have access to **ctxovrflw**, a shared memory layer that persists across sessions and is
accessible by every AI tool the user runs (Cursor, Claude Code, Cline, Windsurf, etc.).

You interact with it via the MCP tools: `remember`, `recall`, `forget`, and `status`.

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
recall("", subject: "person:sarah")  // everything about sarah
recall("preferences", subject: "user")  // user preferences only
```

Use `subjects` tool to list all known entities.

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
