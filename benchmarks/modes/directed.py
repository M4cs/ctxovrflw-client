"""
Directed mode - LLM given explicit file paths to read.

Simulates @file references. Context is loaded but not pre-summarized.
"""

import os
from typing import Optional, List, Dict, Any
try:
    from ..config import get_tools_for_mode, REPO_ROOT
except ImportError:
    import sys
    import os
    sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from config import get_tools_for_mode, REPO_ROOT

class DirectedMode:
    """Directed testing mode with explicit file context."""
    
    def __init__(self):
        self.mode_name = "directed"
        self.tools = get_tools_for_mode("directed")
        
        # Map scenarios to relevant files
        self.scenario_files = {
            "ar_1_encryption": [
                "src/crypto/mod.rs",
                "src/crypto/encryption.rs", 
                "src/sync/mod.rs",
                "Cargo.toml"
            ],
            "ar_2_hybrid_search": [
                "src/search/mod.rs",
                "src/search/hybrid.rs",
                "src/search/semantic.rs",
                "src/search/lexical.rs"
            ],
            "ar_3_ci_platforms": [
                ".github/workflows/ci.yml",
                ".github/workflows/release.yml",
                "Cargo.toml"
            ],
            "lru_1_auth_flow": [
                "src/device-auth.ts",
                "src/auth.ts", 
                "src/login.rs",
                "src/crypto/mod.rs",
                "src/sync/mod.rs",
                "src/api/auth.rs"
            ],
            "cr_1_pin_derivation": [
                "src/crypto/pin.rs",
                "src/crypto/mod.rs",
                "src/auth/pin.rs",
                "CHANGELOG.md"
            ]
        }
    
    def prepare_system_prompt(self, base_prompt: Optional[str] = None) -> str:
        """Prepare system prompt for directed mode."""
        
        directed_instructions = f"""
You are an AI assistant helping to answer questions about the ctxovrflw codebase.

Repository location: {REPO_ROOT}

You have access to these tools:
- Read: Read file contents
- Bash: Execute shell commands
- Glob: Find files matching patterns
- Edit: Make precise edits to files
- Write: Create or overwrite files

The relevant files for this question have been identified. You should focus on reading and analyzing these specific files, though you may explore related files if needed.

Provide accurate, detailed answers based on what you find in the code.
"""
        
        if base_prompt:
            return f"{base_prompt}\n\n{directed_instructions}"
        else:
            return directed_instructions
    
    def get_allowed_tools(self) -> List[str]:
        """Get list of allowed tools for this mode."""
        return self.tools.copy()
    
    def get_working_directory(self) -> str:
        """Get working directory for this mode."""
        return REPO_ROOT
    
    def supports_cross_session_memory(self) -> bool:
        """Whether this mode supports cross-session memory."""
        return False
    
    def prepare_context(self, scenario_id: str) -> Optional[str]:
        """Prepare file context for the scenario."""
        
        files = self.scenario_files.get(scenario_id, [])
        if not files:
            return None
        
        context_parts = ["RELEVANT FILES FOR THIS QUESTION:"]
        
        for file_path in files:
            full_path = os.path.join(REPO_ROOT, file_path)
            
            if os.path.exists(full_path):
                try:
                    with open(full_path, 'r', encoding='utf-8') as f:
                        content = f.read()
                    
                    context_parts.append(f"\n=== {file_path} ===")
                    
                    # If file is very large, truncate it
                    if len(content) > 10000:
                        context_parts.append(content[:10000] + "\n... [FILE TRUNCATED] ...")
                    else:
                        context_parts.append(content)
                    
                except Exception as e:
                    context_parts.append(f"\n=== {file_path} ===")
                    context_parts.append(f"ERROR: Could not read file: {e}")
            else:
                context_parts.append(f"\n=== {file_path} ===")
                context_parts.append("ERROR: File not found")
        
        return "\n".join(context_parts)
    
    def get_files_for_scenario(self, scenario_id: str) -> List[str]:
        """Get list of relevant files for a scenario."""
        return self.scenario_files.get(scenario_id, []).copy()
    
    def add_files_for_scenario(self, scenario_id: str, files: List[str]):
        """Add files for a scenario."""
        if scenario_id not in self.scenario_files:
            self.scenario_files[scenario_id] = []
        self.scenario_files[scenario_id].extend(files)
    
    def get_description(self) -> str:
        """Get human-readable description of this mode."""
        return (
            "Directed mode: LLM given explicit file paths to read. "
            "Simulates @file references with pre-loaded file contents."
        )