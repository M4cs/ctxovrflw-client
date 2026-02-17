"""
Claude Code platform adapter using claude-agent-sdk.
"""

import asyncio
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field

try:
    from claude_agent_sdk import (
        query, ClaudeAgentOptions,
        AssistantMessage, ResultMessage, SystemMessage, UserMessage,
        TextBlock, ToolUseBlock, ToolResultBlock,
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


@dataclass
class ClaudeCodeResult:
    """Result from Claude Code execution."""
    final_answer: str = ""
    tool_calls: List[Dict[str, Any]] = field(default_factory=list)
    input_tokens: int = 0
    output_tokens: int = 0
    elapsed_ms: float = 0.0
    total_cost_usd: float = 0.0
    num_turns: int = 0
    error: Optional[str] = None


class ClaudeCodePlatform:
    """Platform adapter for Claude Code via claude-agent-sdk."""

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
    ) -> ClaudeCodeResult:
        """Run a task using Claude Code SDK."""

        start_time = time.time()
        tool_calls = []
        text_parts = []
        result_data = None
        error = None

        try:
            stderr_lines = []
            def capture_stderr(line: str):
                stderr_lines.append(line)

            options = ClaudeAgentOptions(
                allowed_tools=allowed_tools or ["Read", "Bash", "Glob"],
                cwd=cwd or REPO_ROOT,
                max_turns=max_turns or BENCHMARK_CONFIG["max_turns"],
                permission_mode="bypassPermissions",
                debug_stderr=None,
                stderr=capture_stderr,
            )

            if system_prompt:
                options.system_prompt = system_prompt

            # Load MCP config from project .mcp.json if it exists
            import pathlib, json as _json
            mcp_json = pathlib.Path(cwd or REPO_ROOT) / ".mcp.json"
            if mcp_json.exists():
                mcp_cfg = _json.loads(mcp_json.read_text())
                servers = mcp_cfg.get("mcpServers", {})
                sdk_servers = {}
                for name, server_cfg in servers.items():
                    if "command" in server_cfg:
                        sdk_servers[name] = {
                            "type": "stdio",
                            "command": server_cfg["command"],
                            "args": server_cfg.get("args", []),
                        }
                    elif "url" in server_cfg:
                        sdk_servers[name] = {"type": "sse", "url": server_cfg["url"]}
                if sdk_servers:
                    options.mcp_servers = sdk_servers

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

        # Extract usage from ResultMessage
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
            # Prefer result text if available
            if result_data.result:
                final_answer = result_data.result
            if result_data.is_error:
                error = result_data.result or error

        if not final_answer:
            # Fall back to last substantial text block
            for part in reversed(text_parts):
                if part and len(part.strip()) > 20:
                    final_answer = part.strip()
                    break
            if not final_answer:
                final_answer = "\n".join(text_parts)

        return ClaudeCodeResult(
            final_answer=final_answer,
            tool_calls=tool_calls,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            elapsed_ms=elapsed_ms,
            total_cost_usd=total_cost,
            num_turns=num_turns,
            error=error,
        )

    def run_sync(self, *args, **kwargs) -> ClaudeCodeResult:
        """Synchronous wrapper for async run_task."""
        return asyncio.run(self.run_task(*args, **kwargs))
