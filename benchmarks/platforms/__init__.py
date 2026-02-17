"""
Platform adapters for running benchmarks on different AI platforms.
"""

from .claude_code import ClaudeCodePlatform
from .openclaw import OpenClawPlatform

__all__ = ["ClaudeCodePlatform", "OpenClawPlatform"]