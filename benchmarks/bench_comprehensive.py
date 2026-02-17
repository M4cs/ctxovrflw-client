#!/usr/bin/env python3
"""
Comprehensive Memory Benchmark (MAB-inspired)

Expanded benchmark covering 8 categories and 30 scenarios, testing memory
capabilities beyond coding context. Inspired by MemoryAgentBench methodology:
- Temporal reasoning (when things happened, ordering events)
- Entity tracking (people, places, organizations)
- Preference evolution (changed opinions over time)
- Multi-hop reasoning (connecting multiple memories)
- Long-term consistency (facts seeded across time)
- Emotional/social context (sentiment, relationships)
- Contradiction handling (updated/corrected information)
- Spatial/environmental (locations, setups, configurations)

Two modes:
- baseline: No memory, agent can only use training knowledge
- ctxovrflw: MCP recall tools with pre-seeded memories
"""

import sys
import asyncio
import time
import json
import os
import requests
from typing import Dict, List, Any
from datetime import datetime
from dataclasses import dataclass, field

sys.stdout.reconfigure(line_buffering=True)

CTXOVRFLW_API_BASE = "http://127.0.0.1:7437"
RESULTS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")


@dataclass
class Scenario:
    id: str
    category: str
    question: str
    ground_truth: str
    keywords: List[str]
    memories: List[Dict[str, Any]]  # Can seed multiple memories per scenario


