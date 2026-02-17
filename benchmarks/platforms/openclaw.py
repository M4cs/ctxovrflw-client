"""
OpenClaw platform adapter.

OpenClaw agents have workspace context (AGENTS.md, MEMORY.md, memory/*.md)
injected automatically. We simulate this by adding the OpenClaw preamble
to the system prompt.

Uses the same Claude Agent SDK under the hood for consistent measurement.
"""

import asyncio
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field

try:
    from claude_agent_sdk import (
        query, ClaudeAgentOptions,
        AssistantMessage, ResultMessage,
        TextBlock, ToolUseBlock,
    )
    CLAUDE_SDK_AVAILABLE = True
except ImportError:
    CLAUDE_SDK_AVAILABLE = False

try:
    from ..config import BENCHMARK_CONFIG, REPO_ROOT
except ImportError:
    import sys, os
    sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from config import BENCHMARK_CONFIG, REPO_ROOT


OPENCLAW_PREAMBLE = """You are an AI assistant running inside OpenClaw, a personal AI gateway.
You have access to workspace files including AGENTS.md, MEMORY.md, and memory/*.md daily logs.
You should check memory files for prior context before exploring the codebase.
Your working directory is the project repository.
"""


@dataclass
class OpenClawResult:
    """Result from OpenClaw execution."""
    final_answer: str = ""
    tool_calls: List[Dict[str, Any]] = field(default_factory=list)
    input_tokens: int = 0
    output_tokens: int = 0
    elapsed_ms: float = 0.0
    total_cost_usd: float = 0.0
    num_turns: int = 0
    error: Optional[str] = None


class OpenClawPlatform:
    """Platform adapter simulating OpenClaw-style agent execution."""

    def __init__(self):
        if not CLAUDE_SDK_AVAILABLE:
            raise ImportError(
                "claude-agent-sdk not available. Install with: pip install claude-agent-sdk"
            )

    async def run_task(
        self,
        prompt: str,
        system_prompt: Optional[str] = None,
        allowed_tools: Optional[List[str]] = None,
        max_turns: Optional[int] = None,
        cwd: Optional[str] = None,
    ) -> OpenClawResult:
        """Run a task simulating OpenClaw agent behavior."""

        start_time = time.time()
        tool_calls = []
        text_parts = []
        result_data = None
        error = None

        full_system = OPENCLAW_PREAMBLE
        if system_prompt:
            full_system += "\n" + system_prompt

        try:
            stderr_lines = []
            def capture_stderr(line: str):
                stderr_lines.append(line)

            options = ClaudeAgentOptions(
                allowed_tools=allowed_tools or ["Read", "Bash", "Glob"],
                cwd=cwd or REPO_ROOT,
                max_turns=max_turns or BENCHMARK_CONFIG["max_turns"],
                permission_mode="bypassPermissions",
                system_prompt=full_system,
                debug_stderr=None,
                stderr=capture_stderr,
            )

            async for msg in query(prompt=prompt, options=options):
                if isinstance(msg, AssistantMessage):
                    for block in (msg.content or []):
                        if isinstance(block, TextBlock):
                            text_parts.append(block.text)
                        elif isinstance(block, ToolUseBlock):
                            tool_calls.append({
                                "id": block.id,
                                "name": block.name,
                                "input": block.input,
                            })
                elif isinstance(msg, ResultMessage):
                    result_data = msg

        except Exception as e:
            error = str(e)
            import traceback
            print(f"  ⚠ SDK error: {e}")
            if stderr_lines:
                print(f"  ⚠ Claude stderr: {''.join(stderr_lines[-5:])}")
            traceback.print_exc()

        elapsed_ms = (time.time() - start_time) * 1000

        input_tokens = 0
        output_tokens = 0
        total_cost = 0.0
        num_turns = 0
        final_answer = ""

        if result_data:
            usage = result_data.usage or {}
            input_tokens = usage.get("input_tokens", 0)
            input_tokens += usage.get("cache_read_input_tokens", 0)
            input_tokens += usage.get("cache_creation_input_tokens", 0)
            output_tokens = usage.get("output_tokens", 0)
            total_cost = result_data.total_cost_usd or 0.0
            num_turns = result_data.num_turns or 0
            if result_data.result:
                final_answer = result_data.result
            if result_data.is_error:
                error = result_data.result or error

        if not final_answer:
            for part in reversed(text_parts):
                if part and len(part.strip()) > 20:
                    final_answer = part.strip()
                    break
            if not final_answer:
                final_answer = "\n".join(text_parts)

        return OpenClawResult(
            final_answer=final_answer,
            tool_calls=tool_calls,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            elapsed_ms=elapsed_ms,
            total_cost_usd=total_cost,
            num_turns=num_turns,
            error=error,
        )

    def run_sync(self, *args, **kwargs) -> OpenClawResult:
        return asyncio.run(self.run_task(*args, **kwargs))
