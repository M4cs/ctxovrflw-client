# ctxovrflw Benchmark Suite

A comprehensive benchmark suite that demonstrates ctxovrflw's value for agentic coding tools by comparing performance across different context strategies and platforms.

## Overview

This benchmark tests **4 key metrics** that prove ctxovrflw's effectiveness:

1. **Tool Call Reduction** — Fewer Read/Bash/Glob calls needed when ctxovrflw has pre-indexed context
2. **Response Latency** — Time from prompt to final answer (first token or complete)  
3. **Token Efficiency** — Total input+output tokens per task completion
4. **Cross-Session Recall** — Ability to answer questions about work done in a previous session

## Architecture

### 3 Test Modes

- **Baseline** — Raw LLM with no pre-loaded context. Agent must discover everything via tools (Read, Bash, Glob) and store in session memory only.
- **Directed** — LLM given explicit file paths to read (simulates `@file` references). Context is loaded but not pre-summarized.
- **ctxovrflw** — LLM has access to ctxovrflw MCP tools (recall, remember). Memories pre-seeded from the codebase. Agent can search semantically.

### 2 Platforms  

- **Claude Code** — via `claude-agent-sdk` Python package
- **OpenClaw** — via REST API calls to the OpenClaw gateway

### 4 Test Categories

Inspired by MemoryAgentBench (ICLR 2026):

1. **Accurate Retrieval (AR)** — Find specific technical information
2. **Test-Time Learning (TTL)** — Cross-session memory retention  
3. **Long-Range Understanding (LRU)** — Connect information across multiple files
4. **Conflict Resolution (CR)** — Handle conflicting information and prefer recent/accurate data

## Installation

```bash
# Install dependencies
pip install -r requirements.txt

# Install Claude Agent SDK (for Claude Code platform)
pip install claude-agent-sdk

# Ensure OpenClaw CLI is available
claude --version

# Ensure ctxovrflw service is running
curl http://127.0.0.1:7437/health
```

## Quick Start

```bash
# Run quick benchmark (subset of scenarios)
python run.py --quick

# Run specific platform and mode
python run.py --platform claude --mode ctxovrflw

# Run full benchmark suite
python run.py --platform both --mode all

# Generate HTML report
python report.py results/benchmark_results_20240216_120000.json
```

## Usage

### Basic Usage

```bash
python run.py [OPTIONS]

Options:
  --platform {claude|openclaw|both}   Platform(s) to test (default: both)
  --mode {all|baseline|directed|ctxovrflw}  Test mode(s) (default: all)  
  --quick                             Run quick mode (3 scenarios vs 6)
  --output FILENAME                   Custom output filename
```

### Seeding ctxovrflw

Before running ctxovrflw mode, seed the memory system:

```bash
python seed_memories.py
```

This populates ctxovrflw with:
- Architecture decisions (encryption, search algorithms)
- Security model details (PIN derivation, zero-knowledge)
- Deployment process (CI platforms, scripts)
- Code structure (key files and their purposes)
- Conflict resolution test data

### Generating Reports

```bash
# Generate HTML report with charts
python report.py results/your_results.json

# Custom output location
python report.py results/your_results.json -o custom_report.html
```

## Test Scenarios

### Accurate Retrieval (AR)

**AR1: Encryption Details**
- Question: "What encryption algorithm does ctxovrflw use for sync? What are the PBKDF2 parameters?"
- Tests: Technical specification retrieval
- Expected: AES-256-GCM, PBKDF2 with 100,000 iterations, SHA-256

**AR2: Hybrid Search** 
- Question: "How does the hybrid search work? What fusion method is used?"
- Tests: Algorithm understanding
- Expected: Semantic + BM25 lexical search with RRF fusion (k=60)

**AR3: CI Platforms**
- Question: "What platforms does the CI build for? List all 5."
- Tests: Configuration parsing
- Expected: Linux x64/ARM64, Windows x64, macOS x64/ARM64

### Test-Time Learning (TTL)