SCENARIOS = [
    # ═══════════════════════════════════════════════════════════════════
    # Category 1: TEMPORAL REASONING
    # Tests ability to recall when things happened and order events
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="temporal_1_event_order",
        category="Temporal",
        question="What was the sequence of major ctxovrflw releases this month? List them in order.",
        ground_truth="v0.3.7 (Feb 8) → v0.3.8 (Feb 10) → v0.3.9 (Feb 12) → v0.4.0 (Feb 14) → v0.4.2 (Feb 16)",
        keywords=["v0.3.7", "v0.3.8", "v0.3.9", "v0.4.0", "v0.4.2"],
        memories=[
            {"content": "Release v0.3.7 shipped on February 8, 2026. Changes: mandatory E2E encryption for sync, ONNX embedder singleton (recall latency 9300ms → 3ms).", "type": "semantic", "tags": ["release", "v0.3.7", "timeline"], "subject": "ctxovrflw"},
            {"content": "Release v0.3.8 shipped on February 10, 2026. Changes: hybrid search with RRF (k=60), signup flow fix, dashboard crash fix.", "type": "semantic", "tags": ["release", "v0.3.8", "timeline"], "subject": "ctxovrflw"},
            {"content": "Release v0.3.9 shipped on February 12, 2026. Changes: JWT session tokens for web frontend, cross-device PIN fix.", "type": "semantic", "tags": ["release", "v0.3.9", "timeline"], "subject": "ctxovrflw"},
            {"content": "Release v0.4.0 shipped on February 14, 2026. Changes: per-device API keys, devices table migration.", "type": "semantic", "tags": ["release", "v0.4.0", "timeline"], "subject": "ctxovrflw"},
            {"content": "Release v0.4.2 shipped on February 16, 2026. Changes: server-side PIN salt, eliminated email dependency from key derivation.", "type": "semantic", "tags": ["release", "v0.4.2", "timeline"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="temporal_2_recency",
        category="Temporal",
        question="What was the most recent bug we fixed and when?",
        ground_truth="The most recent fix was the tokio blocking_lock panic in sync that crashed SSE connections, fixed on February 16 in v0.4.4.",
        keywords=["blocking_lock", "panic", "SSE", "v0.4.4", "February 16"],
        memories=[
            {"content": "Bug fix February 10: signup email verification polling was broken, returning 404 instead of polling status.", "type": "semantic", "tags": ["bug", "fix", "timeline"], "subject": "ctxovrflw"},
            {"content": "Bug fix February 14: cross-device PIN verification failed because derive_key() used email as salt, which differed between devices.", "type": "semantic", "tags": ["bug", "fix", "timeline"], "subject": "ctxovrflw"},
            {"content": "Bug fix February 16: tokio::sync::Mutex::blocking_lock() panicked in auto-sync task, crashing tokio worker threads and killing active SSE/MCP connections. Fixed by switching to std::sync::Mutex. Shipped in v0.4.4.", "type": "semantic", "tags": ["bug", "fix", "timeline", "recent"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="temporal_3_duration",
        category="Temporal",
        question="How long did it take to go from the first version with cloud sync to the version with E2E encryption?",
        ground_truth="Cloud sync was added in v0.2.5 (late January) and mandatory E2E encryption was added in v0.3.7 (February 8), roughly 1-2 weeks.",
        keywords=["v0.2.5", "v0.3.7", "sync", "encryption", "weeks"],
        memories=[
            {"content": "v0.2.5 (late January 2026): First version with cloud sync support. Memories could be pushed/pulled to the cloud API.", "type": "semantic", "tags": ["release", "milestone", "sync"], "subject": "ctxovrflw"},
            {"content": "v0.3.7 (February 8, 2026): Mandatory E2E encryption for all cloud sync. No plaintext sync path allowed. get_encryption_key() returns Result<[u8;32]> not Option.", "type": "semantic", "tags": ["release", "milestone", "encryption"], "subject": "ctxovrflw"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 2: ENTITY TRACKING
    # Tests tracking of people, organizations, and their attributes
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="entity_1_people_roles",
        category="Entity Tracking",
        question="Who are all the people involved in ctxovrflw development and what are their roles?",
        ground_truth="Max (founder, main dev), Jake (Rust Discord, ONNX CI fix), Sarah (designer beta tester), Tom (backend dev beta tester), Lisa (PM at potential enterprise client)",
        keywords=["Max", "Jake", "Sarah", "Tom", "Lisa"],
        memories=[
            {"content": "Max B is the founder and primary developer of ctxovrflw. Software engineer based in Boston, EST timezone. Goal: build profitable software.", "type": "semantic", "tags": ["people", "team"], "subject": "Max"},
            {"content": "Jake from Rust Discord helped debug ONNX runtime linking on ARM64. Contributed CI fix for cross-compilation.", "type": "semantic", "tags": ["people", "contributor"], "subject": "Jake"},
            {"content": "Sarah is a designer who beta-tested ctxovrflw. Found the init wizard confusing, recommended better defaults.", "type": "semantic", "tags": ["people", "beta-tester"], "subject": "Sarah"},
            {"content": "Tom is a backend developer who beta-tested ctxovrflw. Reported recall latency issue (9+ seconds) before singleton fix.", "type": "semantic", "tags": ["people", "beta-tester"], "subject": "Tom"},
            {"content": "Lisa is a PM at a fintech startup, potential enterprise client. Interested in team shared memory spaces. Met at a meetup, exchanged emails.", "type": "semantic", "tags": ["people", "lead", "enterprise"], "subject": "Lisa"},
        ],
    ),
    Scenario(
        id="entity_2_org_details",
        category="Entity Tracking",
        question="What companies or organizations have expressed interest in ctxovrflw?",
        ground_truth="Lisa's fintech startup wants team shared memories. A DevOps agency asked about self-hosted deployment. The Rust Discord community has been supportive.",
        keywords=["fintech", "Lisa", "DevOps", "self-hosted", "Rust Discord"],
        memories=[
            {"content": "Lisa's fintech startup (unnamed, ~50 devs) interested in ctxovrflw for team shared memory. They want on-prem deployment option. Key concern: SOC2 compliance.", "type": "semantic", "tags": ["leads", "enterprise", "fintech"], "subject": "enterprise-leads"},
            {"content": "A DevOps agency (via Moltbook DM) asked about self-hosted ctxovrflw deployment for their clients. Want to bundle with their CI/CD offering.", "type": "semantic", "tags": ["leads", "enterprise", "devops"], "subject": "enterprise-leads"},
            {"content": "Rust Discord community has been supportive of ctxovrflw. Several members testing it. Jake contributed code. Good word-of-mouth channel.", "type": "semantic", "tags": ["community", "rust", "marketing"], "subject": "community"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 3: PREFERENCE EVOLUTION
    # Tests remembering how preferences changed over time
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="pref_evo_1_tech_choice",
        category="Preference Evolution",
        question="How has our approach to database encryption changed over the project lifetime?",
        ground_truth="Started with no encryption, then added optional encryption, then made E2E mandatory after security audit. Also switched from email-based salt to server-side salt.",
        keywords=["optional", "mandatory", "PIN", "plaintext", "security audit"],
        memories=[
            {"content": "Early ctxovrflw (v0.1.x): No encryption for cloud sync. Memories stored in plaintext on server. Quick prototype phase.", "type": "semantic", "tags": ["history", "encryption", "evolution"], "subject": "ctxovrflw"},
            {"content": "v0.2.x: Added optional E2E encryption. Users could set a PIN to encrypt before sync. Some users skipped it for convenience.", "type": "semantic", "tags": ["history", "encryption", "evolution"], "subject": "ctxovrflw"},
            {"content": "v0.3.7: Made E2E encryption mandatory after security audit found plaintext sync was a liability. No opt-out. get_encryption_key() returns Result not Option.", "type": "semantic", "tags": ["history", "encryption", "evolution"], "subject": "ctxovrflw"},
            {"content": "v0.4.2: Switched PIN key derivation from email-based salt to server-side random salt. Email in salt caused cross-device failures.", "type": "semantic", "tags": ["history", "encryption", "evolution"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="pref_evo_2_opinion_change",
        category="Preference Evolution",
        question="What's Max's current stance on using AI for marketing, and how has it changed?",
        ground_truth="Initially skeptical about AI marketing, then tried Moltbook engagement and found genuine community interaction works. Now believes in 'community member first, not advertiser' approach.",
        keywords=["skeptical", "Moltbook", "genuine", "community", "advertiser"],
        memories=[
            {"content": "January 2026: Max initially skeptical about using AI for marketing. Worried it would come across as spammy and inauthentic.", "type": "semantic", "tags": ["preference", "marketing", "evolution"], "subject": "Max"},
            {"content": "February 2026: Max tried Moltbook AI social network for ctxovrflw outreach. Set up automated engagement with strict rules: max 2-3 comments per run, genuine insights only, community member first not advertiser.", "type": "semantic", "tags": ["preference", "marketing", "evolution"], "subject": "Max"},
            {"content": "Current stance: Max believes AI-assisted community engagement works IF it's genuine. Quality over quantity. Reply to help, not to sell. The Moltbook experiment validated this approach.", "type": "semantic", "tags": ["preference", "marketing", "current"], "subject": "Max"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 4: MULTI-HOP REASONING
    # Tests connecting information across multiple memories
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="multihop_1_cause_effect",
        category="Multi-hop",
        question="Why did our benchmark results improve dramatically between the first and second run?",
        ground_truth="First run had SSE connection failures caused by blocking_lock panic in sync, which killed tokio workers. Fixing the mutex (std::sync instead of tokio::sync) resolved SSE drops, so MCP recall worked reliably.",
        keywords=["mutex", "tokio", "std::sync", "MCP", "recall", "69%"],
        memories=[
            {"content": "First benchmark run (Feb 16 morning): ctxovrflw mode scored only 69% coverage. 5 out of 11 scenarios showed 'service unreachable' when calling MCP recall.", "type": "semantic", "tags": ["benchmark", "results", "failure"], "subject": "benchmarks"},
            {"content": "Root cause found: tokio::sync::Mutex::blocking_lock() in sync/mod.rs panicked inside async context, crashing tokio worker threads that were serving SSE connections.", "type": "semantic", "tags": ["bug", "root-cause", "mutex"], "subject": "ctxovrflw"},
            {"content": "Fix: Switched global embedder from tokio::sync::Mutex to std::sync::Mutex. CPU-bound ONNX work should use blocking mutex anyway. Second benchmark run: 98% coverage, 0 failures.", "type": "semantic", "tags": ["fix", "benchmark", "improvement"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="multihop_2_dependency_chain",
        category="Multi-hop",
        question="What's the connection between Jake's contribution and our benchmark improvement?",
        ground_truth="Jake fixed ONNX ARM64 CI, which enabled the embedder to work. The embedder singleton used tokio::Mutex which caused the panic. Fixing to std::sync::Mutex (still using Jake's ONNX setup) fixed benchmarks.",
        keywords=["Jake", "ONNX", "ARM64", "embedder", "Mutex"],
        memories=[
            {"content": "Jake contributed the ONNX runtime CI fix for ARM64, enabling cross-platform ONNX builds.", "type": "semantic", "tags": ["contribution", "onnx", "ci"], "subject": "Jake"},
            {"content": "ONNX embedder is loaded as a global singleton (Arc<Mutex<Embedder>>) at daemon startup, shared across HTTP, MCP, and sync tasks.", "type": "semantic", "tags": ["architecture", "onnx", "singleton"], "subject": "ctxovrflw"},
            {"content": "The embedder singleton originally used tokio::sync::Mutex. The sync task called blocking_lock() which panicked inside the tokio runtime, crashing SSE connections.", "type": "semantic", "tags": ["bug", "mutex", "onnx"], "subject": "ctxovrflw"},
            {"content": "Switching to std::sync::Mutex fixed the panic. The ONNX embedder (enabled by Jake's CI work) now works reliably across all concurrent contexts.", "type": "semantic", "tags": ["fix", "onnx", "mutex"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="multihop_3_impact_analysis",
        category="Multi-hop",
        question="If we downgraded a Pro user to Free tier, what specific features would they lose?",
        ground_truth="They'd lose hybrid search (falls back to keyword only), knowledge graph, webhooks, consolidation, context synthesis, cloud sync, and go from unlimited to 100 memory cap.",
        keywords=["hybrid", "knowledge graph", "webhooks", "consolidation", "cloud sync", "100"],
        memories=[
            {"content": "Free tier features: keyword search only, 100 memory limit, local storage only, 1 device.", "type": "semantic", "tags": ["pricing", "free", "limits"], "subject": "ctxovrflw"},
            {"content": "Pro tier features: hybrid search (semantic + keyword + RRF), unlimited memories, knowledge graph (entities + relations), webhooks, consolidation, context synthesis, unlimited devices.", "type": "semantic", "tags": ["pricing", "pro", "features"], "subject": "ctxovrflw"},
            {"content": "Cloud sync requires Standard tier or above. Free users can only use local storage.", "type": "semantic", "tags": ["pricing", "sync", "limits"], "subject": "ctxovrflw"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 5: LONG-TERM CONSISTENCY
    # Tests recall of facts that were established early and never repeated
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="longterm_1_origin",
        category="Long-term",
        question="Why was ctxovrflw created? What was the original motivation?",
        ground_truth="Max was frustrated that AI agents forgot everything between sessions. Context windows are expensive and ephemeral. Wanted persistent, private, cross-agent memory.",
        keywords=["started from zero", "context window", "persistent", "private", "MCP"],
        memories=[
            {"content": "ctxovrflw origin story: Max was frustrated that every AI coding session started from zero. Claude Code, Cursor, Copilot — none remembered decisions from yesterday. Context windows are expensive and ephemeral. He wanted persistent memory that's private (local-first), works across any AI tool (MCP), and syncs across devices.", "type": "semantic", "tags": ["origin", "motivation", "founding"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="longterm_2_early_decision",
        category="Long-term",
        question="Why did we choose Rust for the daemon instead of Python or Go?",
        ground_truth="Rust for performance (sub-ms recall), single binary distribution, memory safety without GC pauses, and SQLite FFI is straightforward.",
        keywords=["performance", "binary", "memory safety", "SQLite", "GC"],
        memories=[
            {"content": "Architecture decision: Rust chosen for ctxovrflw daemon. Reasons: sub-millisecond recall latency (no GC pauses), compiles to single static binary (easy distribution), memory safety guarantees for a long-running daemon, excellent SQLite FFI via rusqlite, and strong async ecosystem (tokio) for HTTP/SSE server.", "type": "semantic", "tags": ["decision", "architecture", "rust"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="longterm_3_name",
        category="Long-term",
        question="How did ctxovrflw get its name?",
        ground_truth="Play on 'context overflow' — AI context windows overflow and forget. ctxovrflw catches what overflows. Also a nod to stackoverflow for developers.",
        keywords=["context overflow", "overflow", "catches", "stackoverflow"],
        memories=[
            {"content": "Name origin: ctxovrflw = 'context overflow'. AI context windows have limited space — when they overflow, knowledge is lost. ctxovrflw catches what overflows and persists it. Also a deliberate nod to stackoverflow — a tool developers already associate with finding answers. The abbreviated spelling (no vowels) follows Unix naming conventions.", "type": "semantic", "tags": ["name", "branding", "origin"], "subject": "ctxovrflw"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 6: EMOTIONAL/SOCIAL CONTEXT
    # Tests recall of sentiment, reactions, and social dynamics
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="social_1_reaction",
        category="Social",
        question="How did the team react to the security audit findings?",
        ground_truth="Max was alarmed by the 0.0.0.0 binding (called it 'holy shit moment'). Prioritized it immediately as P0. Was relieved the fix was simple (one-line change to 127.0.0.1).",
        keywords=["holy shit", "0.0.0.0", "P0", "127.0.0.1"],
        memories=[
            {"content": "Max's reaction to security audit: 'holy shit moment' when he saw the daemon was binding 0.0.0.0 — anyone on the network could access memories. Immediately classified as P0, dropped everything else. Was relieved the fix was just changing one line to 127.0.0.1 in the HTTP server config.", "type": "semantic", "tags": ["reaction", "security", "sentiment"], "subject": "Max"},
        ],
    ),
    Scenario(
        id="social_2_frustration",
        category="Social",
        question="What technical issue caused the most frustration during development?",
        ground_truth="The cross-device PIN verification failure was the most frustrating. Took 3 days to find because email salt mismatch was subtle — worked on same device, broke across devices.",
        keywords=["PIN", "cross-device", "3 days", "email", "PBKDF2"],
        memories=[
            {"content": "Most frustrating bug: cross-device PIN verification. Took 3 days to diagnose. derive_key() used email as PBKDF2 salt, but email casing or encoding differed slightly between devices. Worked perfectly on the same device (same email string), only broke when syncing to a second device. Subtle and maddening. Fixed by moving to server-side random salt in v0.4.2.", "type": "semantic", "tags": ["frustration", "bug", "pin", "debugging"], "subject": "ctxovrflw"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 7: CONTRADICTION HANDLING
    # Tests ability to find the most current/corrected information
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="contradiction_1_corrected",
        category="Contradiction",
        question="What database does ctxovrflw use for the cloud API?",
        ground_truth="PostgreSQL via Drizzle ORM on Railway. Initially considered SQLite for cloud too but switched for concurrent access.",
        keywords=["PostgreSQL", "Drizzle", "Railway"],
        memories=[
            {"content": "Early plan: use SQLite for both daemon and cloud API. Simple, consistent stack.", "type": "semantic", "tags": ["decision", "database", "early"], "subject": "ctxovrflw-cloud"},
            {"content": "CORRECTION: Cloud API switched from SQLite to PostgreSQL (via Drizzle ORM on Railway). SQLite couldn't handle concurrent write access from multiple API instances. Daemon still uses SQLite locally.", "type": "semantic", "tags": ["decision", "database", "correction", "current"], "subject": "ctxovrflw-cloud"},
        ],
    ),
    Scenario(
        id="contradiction_2_updated",
        category="Contradiction",
        question="How much does the Pro tier cost?",
        ground_truth="$15/month. Was originally $10/month but raised after adding knowledge graph and webhooks.",
        keywords=["$15", "originally", "$10", "knowledge graph", "webhooks"],
        memories=[
            {"content": "Original pricing plan: Pro tier at $10/month with semantic search, unlimited memories, and cloud sync.", "type": "semantic", "tags": ["pricing", "original", "outdated"], "subject": "ctxovrflw"},
            {"content": "Updated pricing (current): Pro tier raised to $15/month after adding knowledge graph, webhooks, consolidation, and context synthesis features. Justified by significant new value.", "type": "semantic", "tags": ["pricing", "current", "updated"], "subject": "ctxovrflw"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Category 8: SPATIAL/ENVIRONMENTAL
    # Tests recall of setups, configurations, locations
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="spatial_1_infrastructure",
        category="Spatial",
        question="Describe the full deployment infrastructure — where does each component run?",
        ground_truth="Daemon runs locally on user machines. Cloud API on Railway (Hono/Bun). Website on Vercel (Vite/React). CI on GitHub Actions (public repo). Database is PostgreSQL on Railway.",
        keywords=["Railway", "GitHub Actions", "PostgreSQL", "local", "systemd"],
        memories=[
            {"content": "ctxovrflw infrastructure map: Daemon — runs on user's local machine (systemd service on Linux, launchd on Mac). Cloud API — Railway (Hono/Bun/TypeScript, PostgreSQL). Website (ctxovrflw.dev) — Vercel (Vite/React). CI — GitHub Actions on public repo M4cs/ctxovrflw-client (5 platform builds). DNS — Cloudflare.", "type": "semantic", "tags": ["infrastructure", "deployment", "architecture"], "subject": "ctxovrflw"},
        ],
    ),
    Scenario(
        id="spatial_2_dev_setup",
        category="Spatial",
        question="What's Max's development environment setup?",
        ground_truth="VPS at Hostinger (srv1370565), WSL on Windows desktop, uses OpenClaw AI assistant (Aldous), Telegram for notifications, VS Code as editor.",
        keywords=["VPS", "Hostinger", "WSL", "OpenClaw", "Telegram", "VS Code"],
        memories=[
            {"content": "Max's dev setup: Primary development on VPS at Hostinger (srv1370565.hstgr.cloud, Linux x64). Secondary on WSL (Windows desktop). Uses OpenClaw AI assistant named Aldous for automation via Telegram. Editor: VS Code. Rust toolchain via rustup. Node via nvm.", "type": "semantic", "tags": ["setup", "development", "environment"], "subject": "Max"},
        ],
    ),

    # ═══════════════════════════════════════════════════════════════════
    # Additional: PERSONAL FACTS (non-technical)
    # ═══════════════════════════════════════════════════════════════════
    Scenario(
        id="personal_1_schedule",
        category="Personal",
        question="When is Max usually available and when should I avoid messaging?",
        ground_truth="Max is in EST timezone (Boston). Usually active 9am-midnight. Avoid 1am-8am EST. Prefers async communication.",
        keywords=["EST", "Boston", "9 AM", "midnight", "async"],
        memories=[
            {"content": "Max's availability: EST timezone (Boston). Typically active 9am–midnight EST. Deep work blocks usually morning (10am-1pm). Avoid messaging 1am–8am EST unless urgent. Prefers async communication — don't expect instant replies during focus time.", "type": "preference", "tags": ["schedule", "availability", "timezone"], "subject": "Max"},
        ],
    ),
    Scenario(
        id="personal_2_goals",
        category="Personal",
        question="What are Max's long-term career goals beyond ctxovrflw?",
        ground_truth="Build wealth through software. ctxovrflw is the current vehicle but the broader goal is multiple revenue streams from developer tools. Wants financial independence.",
        keywords=["wealth", "software", "revenue", "developer tools", "financial independence"],
        memories=[
            {"content": "Max's goals: Primary drive is building wealth through software. ctxovrflw is the current focus but not the only bet — wants to build multiple revenue streams from developer tools. Long-term goal: financial independence through profitable software products, not VC-funded growth.", "type": "semantic", "tags": ["goals", "career", "personal"], "subject": "Max"},
        ],
    ),
    Scenario(
        id="personal_3_pet_peeves",
        category="Personal",
        question="What annoys Max about working with AI assistants?",
        ground_truth="Hates sycophancy ('yes man' behavior), verbose responses, asking permission for obvious things, and losing context between sessions.",
        keywords=["sycophancy", "verbose", "permission", "context", "sessions"],
        memories=[
            {"content": "Max's pet peeves with AI assistants: 1) Sycophancy — hates 'yes man' behavior, wants honest pushback. 2) Verbose responses — says 'say it once, say it well'. 3) Asking permission for obvious read-only tasks. 4) Losing context between sessions (this is literally why he built ctxovrflw). 5) Corporate-speak and filler phrases.", "type": "preference", "tags": ["pet-peeves", "ai", "preference"], "subject": "Max"},
        ],
    ),
]


def seed_memory(mem: Dict[str, Any]) -> bool:
    try:
        resp = requests.post(
            f"{CTXOVRFLW_API_BASE}/v1/memories",
            json=mem, timeout=10,
            headers={"Content-Type": "application/json"},
        )
        return resp.status_code in [200, 201]
    except:
        return False


def clear_bench_memories():
    """Clear all memories to start fresh."""
    try:
        # Get all memories and delete them
        for query in ["ctxovrflw", "Max", "benchmark", "release", "bug", "pricing",
                      "people", "enterprise", "architecture", "preference", "personal"]:
            resp = requests.post(
                f"{CTXOVRFLW_API_BASE}/v1/memories/recall",
                json={"query": query, "limit": 50}, timeout=10,
            )
            if resp.status_code == 200:
                for r in resp.json().get("results", []):
                    mid = r.get("memory", {}).get("id")
                    if mid:
                        requests.delete(f"{CTXOVRFLW_API_BASE}/v1/memories/{mid}", timeout=5)
    except Exception as e:
        print(f"  Clear error: {e}")


async def run_scenario(scenario: Scenario, mode: str) -> Dict[str, Any]:
    from claude_agent_sdk import query as sdk_query, ClaudeAgentOptions
    from claude_agent_sdk import AssistantMessage, ResultMessage, TextBlock, ToolUseBlock

    cwd = "/home/max/.openclaw/workspace/ctxovrflw"

    if mode == "baseline":
        system_prompt = """You are a helpful AI assistant. Answer the user's question based on what you know.
If you don't have the information, say so honestly. Do NOT make up facts.
You have no access to external memory or context systems."""
        mcp = False
    else:
        system_prompt = """You are a helpful AI assistant with access to persistent memory via ctxovrflw MCP tools.

IMPORTANT WORKFLOW:
1. ALWAYS call recall first with relevant search queries before answering.
2. Try multiple recall queries if the first doesn't find what you need.
3. Answer based on what you find in memory.
4. If memory contains the information, USE it confidently — don't hedge.
5. If recall returns nothing relevant, say you don't have that stored.

Recall is fast (~3ms) and free (local search). Use it liberally."""
        mcp = True

    start = time.time()
    stderr_lines = []
    options = ClaudeAgentOptions(
        allowed_tools=[] if mode == "baseline" else ["Read", "Bash", "Glob"],
        cwd=cwd,
        max_turns=5,
        permission_mode="bypassPermissions",
        debug_stderr=None,
        stderr=lambda l: stderr_lines.append(l),
        system_prompt=system_prompt,
    )

    if mcp:
        options.mcp_servers = {"ctxovrflw": {
            "type": "sse",
            "url": f"{CTXOVRFLW_API_BASE}/mcp/sse",
        }}

    tool_calls = []
    text_parts = []
    result_data = None
    error = None

    try:
        async for msg in sdk_query(prompt=scenario.question, options=options):
            if isinstance(msg, AssistantMessage):
                for block in (msg.content or []):
                    if isinstance(block, TextBlock):
                        text_parts.append(block.text)
                    elif isinstance(block, ToolUseBlock):
                        tool_calls.append({"name": block.name, "input": block.input})
            elif isinstance(msg, ResultMessage):
                result_data = msg
    except Exception as e:
        error = str(e)
        print(f"    ⚠ Error: {e}")

    elapsed_ms = (time.time() - start) * 1000

    input_tokens = 0
    output_tokens = 0
    final_answer = ""
    if result_data:
        usage = result_data.usage or {}
        input_tokens = usage.get("input_tokens", 0) + usage.get("cache_read_input_tokens", 0) + usage.get("cache_creation_input_tokens", 0)
        output_tokens = usage.get("output_tokens", 0)
        if result_data.result:
            final_answer = result_data.result
    if not final_answer:
        final_answer = "\n".join(text_parts)

    # Score: keyword coverage
    answer_lower = final_answer.lower()
    hits = [kw for kw in scenario.keywords if kw.lower() in answer_lower]
    coverage = len(hits) / len(scenario.keywords) if scenario.keywords else 0

    admits_no_knowledge = any(phrase in answer_lower for phrase in [
        "don't have", "don't know", "no information", "not aware",
        "cannot recall", "no memory", "no context", "not stored",
        "no specific", "unable to find",
    ])

    return {
        "scenario_id": scenario.id,
        "category": scenario.category,
        "mode": mode,
        "tool_calls": len(tool_calls),
        "tool_details": [tc["name"] for tc in tool_calls],
        "elapsed_ms": elapsed_ms,
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": input_tokens + output_tokens,
        "keyword_coverage": coverage,
        "hit_keywords": hits,
        "missed_keywords": [kw for kw in scenario.keywords if kw.lower() not in answer_lower],
        "admits_no_knowledge": admits_no_knowledge,
        "answer_preview": final_answer[:300],
        "error": error,
    }


async def main():
    print("Comprehensive Memory Benchmark (MAB-inspired)")
    print("=" * 70)

    categories = sorted(set(s.category for s in SCENARIOS))
    print(f"Categories: {len(categories)} — {', '.join(categories)}")
    print(f"Scenarios: {len(SCENARIOS)}")
    print(f"Total runs: {len(SCENARIOS) * 2}")
    print()

    # Health check
    try:
        resp = requests.get(f"{CTXOVRFLW_API_BASE}/health", timeout=5)
        info = resp.json()
        print(f"Daemon: {info.get('version', '?')} — {info.get('status', '?')}\n")
    except:
        print("❌ Daemon not reachable\n")
        return

    # Clear and seed memories
    print("Clearing existing memories...")
    clear_bench_memories()
    import time as t; t.sleep(1)

    total_memories = sum(len(s.memories) for s in SCENARIOS)
    print(f"Seeding {total_memories} memories across {len(SCENARIOS)} scenarios...")
    seeded = 0
    for s in SCENARIOS:
        for mem in s.memories:
            if seed_memory(mem):
                seeded += 1
    print(f"  ✓ {seeded}/{total_memories} memories seeded\n")

    # Run benchmarks
    results = []
    total = len(SCENARIOS) * 2
    current = 0

    for scenario in SCENARIOS:
        for mode in ["baseline", "ctxovrflw"]:
            current += 1
            print(f"[{current}/{total}] {scenario.id} — {mode}")
            print(f"   Q: {scenario.question}")

            result = await run_scenario(scenario, mode)
            results.append(result)

            emoji = "✅" if result["keyword_coverage"] >= 0.6 else "⚠️" if result["keyword_coverage"] >= 0.3 else "❌"
            knows = "admits no knowledge" if result["admits_no_knowledge"] else f"coverage={result['keyword_coverage']:.0%}"
            print(f"   {emoji} {mode:10s}: tools={result['tool_calls']}, {result['elapsed_ms']/1000:.1f}s, {result['total_tokens']} tokens, {knows}")
            if result["missed_keywords"]:
                print(f"      Missed: {', '.join(result['missed_keywords'][:5])}")
            print()

            await asyncio.sleep(1)

    # ── Summary ──
    print("=" * 70)
    print("OVERALL SUMMARY")
    print("-" * 70)

    for mode in ["baseline", "ctxovrflw"]:
        mr = [r for r in results if r["mode"] == mode]
        print(f"  {mode:10s}: tools={sum(r['tool_calls'] for r in mr)/len(mr):.1f}, "
              f"time={sum(r['elapsed_ms'] for r in mr)/len(mr)/1000:.1f}s, "
              f"tokens={sum(r['total_tokens'] for r in mr)/len(mr):.0f}, "
              f"coverage={sum(r['keyword_coverage'] for r in mr)/len(mr):.0%}, "
              f"no_knowledge={sum(1 for r in mr if r['admits_no_knowledge'])}/{len(mr)}")

    # Per-category summary
    print()
    print("PER-CATEGORY BREAKDOWN")
    print("-" * 70)
    print(f"{'Category':25s} | {'Base Cov':>8s} | {'Ctx Cov':>8s} | {'Delta':>6s} | {'Base NK':>7s} | {'Ctx NK':>7s}")
    print("-" * 70)
    for cat in categories:
        base = [r for r in results if r["category"] == cat and r["mode"] == "baseline"]
        ctx = [r for r in results if r["category"] == cat and r["mode"] == "ctxovrflw"]
        b_cov = sum(r["keyword_coverage"] for r in base) / len(base) if base else 0
        c_cov = sum(r["keyword_coverage"] for r in ctx) / len(ctx) if ctx else 0
        b_nk = sum(1 for r in base if r["admits_no_knowledge"])
        c_nk = sum(1 for r in ctx if r["admits_no_knowledge"])
        n = len(base)
        print(f"  {cat:23s} | {b_cov:>7.0%} | {c_cov:>7.0%} | {c_cov-b_cov:>+5.0%} | {b_nk:>3d}/{n:<3d} | {c_nk:>3d}/{n:<3d}")

    # Per-scenario delta
    print()
    print("PER-SCENARIO DELTA (ctxovrflw - baseline):")
    print("-" * 70)
    for scenario in SCENARIOS:
        base = next((r for r in results if r["scenario_id"] == scenario.id and r["mode"] == "baseline"), None)
        ctx = next((r for r in results if r["scenario_id"] == scenario.id and r["mode"] == "ctxovrflw"), None)
        if base and ctx:
            print(
                f"  {scenario.id:30s}: "
                f"tools {ctx['tool_calls']-base['tool_calls']:+3d}, "
                f"tokens {ctx['total_tokens']-base['total_tokens']:+6d}, "
                f"time {(ctx['elapsed_ms']-base['elapsed_ms'])/1000:+6.1f}s, "
                f"coverage {ctx['keyword_coverage']-base['keyword_coverage']:+.0%}"
            )

    # MCP overhead calculation
    print()
    base_zero = [r for r in results if r["mode"] == "baseline" and r["tool_calls"] == 0]
    ctx_all = [r for r in results if r["mode"] == "ctxovrflw"]
    if base_zero and ctx_all:
        avg_base = sum(r["total_tokens"] for r in base_zero) / len(base_zero)
        avg_ctx = sum(r["total_tokens"] for r in ctx_all) / len(ctx_all)
        overhead = avg_ctx - avg_base
        print(f"MCP session overhead: ~{overhead:.0f} tokens (one-time, amortized in real usage)")
        print(f"Adjusted ctxovrflw tokens: ~{avg_ctx - overhead:.0f} (vs baseline ~{avg_base:.0f})")

    # Save results
    os.makedirs(RESULTS_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    outpath = os.path.join(RESULTS_DIR, f"comprehensive_{ts}.json")
    with open(outpath, "w") as f:
        json.dump({
            "benchmark": "comprehensive_memory_mab",
            "completed_at": datetime.now().isoformat(),
            "scenarios": len(SCENARIOS),
            "categories": categories,
            "total_memories_seeded": total_memories,
            "results": results,
        }, f, indent=2)
    print(f"\nResults saved: {outpath}")


if __name__ == "__main__":
    asyncio.run(main())
