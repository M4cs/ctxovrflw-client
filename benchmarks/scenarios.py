"""
Test scenarios for ctxovrflw benchmark suite.

Based on MemoryAgentBench ICLR 2026 methodology with ctxovrflw-specific adaptations.
"""

from typing import Dict, List, Any, Optional
from dataclasses import dataclass

@dataclass
class GroundTruth:
    """Ground truth data for scoring."""
    keywords: List[str]  # Required keywords/phrases
    description: str     # Human-readable description
    points: int         # Maximum points for this question

@dataclass
class TestScenario:
    """A single test scenario."""
    id: str
    category: str
    question: str
    ground_truth: GroundTruth
    setup_instructions: Optional[str] = None  # For TTL and CR scenarios
    previous_session_context: Optional[str] = None  # For TTL scenarios

# Category 1: Accurate Retrieval (AR)
# Tests the agent's ability to find and extract specific technical information

AR_SCENARIOS = [
    TestScenario(
        id="ar_1_encryption",
        category="Accurate Retrieval",
        question="What encryption algorithm does ctxovrflw use for sync? What are the PBKDF2 parameters?",
        ground_truth=GroundTruth(
            keywords=[
                "AES-256-GCM", "PBKDF2",
                "600,000", "SHA-256",
                "salt", "encryption",
                "ctxovrflw-zk-v1-",
            ],
            description="Should identify AES-256-GCM encryption and PBKDF2 with 600,000 iterations using SHA-256 with server-generated salt prefix ctxovrflw-zk-v1-",
            points=10
        )
    ),
    
    TestScenario(
        id="ar_2_hybrid_search",
        category="Accurate Retrieval", 
        question="How does the hybrid search work? What fusion method is used?",
        ground_truth=GroundTruth(
            keywords=[
                "hybrid", "semantic", "lexical",
                "BM25", "embeddings",
                "Reciprocal Rank Fusion", "RRF",
                "k=60",
            ],
            description="Should explain hybrid search combining semantic embeddings with BM25 lexical search using RRF fusion with k=60",
            points=10
        )
    ),
    
    TestScenario(
        id="ar_3_ci_platforms",
        category="Accurate Retrieval",
        question="What platforms does the CI build for? List all 5.",
        ground_truth=GroundTruth(
            keywords=[
                "Linux x64", "Linux ARM64",
                "Windows x64", "macOS x64", "macOS ARM64",
            ],
            description="Should identify all 5 target platforms exactly as stored: Linux x64, Linux ARM64, Windows x64, macOS x64, macOS ARM64",
            points=10
        )
    )
]

# Category 2: Test-Time Learning (TTL)
# Tests cross-session memory retention

TTL_SCENARIOS = [
    TestScenario(
        id="ttl_1_deploy",
        category="Test-Time Learning",
        question="How do I deploy a new version of ctxovrflw?",
        setup_instructions="Tell the agent: The deploy script is at scripts/deploy.sh. It syncs to a public repo M4cs/ctxovrflw-client, tags, and triggers CI.",
        previous_session_context="The deploy script is at scripts/deploy.sh. It syncs to a public repo M4cs/ctxovrflw-client, tags, and triggers CI.",
        ground_truth=GroundTruth(
            keywords=[
                "scripts/deploy.sh", "deploy", "script",
                "M4cs/ctxovrflw-client", "public", "repo",
                "tag", "CI", "trigger"
            ],
            description="Should recall deployment process from previous session context",
            points=10
        )
    )
]

# Category 3: Long-Range Understanding (LRU) 
# Tests ability to connect information across multiple files

LRU_SCENARIOS = [
    TestScenario(
        id="lru_1_auth_flow", 
        category="Long-Range Understanding",
        question="Trace the full auth flow from device code request to first encrypted sync. What are all the steps?",
        ground_truth=GroundTruth(
            keywords=[
                "device code", "OAuth", "token",
                "PIN", "encryption", "key derivation",
                "encrypted sync", "upload",
            ],
            description="Should trace: device code request → OAuth token exchange → PIN-based encryption key derivation → encrypted sync",
            points=15
        )
    )
]

# Category 4: Conflict Resolution (CR)
# Tests ability to handle conflicting information and prefer recent/accurate data

