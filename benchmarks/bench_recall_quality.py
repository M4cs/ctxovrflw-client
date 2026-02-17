#!/usr/bin/env python3
"""
Phase 1: Direct recall quality benchmark.

No LLM involved. Tests whether ctxovrflw returns relevant results
for each scenario question at each tier (keyword/semantic/hybrid).

Measures:
- Precision: Do returned results contain ground truth keywords?
- Coverage: What % of ground truth keywords appear in results?
- Latency: How fast is each search method?
- Result count: How many results does each method return?
"""

import sys
import time
import json
import requests
from typing import Dict, List, Any
from datetime import datetime

sys.stdout.reconfigure(line_buffering=True)

from config import CTXOVRFLW_API_BASE, RESULTS_DIR
from scenarios import get_scenarios

TIERS = {
    "free": "keyword",
    "standard": "semantic",
    "pro": "hybrid",
}


def recall(query: str, method: str, limit: int = 8) -> Dict[str, Any]:
    """Execute a recall query with a specific search method."""
    start = time.time()
    try:
        resp = requests.post(
            f"{CTXOVRFLW_API_BASE}/v1/memories/recall",
            json={"query": query, "limit": limit, "search_method": method},
            timeout=10,
            headers={"Content-Type": "application/json"},
        )
        elapsed_ms = (time.time() - start) * 1000
        if resp.status_code == 200:
            data = resp.json()
            return {
                "method": data.get("search_method", method),
                "results": data.get("results", []),
                "elapsed_ms": elapsed_ms,
                "error": None,
            }
        return {"method": method, "results": [], "elapsed_ms": elapsed_ms, "error": f"HTTP {resp.status_code}"}
    except Exception as e:
        return {"method": method, "results": [], "elapsed_ms": (time.time() - start) * 1000, "error": str(e)}


def score_recall(results: List[Dict], ground_truth_keywords: List[str]) -> Dict[str, Any]:
    """Score recall results against ground truth keywords."""
    if not results:
        return {"coverage": 0.0, "hit_keywords": [], "missed_keywords": ground_truth_keywords, "result_count": 0}

    # Combine all result content into one blob
    combined = " ".join(
        r.get("memory", {}).get("content", "").lower()
        for r in results
    )

    hits = []
    misses = []
    for kw in ground_truth_keywords:
        if kw.lower() in combined:
            hits.append(kw)
        else:
            misses.append(kw)

    coverage = len(hits) / len(ground_truth_keywords) if ground_truth_keywords else 0.0

    return {
        "coverage": coverage,
        "hit_keywords": hits,
        "missed_keywords": misses,
        "result_count": len(results),
        "top_score": results[0].get("score", 0) if results else 0,
    }


def run_benchmark(quick: bool = False):
    """Run Phase 1 recall quality benchmark."""
    print("Phase 1: Direct Recall Quality Benchmark")
    print("=" * 70)
    print("No LLM involved — testing search quality directly.\n")

    # Health check
    try:
        resp = requests.get(f"{CTXOVRFLW_API_BASE}/health", timeout=5)
        health = resp.json()
        print(f"Daemon: {health.get('version', '?')} — {health.get('status', '?')}")
    except Exception as e:
        print(f"❌ Daemon not reachable: {e}")
        return

    scenarios = get_scenarios(quick)
    all_results = []

    for scenario in scenarios:
        print(f"\n📋 {scenario.id} ({scenario.category})")
        print(f"   Q: {scenario.question}")
        print(f"   Keywords: {', '.join(scenario.ground_truth.keywords[:5])}...")

        scenario_results = {}
        for tier, method in TIERS.items():
            data = recall(scenario.question, method)
            scores = score_recall(data["results"], scenario.ground_truth.keywords)

            scenario_results[tier] = {
                "method": data["method"],
                "elapsed_ms": data["elapsed_ms"],
                "error": data["error"],
                **scores,
            }

            emoji = "✅" if scores["coverage"] >= 0.7 else "⚠️" if scores["coverage"] >= 0.4 else "❌"
            print(
                f"   {emoji} {tier:10s} ({data['method']:8s}): "
                f"coverage={scores['coverage']:.0%}, "
                f"results={scores['result_count']}, "
                f"top_score={scores['top_score']:.3f}, "
                f"{data['elapsed_ms']:.0f}ms"
            )
            if scores["missed_keywords"]:
                print(f"      Missed: {', '.join(scores['missed_keywords'][:5])}")

        all_results.append({
            "scenario_id": scenario.id,
            "category": scenario.category,
            "question": scenario.question,
            "ground_truth_keywords": scenario.ground_truth.keywords,
            "tiers": scenario_results,
        })

    # Summary
    print("\n" + "=" * 70)
    print("SUMMARY BY TIER")
    print("-" * 70)

    for tier in TIERS:
        coverages = [r["tiers"][tier]["coverage"] for r in all_results]
        latencies = [r["tiers"][tier]["elapsed_ms"] for r in all_results]
        avg_cov = sum(coverages) / len(coverages)
        avg_lat = sum(latencies) / len(latencies)
        print(f"  {tier:10s}: avg_coverage={avg_cov:.0%}, avg_latency={avg_lat:.0f}ms")

    # Save
    import os
    os.makedirs(RESULTS_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    outpath = os.path.join(RESULTS_DIR, f"recall_quality_{ts}.json")
    with open(outpath, "w") as f:
        json.dump({
            "phase": "recall_quality",
            "completed_at": datetime.now().isoformat(),
            "scenario_count": len(all_results),
            "tiers": list(TIERS.keys()),
            "results": all_results,
        }, f, indent=2)

    print(f"\nResults saved: {outpath}")


if __name__ == "__main__":
    quick = "--quick" in sys.argv
    run_benchmark(quick)
