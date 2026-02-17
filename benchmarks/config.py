"""
Configuration settings for ctxovrflw benchmark suite.
"""
import os
from typing import Dict, Any

# API Configuration
ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY")
OPENROUTER_API_KEY = os.environ.get("OPENROUTER_API_KEY")

# ctxovrflw Daemon Configuration
CTXOVRFLW_API_BASE = "http://127.0.0.1:7437"
CTXOVRFLW_MCP_URL = "http://127.0.0.1:7437/mcp/sse"

# Model Configuration
DEFAULT_MODELS = {
    "claude_code": "claude-sonnet-4-20250514",
    "openclaw": "anthropic/claude-sonnet-4-20250514",
    "openrouter": "anthropic/claude-sonnet-4-20250514",
}

# Benchmark Configuration
BENCHMARK_CONFIG = {
    "max_turns": 20,
    "timeout_seconds": 120,
    "max_retries": 3,
    "cooldown_seconds": 5,
}

# Tool Lists
BASELINE_TOOLS = ["Read", "Bash", "Glob", "Edit", "Write"]
DIRECTED_TOOLS = ["Read", "Bash", "Glob", "Edit", "Write"]  # Same as baseline
CTXOVRFLW_TOOLS = ["Read", "Bash", "Glob"]  # File tools as fallback only; recall pre-injected

# Scoring Configuration
SCORING_CONFIG = {
    "keyword_weight": 0.4,
    "llm_judge_weight": 0.6,
    "llm_judge_model": "gpt-4",
    "keyword_match_threshold": 0.7,
}

# Repository Paths
REPO_ROOT = "/home/max/.openclaw/workspace/ctxovrflw"
BENCHMARK_ROOT = "/home/max/.openclaw/workspace/ctxovrflw/benchmarks"
RESULTS_DIR = f"{BENCHMARK_ROOT}/results"
TEMPLATES_DIR = f"{BENCHMARK_ROOT}/templates"

# Test Configuration
TEST_MODES = ["baseline", "directed", "ctxovrflw"]
PLATFORMS = ["claude", "openclaw"]

def get_api_key(service: str) -> str:
    """Get API key for specified service."""
    if service == "anthropic":
        return ANTHROPIC_API_KEY
    elif service == "openrouter":
        return OPENROUTER_API_KEY
    else:
        raise ValueError(f"Unknown service: {service}")

def get_model_name(platform: str) -> str:
    """Get default model name for platform."""
    return DEFAULT_MODELS.get(platform, DEFAULT_MODELS["claude"])

def get_tools_for_mode(mode: str) -> list:
    """Get tool list for specified mode."""
    if mode == "baseline":
        return BASELINE_TOOLS
    elif mode == "directed":
        return DIRECTED_TOOLS
    elif mode == "ctxovrflw":
        return CTXOVRFLW_TOOLS
    else:
        raise ValueError(f"Unknown mode: {mode}")