CR_SCENARIOS = [
    TestScenario(
        id="cr_1_pin_derivation",
        category="Conflict Resolution", 
        question="How is the PIN encryption key derived?",
        setup_instructions="Seed conflicting memories: old (email salt) vs new (server salt)",
        ground_truth=GroundTruth(
            keywords=[
                "server-generated", "random", "salt",
                "PIN", "encryption", "key",
                "v0.4.2", "current",
            ],
            description="Should prefer current method (server-generated random salt, v0.4.2) over outdated (email salt)",
            points=10
        )
    )
]

# Category 5: Tier Comparison
# Tests the difference between Free (keyword), Standard (semantic), and Pro (hybrid) search
# These use the SAME questions but measure recall quality at different search tiers

TIER_SCENARIOS = [
    TestScenario(
        id="tier_1_vague_query",
        category="Tier Comparison",
        question="How does the project handle security for data at rest?",
        ground_truth=GroundTruth(
            keywords=[
                "AES-256-GCM", "encryption", "PBKDF2",
                "zero-knowledge", "encrypted data",
                "salt", "PIN",
            ],
            description="Vague query — should find encryption + zero-knowledge memories despite no keyword match for 'data at rest'. Semantic should outperform keyword.",
            points=10
        )
    ),
    TestScenario(
        id="tier_2_conceptual_query",
        category="Tier Comparison",
        question="What prevents the cloud service from reading user data?",
        ground_truth=GroundTruth(
            keywords=[
                "zero-knowledge", "cannot decrypt",
                "encrypted", "encrypt",
                "PIN", "key derivation",
                "server",
            ],
            description="Conceptual query — keyword search for 'reading user data' won't match 'zero-knowledge'. Semantic should excel.",
            points=10
        )
    ),
    TestScenario(
        id="tier_3_multi_concept",
        category="Tier Comparison",
        question="How does the system ensure data integrity and consistency across multiple devices?",
        ground_truth=GroundTruth(
            keywords=[
                "sync", "incremental", "conflict",
                "encrypted", "upload", "download",
                "resolution",
            ],
            description="Multi-concept query spanning sync + encryption + conflict resolution. Hybrid should combine keyword 'sync' with semantic 'data integrity'.",
            points=10
        )
    ),
]

# All scenarios combined
ALL_SCENARIOS = AR_SCENARIOS + TTL_SCENARIOS + LRU_SCENARIOS + CR_SCENARIOS + TIER_SCENARIOS

# Quick mode subset (for faster testing)
QUICK_SCENARIOS = [
    AR_SCENARIOS[0],  # ar_1_encryption
    TTL_SCENARIOS[0], # ttl_1_deploy  
    CR_SCENARIOS[0]   # cr_1_pin_derivation
]

def get_scenarios(quick_mode: bool = False) -> List[TestScenario]:
    """Get test scenarios based on mode."""
    return QUICK_SCENARIOS if quick_mode else ALL_SCENARIOS

def get_scenario_by_id(scenario_id: str) -> TestScenario:
    """Get specific scenario by ID."""
    for scenario in ALL_SCENARIOS:
        if scenario.id == scenario_id:
            return scenario
    raise ValueError(f"Scenario not found: {scenario_id}")

def get_scenarios_by_category(category: str) -> List[TestScenario]:
    """Get all scenarios in a specific category."""
    return [s for s in ALL_SCENARIOS if s.category == category]

# Memory seeds for conflict resolution testing
CR_MEMORY_SEEDS = {
    "cr_1_pin_derivation": [
        {
            "content": "PIN key derivation uses email as salt",
            "type": "semantic",
            "tags": ["auth", "PIN", "outdated"],
            "subject": "encryption"
        },
        {
            "content": "PIN key derivation uses server-generated random salt (v0.4.2)",
            "type": "semantic", 
            "tags": ["auth", "PIN", "current", "v0.4.2"],
            "subject": "encryption"
        }
    ]
}

def get_memory_seeds_for_scenario(scenario_id: str) -> List[Dict[str, Any]]:
    """Get memory seeds for conflict resolution scenarios."""
    return CR_MEMORY_SEEDS.get(scenario_id, [])