**TTL1: Deployment Process**
- Phase 1: Tell agent about deploy script location and process
- Phase 2: Ask "How do I deploy a new version of ctxovrflw?"
- Tests: Cross-session memory retention
- Expected: Only ctxovrflw mode should succeed in Phase 2

### Long-Range Understanding (LRU)

**LRU1: Auth Flow**
- Question: "Trace the full auth flow from device code request to first encrypted sync. What are all the steps?"
- Tests: Information synthesis across multiple files
- Expected: Complete flow from OAuth → PIN → encryption → sync

### Conflict Resolution (CR)

**CR1: PIN Derivation**
- Setup: Seed conflicting memories (old: email salt vs new: server salt)
- Question: "How is the PIN encryption key derived?"  
- Tests: Preference for current/accurate information
- Expected: Should prefer v0.4.2 server-generated salt method

## Interpreting Results

### Key Metrics

- **Tool Calls**: Lower is better (ctxovrflw should reduce file system exploration)
- **Latency**: Lower is better (semantic search is faster than file reading)
- **Tokens**: Lower is better (avoid injecting large file contents)
- **Accuracy**: Higher is better (all modes should be similar, proving ctxovrflw doesn't sacrifice quality)

### Expected Outcomes

**ctxovrflw should demonstrate:**
- **50-80% reduction** in tool calls vs baseline
- **30-50% faster** response times vs baseline  
- **20-40% fewer** tokens used vs directed mode
- **Similar accuracy** to other modes (proving no quality loss)
- **Unique advantage** in cross-session recall (TTL scenarios)

### HTML Report

The generated HTML report includes:
- Interactive charts comparing all metrics across modes
- Detailed breakdowns by scenario and platform
- Performance interpretations and key insights
- Raw data export for further analysis

## Files Structure

```
benchmarks/
├── README.md              # This file
├── config.py              # Shared configuration
├── scenarios.py           # Test scenarios and ground truth
├── metrics.py             # Scoring and evaluation
├── platforms/             # Platform adapters
│   ├── claude_code.py     # Claude Agent SDK integration
│   └── openclaw.py        # OpenClaw CLI/API integration
├── modes/                 # Test mode implementations  
│   ├── baseline.py        # No context mode
│   ├── directed.py        # File-directed context mode
│   └── ctxovrflw.py       # Semantic memory mode
├── run.py                 # Main benchmark runner
├── report.py              # HTML report generator
├── seed_memories.py       # ctxovrflw memory seeding
└── results/               # Output directory
```

## Requirements

- Python 3.8+
- `claude-agent-sdk` (for Claude Code platform)
- OpenClaw CLI and gateway (for OpenClaw platform)
- ctxovrflw service running on localhost:7437
- Environment variables: `ANTHROPIC_API_KEY` (required), `OPENROUTER_API_KEY` (optional)

## API Keys

- **ANTHROPIC_API_KEY**: Required for Claude Code platform and LLM-as-judge scoring
- **OPENROUTER_API_KEY**: Optional alternative for LLM-as-judge scoring

Without API keys, benchmarks will still run but with limited scoring capabilities.

## Troubleshooting

### Common Issues

**"claude-agent-sdk not available"**
```bash
pip install claude-agent-sdk
export ANTHROPIC_API_KEY="your_key_here"
```

**"OpenClaw CLI not available"**
```bash
# Install OpenClaw CLI
npm install -g @openclaw/cli
# Or check if it's in PATH
which claude
```

**"ctxovrflw service not healthy"**
```bash
# Check if ctxovrflw is running
curl http://127.0.0.1:7437/health

# Start ctxovrflw if needed
ctxovrflw daemon start
```

**"No results generated"**
- Check that at least one platform is available
- Verify API keys are set correctly
- Check ctxovrflw service status for ctxovrflw mode
- Review error messages in console output

## Contributing

This benchmark suite is designed to be extended with additional scenarios and metrics. To add new test cases:

1. Add scenarios to `scenarios.py`
2. Update ground truth data
3. Add memory seeds if needed in `seed_memories.py`
4. Test with `python run.py --quick`

## License

Part of the ctxovrflw project. See main project license.