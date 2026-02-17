#!/usr/bin/env python3
"""
Conversational Memory Benchmark

Tests ctxovrflw's real value prop: recalling facts, preferences,
relationships, project context, and cross-agent knowledge that
DON'T exist in files.

Two modes:
- baseline: Agent has no memory, must say "I don't know" or hallucinate
- ctxovrflw: Agent has MCP recall tools with pre-seeded memories

Measures: tool calls, latency, tokens, answer accuracy
"""

import sys
import asyncio
import time
import json
import os
import requests
from typing import Dict, List, Any, Optional
from datetime import datetime
from dataclasses import dataclass, field

sys.stdout.reconfigure(line_buffering=True)

CTXOVRFLW_API_BASE = "http://127.0.0.1:7437"
RESULTS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")

# ── Conversational Scenarios ──────────────────────────────────────────

@dataclass
class ConversationalScenario:
    id: str
    category: str
    question: str
    ground_truth: str  # Expected answer content
    keywords: List[str]  # Key terms that should appear
    memory_to_seed: Dict[str, Any]  # Memory to store before test

SCENARIOS = [
    # ── Preferences ──
    ConversationalScenario(
        id="pref_1_coding_style",
        category="Preferences",
        question="What coding conventions does Max prefer for Rust projects?",
        ground_truth="Max prefers snake_case, avoids unwrap in production, uses thiserror for errors, and wants clippy pedantic enabled",
        keywords=["snake_case", "unwrap", "thiserror", "clippy"],
        memory_to_seed={
            "content": "Max's Rust coding preferences: snake_case everywhere, never use unwrap() in production code (use ? or expect with context), use thiserror for error types, always enable clippy::pedantic in CI",
            "type": "preference",
            "tags": ["rust", "coding-style", "preference"],
            "subject": "Max",
        },
    ),
    ConversationalScenario(
        id="pref_2_communication",
        category="Preferences",
        question="How does Max like to receive status updates?",
        ground_truth="Max wants brief bullet points, no fluff, only when something is blocked or done. Doesn't want updates on routine progress.",
        keywords=["bullet", "brief", "blocked", "done", "routine"],
        memory_to_seed={
            "content": "Max's communication preference: brief bullet points only. Report when something is blocked or completed. Don't send updates on routine progress — he'll check in when he wants to know.",
            "type": "preference",
            "tags": ["communication", "preference", "updates"],
            "subject": "Max",
        },
    ),

    # ── Facts & Decisions ──
    ConversationalScenario(
        id="fact_1_api_choice",
        category="Facts",
        question="Why did we choose Hono over Express for the cloud API?",
        ground_truth="Chose Hono because it runs on Bun natively, is 3x faster than Express on benchmarks, has built-in Zod validation, and the team wanted to try a modern stack",
        keywords=["Hono", "Bun", "faster", "Express", "Zod"],
        memory_to_seed={
            "content": "Decision: Cloud API uses Hono instead of Express. Reasons: runs natively on Bun (no adapter needed), benchmarked 3x faster than Express for our workload, built-in Zod validation support, and we wanted a modern TypeScript-first framework",
            "type": "semantic",
            "tags": ["decision", "cloud", "api", "hono"],
            "subject": "ctxovrflw-cloud",
        },
    ),
    ConversationalScenario(
        id="fact_2_pricing",
        category="Facts",
        question="What are the ctxovrflw pricing tiers and what's included in each?",
        ground_truth="Free: keyword search, 1000 memories, local only. Standard $5/mo: semantic search, 10K memories, cloud sync. Pro $15/mo: hybrid search, unlimited memories, knowledge graph, webhooks.",
        keywords=["Free", "Standard", "Pro", "keyword", "semantic", "hybrid", "knowledge graph"],
        memory_to_seed={
            "content": "ctxovrflw pricing: Free tier — keyword search only, 1000 memory limit, local storage only. Standard $5/month — semantic search with ONNX embeddings, 10K memories, cloud sync with E2E encryption. Pro $15/month — hybrid search (semantic + keyword + RRF), unlimited memories, knowledge graph, webhooks, priority support.",
            "type": "semantic",
            "tags": ["pricing", "tiers", "product"],
            "subject": "ctxovrflw",
        },
    ),

    # ── Relationships & People ──
    ConversationalScenario(
        id="rel_1_collaborator",
        category="Relationships",
        question="Who helped with the ONNX integration and what was their contribution?",
        ground_truth="Jake from the Rust Discord helped debug the ONNX runtime linking issue on ARM64 Linux. He suggested using ORT_DYLIB_PATH and contributed the cross-compilation CI fix.",
        keywords=["Jake", "ONNX", "ARM64", "ORT_DYLIB_PATH", "CI"],
        memory_to_seed={
            "content": "Jake (from Rust Discord) helped debug ONNX runtime linking on ARM64 Linux. He found that ORT_DYLIB_PATH needs to be set explicitly for cross-compiled builds and contributed the CI fix for the ARM64 runner.",
            "type": "semantic",
            "tags": ["people", "onnx", "contribution", "ci"],
            "subject": "Jake",
        },
    ),
    ConversationalScenario(
        id="rel_2_user_feedback",
        category="Relationships",
        question="What feedback did our early beta testers give us?",
        ground_truth="Sarah said init wizard was confusing, needs better defaults. Tom said recall latency was too slow before the singleton fix. Both loved the privacy-first approach.",
        keywords=["Sarah", "Tom", "init", "latency", "privacy"],
        memory_to_seed={
            "content": "Beta tester feedback summary: Sarah (designer) said the init wizard was confusing and needed better defaults. Tom (backend dev) reported recall was too slow (9+ seconds) before we added the ONNX singleton — much better now at ~3ms. Both testers specifically praised the privacy-first, local-first architecture.",
            "type": "semantic",
            "tags": ["feedback", "beta", "users"],
            "subject": "beta-testers",
        },
    ),

    # ── Project Context ──
    ConversationalScenario(
        id="proj_1_roadmap",
        category="Project Context",
        question="What are the next three features planned for ctxovrflw?",
        ground_truth="Next three: team/org shared memories, VS Code extension with inline recall, and a web dashboard for memory visualization",
        keywords=["team", "shared", "VS Code", "extension", "dashboard", "visualization"],
        memory_to_seed={
            "content": "ctxovrflw roadmap next 3 features: 1) Team/org shared memory spaces — multiple agents can share a memory namespace. 2) VS Code extension with inline recall — hover over code to see relevant memories. 3) Web dashboard for memory visualization — browse, search, and manage memories in a GUI.",
            "type": "semantic",
            "tags": ["roadmap", "features", "planning"],
            "subject": "ctxovrflw",
        },
    ),
    ConversationalScenario(
        id="proj_2_competitor",
        category="Project Context",
        question="How does ctxovrflw compare to Mem0 and what's our main differentiator?",
        ground_truth="Mem0 is cloud-hosted Python, we're local-first Rust. Our differentiator is zero-knowledge encryption and that we work across any AI tool via MCP, not just their SDK.",
        keywords=["Mem0", "cloud", "local-first", "zero-knowledge", "MCP", "Rust"],
        memory_to_seed={
            "content": "Competitive analysis: Mem0 is cloud-hosted, Python-based, requires their SDK. ctxovrflw differentiators: 1) local-first Rust daemon (fast, private), 2) zero-knowledge E2E encryption for sync (they store plaintext), 3) works with ANY AI tool via MCP protocol (not locked to one SDK), 4) hybrid search vs their semantic-only",
            "type": "semantic",
            "tags": ["competition", "mem0", "differentiator"],
            "subject": "ctxovrflw",
        },
    ),

    # ── Cross-Agent Knowledge ──
    ConversationalScenario(
        id="agent_1_other_work",
        category="Cross-Agent",
        question="What did the security audit agent find last week?",
        ground_truth="Found daemon was binding 0.0.0.0 (exposed externally), missing input validation on content field, and no rate limiting on device auth. All fixed in v0.3.7.",
        keywords=["0.0.0.0", "binding", "validation", "rate limiting", "device auth"],
        memory_to_seed={
            "content": "Security audit agent findings (last week): CRITICAL — daemon binding 0.0.0.0 exposing API externally, fixed to 127.0.0.1. HIGH — no input validation on memory content field, added max 100KB cap. MEDIUM — no rate limiting on device auth endpoint, added 5/min limit. All fixes shipped in v0.3.7.",
            "type": "semantic",
            "tags": ["security", "audit", "agent-work", "findings"],
            "subject": "security-audit",
        },
    ),
    ConversationalScenario(
        id="agent_2_deploy_history",
        category="Cross-Agent",
        question="When was the last deployment and what version was it?",
        ground_truth="Last deploy was v0.4.2 on February 16th, included server-side PIN salt and per-device API keys",
        keywords=["v0.4.2", "February", "PIN", "salt", "per-device"],
        memory_to_seed={
            "content": "Deployment log: v0.4.2 deployed on 2026-02-16. Changes: server-side PIN salt (eliminates email dependency in key derivation), per-device API keys (each device gets its own key, login doesn't invalidate other devices). Deployed via scripts/deploy.sh, all 5 CI platforms built successfully.",
            "type": "semantic",
            "tags": ["deployment", "v0.4.2", "release"],
            "subject": "ctxovrflw",
        },
    ),

    # ── Reminders & Time-Sensitive ──
    ConversationalScenario(
        id="remind_1_todo",
        category="Reminders",
        question="What was I supposed to do after the v0.4.2 deploy?",
        ground_truth="Run ctxovrflw update on WSL and VPS, re-login on both devices, and reset the sync PIN since the salt mechanism changed",
        keywords=["update", "WSL", "VPS", "login", "PIN", "reset"],
        memory_to_seed={
            "content": "TODO after v0.4.2 deploy: 1) Run 'ctxovrflw update' on both WSL and VPS. 2) Run 'ctxovrflw logout && ctxovrflw login' on WSL first (creates new PIN with server salt), then VPS (verifies against it). 3) Reset sync PIN since key derivation changed from email salt to server salt.",
            "type": "semantic",
            "tags": ["todo", "reminder", "v0.4.2", "post-deploy"],
            "subject": "Max",
        },
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
    """Clear all benchmark-seeded memories."""
    try:
        for tag in ["preference", "decision", "people", "beta", "roadmap",
                     "competition", "agent-work", "deployment", "todo", "reminder"]:
            resp = requests.post(
                f"{CTXOVRFLW_API_BASE}/v1/memories/recall",
                json={"query": tag, "limit": 50},
                timeout=10,
            )
            if resp.status_code == 200:
                for r in resp.json().get("results", []):
                    mid = r.get("memory", {}).get("id")
                    if mid:
                        requests.delete(f"{CTXOVRFLW_API_BASE}/v1/memories/{mid}", timeout=5)
    except Exception as e:
        print(f"  Clear error: {e}")


async def run_scenario(scenario: ConversationalScenario, mode: str) -> Dict[str, Any]:
    """Run a single scenario in baseline or ctxovrflw mode."""
    from platforms.claude_code import ClaudeCodePlatform

    platform = ClaudeCodePlatform()
    cwd = "/home/max/.openclaw/workspace/ctxovrflw"

    if mode == "baseline":
        system_prompt = """You are a helpful AI assistant. Answer the user's question based on what you know.
If you don't have the information, say so honestly. Do NOT make up facts.
You have no access to external memory or context systems — only your training data."""
        allowed_tools = []
        mcp = False
    else:  # ctxovrflw
        system_prompt = """You are a helpful AI assistant with access to persistent memory via ctxovrflw MCP tools.

Your workflow:
1. FIRST: Use the ctxovrflw recall tool to search your memory for relevant information.
   Try semantic queries related to the question topic.
2. THEN: Answer based on what you find in memory.
3. If recall returns nothing relevant, say you don't have that information stored.

Use recall liberally — it's fast (~3ms) and free (local search, no API cost).
Your memory contains facts, preferences, decisions, and context from previous conversations."""
        allowed_tools = []
        mcp = True

    start = time.time()

    # Build options manually to control MCP
    from claude_agent_sdk import query as sdk_query, ClaudeAgentOptions
    import pathlib

    stderr_lines = []
    options = ClaudeAgentOptions(
        allowed_tools=allowed_tools,
        cwd=cwd,
        max_turns=5,  # Conversational — shouldn't need many turns
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
        from claude_agent_sdk import (
            AssistantMessage, ResultMessage, TextBlock, ToolUseBlock,
        )
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

    # Extract metrics
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

    # Check if agent admitted it doesn't know (valid for baseline)
    admits_no_knowledge = any(phrase in answer_lower for phrase in [
        "don't have", "don't know", "no information", "not aware",
        "cannot recall", "no memory", "no context", "not stored",
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
        "answer_preview": final_answer[:200],
        "error": error,
    }


async def main():
    print("Conversational Memory Benchmark")
    print("=" * 70)
    print(f"Scenarios: {len(SCENARIOS)}")
    print(f"Modes: baseline (no memory), ctxovrflw (MCP recall)")
    print(f"Total runs: {len(SCENARIOS) * 2}\n")

    # Health check
    try:
        resp = requests.get(f"{CTXOVRFLW_API_BASE}/health", timeout=5)
        print(f"Daemon: {resp.json().get('version', '?')} — {resp.json().get('status', '?')}\n")
    except:
        print("❌ Daemon not reachable\n")
        return

    # Seed all memories
    print("Seeding conversational memories...")
    clear_bench_memories()
    for s in SCENARIOS:
        ok = seed_memory(s.memory_to_seed)
        print(f"  {'✓' if ok else '✗'} {s.id}: {s.memory_to_seed['content'][:60]}...")
    print()

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

    # Summary
    print("=" * 70)
    print("SUMMARY")
    print("-" * 70)

    for mode in ["baseline", "ctxovrflw"]:
        mode_results = [r for r in results if r["mode"] == mode]
        avg_tools = sum(r["tool_calls"] for r in mode_results) / len(mode_results)
        avg_time = sum(r["elapsed_ms"] for r in mode_results) / len(mode_results)
        avg_tokens = sum(r["total_tokens"] for r in mode_results) / len(mode_results)
        avg_coverage = sum(r["keyword_coverage"] for r in mode_results) / len(mode_results)
        no_knowledge = sum(1 for r in mode_results if r["admits_no_knowledge"])

        print(f"  {mode:10s}: avg_tools={avg_tools:.1f}, avg_time={avg_time/1000:.1f}s, avg_tokens={avg_tokens:.0f}, coverage={avg_coverage:.0%}, no_knowledge={no_knowledge}/{len(mode_results)}")

    # Delta
    print()
    print("PER-SCENARIO DELTA (ctxovrflw - baseline):")
    print("-" * 70)
    for scenario in SCENARIOS:
        base = next(r for r in results if r["scenario_id"] == scenario.id and r["mode"] == "baseline")
        ctx = next(r for r in results if r["scenario_id"] == scenario.id and r["mode"] == "ctxovrflw")
        print(
            f"  {scenario.id:25s}: "
            f"tools {ctx['tool_calls']-base['tool_calls']:+2d}, "
            f"tokens {ctx['total_tokens']-base['total_tokens']:+6d}, "
            f"time {(ctx['elapsed_ms']-base['elapsed_ms'])/1000:+5.1f}s, "
            f"coverage {ctx['keyword_coverage']-base['keyword_coverage']:+.0%}"
        )

    # Save
    os.makedirs(RESULTS_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    outpath = os.path.join(RESULTS_DIR, f"conversational_{ts}.json")
    with open(outpath, "w") as f:
        json.dump({
            "benchmark": "conversational_memory",
            "completed_at": datetime.now().isoformat(),
            "scenarios": len(SCENARIOS),
            "results": results,
        }, f, indent=2)
    print(f"\nResults saved: {outpath}")


if __name__ == "__main__":
    asyncio.run(main())
