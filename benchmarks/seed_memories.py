#!/usr/bin/env python3
"""
Seed ctxovrflw with test memories extracted from the codebase.

This script pre-populates ctxovrflw with key architectural and technical information
to test the semantic memory capabilities in benchmark scenarios.
"""

import json
import requests
import time
import sys
from typing import List, Dict, Any
from config import CTXOVRFLW_API_BASE

def seed_architecture_memories() -> List[Dict[str, Any]]:
    """Architecture and design decision memories."""
    return [
        {
            "content": "ctxovrflw uses AES-256-GCM for encryption with PBKDF2 key derivation using 600,000 iterations and SHA-256. Salt prefix is 'ctxovrflw-zk-v1-' prepended to server-generated random salt before PBKDF2.",
            "type": "semantic",
            "tags": ["architecture", "security", "encryption", "AES", "PBKDF2"],
            "subject": "encryption"
        },
        {
            "content": "Hybrid search combines semantic embeddings with BM25 lexical search using Reciprocal Rank Fusion (RRF) with k=60",
            "type": "semantic", 
            "tags": ["architecture", "search", "hybrid", "RRF", "BM25", "semantic"],
            "subject": "search"
        },
        {
            "content": "Authentication flow: device code request → OAuth token exchange → PIN-based encryption key derivation → encrypted sync",
            "type": "semantic",
            "tags": ["architecture", "auth", "oauth", "device-code", "PIN"],
            "subject": "authentication"
        },
        {
            "content": "Sync protocol encrypts memories locally before upload using derived PIN key, supports incremental sync with conflict resolution",
            "type": "semantic",
            "tags": ["architecture", "sync", "encryption", "incremental", "conflicts"],
            "subject": "sync"
        }
    ]

def seed_security_memories() -> List[Dict[str, Any]]:
    """Security model and implementation memories."""
    return [
        {
            "content": "PIN encryption key derivation uses server-generated random salt (v0.4.2 current method)",
            "type": "semantic",
            "tags": ["security", "PIN", "salt", "v0.4.2", "current"],
            "subject": "encryption"
        },
        {
            "content": "Old PIN key derivation used email as salt (pre-v0.4.2, deprecated for security)", 
            "type": "semantic",
            "tags": ["security", "PIN", "email", "salt", "deprecated", "outdated"],
            "subject": "encryption"
        },
        {
            "content": "Zero-knowledge architecture: server cannot decrypt user memories, only stores encrypted data",
            "type": "semantic",
            "tags": ["security", "zero-knowledge", "privacy", "encryption"],
            "subject": "privacy"
        },
        {
            "content": "OAuth scopes: read:memories, write:memories, delete:memories for granular access control",
            "type": "semantic",
            "tags": ["security", "oauth", "scopes", "access-control"],
            "subject": "authorization"
        }
    ]

def seed_deployment_memories() -> List[Dict[str, Any]]:
    """Deployment and CI/CD memories."""
    return [
        {
            "content": "CI builds for 5 platforms: Linux x64, Linux ARM64, Windows x64, macOS x64, macOS ARM64",
            "type": "semantic",
            "tags": ["deployment", "CI", "platforms", "linux", "windows", "macos", "arm64"],
            "subject": "build"
        },
        {
            "content": "Release workflow uses GitHub Actions with matrix builds for cross-platform binaries",
            "type": "semantic", 
            "tags": ["deployment", "github-actions", "matrix", "cross-platform"],
            "subject": "ci"
        },
        {
            "content": "Deployment script at scripts/deploy.sh syncs to public repo M4cs/ctxovrflw-client and triggers CI",
            "type": "semantic",
            "tags": ["deployment", "script", "sync", "public-repo", "trigger"],
            "subject": "deploy"
        }
    ]

def seed_code_structure_memories() -> List[Dict[str, Any]]:
    """Code structure and key files memories."""
    return [
        {
            "content": "Key authentication files: src/device-auth.ts (device flow), src/auth.ts (tokens), src/login.rs (PIN)",
            "type": "semantic",
            "tags": ["code", "auth", "files", "device-auth", "login"],
            "subject": "codebase"
        },
        {
            "content": "Crypto implementation in src/crypto/mod.rs with encryption, key derivation, and PIN handling",
            "type": "semantic",
            "tags": ["code", "crypto", "files", "encryption", "keys"],
            "subject": "codebase"
        },
        {
            "content": "Search implementation in src/search/ with hybrid.rs, semantic.rs, and lexical.rs modules",
            "type": "semantic",
            "tags": ["code", "search", "files", "hybrid", "semantic", "lexical"],
            "subject": "codebase"
        },
        {
            "content": "Sync logic in src/sync/mod.rs handles encrypted upload/download and conflict resolution",
            "type": "semantic",
            "tags": ["code", "sync", "files", "upload", "download", "conflicts"],
            "subject": "codebase"
        }
    ]

