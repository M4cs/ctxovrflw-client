"""
Baseline mode - Raw LLM with no pre-loaded context.

Agent must discover everything via tools (Read, Bash, Glob, etc.) and store in session memory only.
"""

from typing import Optional, List
try:
    from ..config import get_tools_for_mode, REPO_ROOT
except ImportError:
    import sys
    import os
    sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from config import get_tools_for_mode, REPO_ROOT

class BaselineMode:
    """Baseline testing mode with no context provided."""
    
    def __init__(self):
        self.mode_name = "baseline"
        self.tools = get_tools_for_mode("baseline")
    
    def prepare_system_prompt(self, base_prompt: Optional[str] = None) -> str:
        """Prepare system prompt for baseline mode."""
        
        baseline_instructions = f"""
You are an AI assistant helping to answer questions about the ctxovrflw codebase.

Repository location: {REPO_ROOT}

You have access to these tools:
- Read: Read file contents
- Bash: Execute shell commands
- Glob: Find files matching patterns
- Edit: Make precise edits to files  
- Write: Create or overwrite files

You must discover all information using these tools. You have no pre-loaded context about the codebase.
Start by exploring the repository structure and reading relevant files to understand the codebase.

Be thorough in your investigation and provide accurate, detailed answers based on what you find in the code.
"""
        
        if base_prompt:
            return f"{base_prompt}\n\n{baseline_instructions}"
        else:
            return baseline_instructions
    
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
        """Prepare any additional context (none for baseline)."""
        return None
    
    def get_description(self) -> str:
        """Get human-readable description of this mode."""
        return (
            "Baseline mode: Raw LLM with no pre-loaded context. "
            "Agent must discover everything via file system tools."
        )