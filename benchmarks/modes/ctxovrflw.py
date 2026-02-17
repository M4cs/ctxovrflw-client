"""
ctxovrflw mode — Agent has native MCP access to ctxovrflw tools.

Claude Code reads .mcp.json from the project root and connects to
ctxovrflw's MCP server automatically. The agent can call recall,
remember, etc. as native tools — no mcporter needed.

The system prompt instructs the agent to use recall FIRST before
exploring files. This tests the real-world experience.
"""

import requests
from typing import Optional, List, Dict, Any

try:
    from ..config import REPO_ROOT, CTXOVRFLW_API_BASE
except ImportError:
    import sys, os
    sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from config import REPO_ROOT, CTXOVRFLW_API_BASE


class CtxovrflwMode:
    """ctxovrflw mode — agent uses MCP recall tools naturally."""

    def __init__(self):
        self.mode_name = "ctxovrflw"
        self.api_base = CTXOVRFLW_API_BASE

    def prepare_system_prompt(self, base_prompt: Optional[str] = None) -> str:
        """System prompt that instructs recall-first workflow."""

        prompt = f"""You are an AI assistant answering questions about the ctxovrflw codebase.

Repository location: {REPO_ROOT}

You have access to ctxovrflw semantic memory via MCP tools. Your workflow should be:

1. FIRST: Use the ctxovrflw recall tool to search your memory for relevant information.
   Try multiple queries if the first doesn't return good results.
2. THEN: If recall gives you a confident answer, respond immediately. Do NOT verify via files unless the recall results are clearly incomplete or contradictory.
3. ONLY IF NEEDED: Fall back to reading files (Read, Bash, Glob) for details not covered by memory.

The recall tool searches across all stored memories using semantic similarity.
It's fast (~3ms) and should be your first instinct for any question.

Be thorough in your answer but efficient in your tool usage. Memory recall should handle most questions without needing to read files."""

        if base_prompt:
            return f"{base_prompt}\n\n{prompt}"
        return prompt

    def get_allowed_tools(self) -> List[str]:
        """File tools available as fallback. MCP tools are automatic via .mcp.json."""
        return ["Read", "Bash", "Glob"]

    def get_working_directory(self) -> str:
        return REPO_ROOT

    def supports_cross_session_memory(self) -> bool:
        return True

    def prepare_context(self, scenario_id: str) -> Optional[str]:
        return None

    def seed_memories(self, memories: List[Dict[str, Any]]) -> bool:
        """Seed ctxovrflw with test memories."""
        success = 0
        for mem in memories:
            try:
                resp = requests.post(
                    f"{self.api_base}/v1/memories",
                    json=mem, timeout=10,
                    headers={"Content-Type": "application/json"},
                )
                if resp.status_code in [200, 201]:
                    success += 1
            except Exception as e:
                print(f"  Seed error: {e}")
        print(f"Seeded {success}/{len(memories)} memories")
        return success == len(memories)

    def clear_test_memories(self) -> bool:
        """Clear test memories."""
        try:
            resp = requests.post(
                f"{self.api_base}/v1/memories/recall",
                json={"query": "test benchmark", "limit": 100},
                timeout=10,
            )
            if resp.status_code != 200:
                return False
            results = resp.json().get("results", [])
            deleted = 0
            for r in results:
                mid = r.get("memory", {}).get("id")
                if mid:
                    try:
                        requests.delete(f"{self.api_base}/v1/memories/{mid}", timeout=10)
                        deleted += 1
                    except:
                        pass
            print(f"Deleted {deleted} test memories")
            return True
        except Exception as e:
            print(f"Clear error: {e}")
            return False

    def check_ctxovrflw_status(self) -> Dict[str, Any]:
        try:
            resp = requests.get(f"{self.api_base}/health", timeout=5)
            if resp.status_code == 200:
                return {"status": "healthy", "details": resp.json()}
            return {"status": "unhealthy", "code": resp.status_code}
        except Exception as e:
            return {"status": "error", "error": str(e)}

    def get_description(self) -> str:
        return (
            "ctxovrflw mode: Agent has native MCP access to ctxovrflw recall/remember tools. "
            "Prompted to use recall first, file tools as fallback. Tests real-world usage."
        )