def seed_conflict_memories() -> List[Dict[str, Any]]:
    """Memories for conflict resolution testing.""" 
    return [
        {
            "content": "PIN key derivation uses email as salt (OUTDATED - pre v0.4.2)",
            "type": "semantic",
            "tags": ["test", "conflict", "PIN", "email", "salt", "outdated"],
            "subject": "encryption"
        },
        {
            "content": "PIN key derivation uses server-generated random salt (v0.4.2 CURRENT)",
            "type": "semantic",
            "tags": ["test", "conflict", "PIN", "server", "salt", "current", "v0.4.2"],
            "subject": "encryption"  
        }
    ]

def check_ctxovrflw_health() -> bool:
    """Check if ctxovrflw service is healthy."""
    try:
        response = requests.get(f"{CTXOVRFLW_API_BASE}/health", timeout=5)
        return response.status_code == 200
    except Exception as e:
        print(f"Health check failed: {e}")
        return False

def seed_memory_batch(memories: List[Dict[str, Any]], batch_name: str) -> int:
    """Seed a batch of memories."""
    print(f"Seeding {batch_name}...")
    success_count = 0
    
    for i, memory in enumerate(memories):
        try:
            response = requests.post(
                f"{CTXOVRFLW_API_BASE}/v1/memories",
                json=memory,
                timeout=10,
                headers={"Content-Type": "application/json"}
            )
            
            if response.status_code in [200, 201]:
                success_count += 1
                print(f"  ✓ {i+1}/{len(memories)}: {memory['content'][:50]}...")
            else:
                print(f"  ✗ {i+1}/{len(memories)}: Failed ({response.status_code})")
                print(f"    Response: {response.text}")
                
        except Exception as e:
            print(f"  ✗ {i+1}/{len(memories)}: Error - {e}")
        
        # Small delay to avoid overwhelming the API
        time.sleep(0.1)
    
    print(f"Seeded {success_count}/{len(memories)} {batch_name} memories\n")
    return success_count

def clear_test_memories():
    """Clear existing test memories."""
    print("Clearing existing test memories...")
    
    try:
        # Search for test memories
        response = requests.post(
            f"{CTXOVRFLW_API_BASE}/v1/memories/recall",
            json={"query": "test conflict benchmark", "limit": 100},
            timeout=10,
            headers={"Content-Type": "application/json"}
        )
        
        if response.status_code != 200:
            print(f"Failed to search memories: {response.status_code}")
            return
        
        results = response.json()
        memories = [r.get("memory", r) for r in results.get("results", [])]
        
        deleted_count = 0
        for memory in memories:
            memory_id = memory.get("id")
            if memory_id:
                try:
                    delete_response = requests.delete(
                        f"{CTXOVRFLW_API_BASE}/v1/memories/{memory_id}",
                        timeout=10
                    )
                    if delete_response.status_code == 200:
                        deleted_count += 1
                except Exception as e:
                    print(f"Error deleting memory {memory_id}: {e}")
        
        print(f"Cleared {deleted_count} existing test memories\n")
        
    except Exception as e:
        print(f"Error clearing test memories: {e}\n")

def main():
    """Main seeding function."""
    print("ctxovrflw Memory Seeding for Benchmarks")
    print("=" * 50)
    
    # Check service health
    if not check_ctxovrflw_health():
        print("❌ ctxovrflw service is not healthy!")
        print("Make sure ctxovrflw is running and accessible.")
        sys.exit(1)
    
    print("✅ ctxovrflw service is healthy\n")
    
    # Clear existing test memories
    clear_test_memories()
    
    # Seed all memory categories
    total_seeded = 0
    
    batches = [
        (seed_architecture_memories(), "Architecture"),
        (seed_security_memories(), "Security"),
        (seed_deployment_memories(), "Deployment"), 
        (seed_code_structure_memories(), "Code Structure"),
        (seed_conflict_memories(), "Conflict Resolution")
    ]
    
    for memories, batch_name in batches:
        total_seeded += seed_memory_batch(memories, batch_name)
    
    print(f"✅ Seeding complete! Total memories seeded: {total_seeded}")
    print("\nYou can now run benchmarks with ctxovrflw mode enabled.")

if __name__ == "__main__":
    main()