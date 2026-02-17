"""
Tier comparison mode — tests ctxovrflw recall quality across Free/Standard/Pro tiers.

Free:     keyword search only (FTS5)
Standard: semantic search (ONNX embeddings + sqlite-vec)
Pro:      hybrid search (semantic + FTS5 + Reciprocal Rank Fusion)

This mode directly queries ctxovrflw with different search methods
and measures recall relevance, then feeds the results to the LLM.
"""

import requests
import time
from typing import Optional, List, Dict, Any

try:
    from ..config import CTXOVRFLW_API_BASE, REPO_ROOT
except ImportError:
    import sys, os
    sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from config import CTXOVRFLW_API_BASE, REPO_ROOT


SEARCH_METHODS = {
    "free": "keyword",
    "standard": "semantic", 
    "pro": "hybrid",
}


class TierTestMode:
    """Tests ctxovrflw recall quality at each tier's search method."""

    def __init__(self):
        self.api_base = CTXOVRFLW_API_BASE

    def recall_at_tier(self, query: str, tier: str, limit: int = 5) -> Dict[str, Any]:
        """Run a recall query using a specific search method (simulating tier).
        
        Returns recall results + timing.
        """
        method = SEARCH_METHODS.get(tier, "hybrid")
        
        start = time.time()
        try:
            response = requests.post(
                f"{self.api_base}/v1/memories/recall",
                json={
                    "query": query,
                    "limit": limit,
                    "search_method": method,
                },
                timeout=10,
                headers={"Content-Type": "application/json"},
            )
            elapsed_ms = (time.time() - start) * 1000

            if response.status_code != 200:
                return {
                    "tier": tier,
                    "method": method,
                    "results": [],
                    "elapsed_ms": elapsed_ms,
                    "error": f"HTTP {response.status_code}",
                }

            data = response.json()
            results = data.get("results", [])
            actual_method = data.get("search_method", method)

            return {
                "tier": tier,
                "method": actual_method,
                "results": results,
                "result_count": len(results),
                "elapsed_ms": elapsed_ms,
                "avg_score": (
                    sum(r.get("score", 0) for r in results) / len(results)
                    if results else 0
                ),
                "top_score": results[0].get("score", 0) if results else 0,
                "contents": [
                    r.get("memory", {}).get("content", "")[:100]
                    for r in results
                ],
                "error": None,
            }

        except Exception as e:
            return {
                "tier": tier,
                "method": method,
                "results": [],
                "elapsed_ms": (time.time() - start) * 1000,
                "error": str(e),
            }

    def run_tier_comparison(self, query: str, limit: int = 5) -> Dict[str, Any]:
        """Run the same query across all three tiers and compare."""
        comparison = {}
        for tier in ["free", "standard", "pro"]:
            comparison[tier] = self.recall_at_tier(query, tier, limit)
        return comparison

    def format_recall_as_context(self, recall_result: Dict[str, Any]) -> str:
        """Format recall results as context to inject into LLM prompt."""
        results = recall_result.get("results", [])
        if not results:
            return "(no relevant memories found)"
        
        lines = []
        for r in results:
            memory = r.get("memory", {})
            content = memory.get("content", "")
            score = r.get("score", 0)
            lines.append(f"[relevance: {score:.3f}] {content}")
        return "\n".join(lines)

    def prepare_system_prompt(self, tier: str, recall_context: str) -> str:
        """Prepare system prompt with tier-specific recall context."""
        tier_labels = {
            "free": "Free tier (keyword search only)",
            "standard": "Standard tier (semantic search with ONNX embeddings)",
            "pro": "Pro tier (hybrid search: semantic + keyword + RRF fusion)",
        }
        
        return f"""You are answering questions about the ctxovrflw codebase.

You have access to memories retrieved via ctxovrflw ({tier_labels.get(tier, tier)}):

--- Retrieved Context ---
{recall_context}
--- End Context ---

Answer the question using ONLY the retrieved context above. If the context doesn't contain enough information, say so.
Do NOT read files or use tools — rely solely on the retrieved memories.
Repository location: {REPO_ROOT}"""

    def get_allowed_tools(self) -> List[str]:
        """Tier test uses no tools — pure recall quality measurement."""
        return []

    def get_working_directory(self) -> str:
        return REPO_ROOT
