"""
Metrics collection and scoring for ctxovrflw benchmark suite.
"""

import re
import json
import time
import requests
from typing import Dict, List, Any, Optional, Union
from dataclasses import dataclass, asdict
try:
    from .scenarios import TestScenario, GroundTruth
    from .config import SCORING_CONFIG, get_api_key
except ImportError:
    from scenarios import TestScenario, GroundTruth
    from config import SCORING_CONFIG, get_api_key

@dataclass
class BenchmarkResult:
    """Results from a single benchmark run."""
    scenario_id: str
    mode: str
    platform: str
    
    # Performance metrics
    elapsed_ms: float
    tool_call_count: int
    tool_call_names: List[str]
    input_tokens: int
    output_tokens: int
    total_tokens: int
    
    # Answer data
    final_answer: str
    
    # Scoring
    keyword_score: float
    timestamp: str
    
    # Optional fields (with defaults)
    llm_judge_score: Optional[float] = None
    composite_score: Optional[float] = None
    error: Optional[str] = None
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        return asdict(self)

class MetricsCollector:
    """Collects and scores benchmark results."""
    
    def __init__(self):
        self.results: List[BenchmarkResult] = []
    
    def create_result(
        self,
        scenario: TestScenario,
        mode: str,
        platform: str,
        elapsed_ms: float,
        tool_calls: List[Dict[str, Any]],
        input_tokens: int,
        output_tokens: int,
        final_answer: str,
        error: Optional[str] = None
    ) -> BenchmarkResult:
        """Create a benchmark result from raw data."""
        
        tool_call_names = []
        for call in tool_calls:
            if isinstance(call, dict):
                tool_call_names.append(call.get('name', 'unknown'))
            else:
                tool_call_names.append(str(call))
        
        result = BenchmarkResult(
            scenario_id=scenario.id,
            mode=mode,
            platform=platform,
            elapsed_ms=elapsed_ms,
            tool_call_count=len(tool_calls),
            tool_call_names=tool_call_names,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            total_tokens=input_tokens + output_tokens,
            final_answer=final_answer,
            keyword_score=0.0,
            timestamp=time.strftime("%Y-%m-%d %H:%M:%S UTC"),
            error=error
        )
        
        # Score the result
        result.keyword_score = self.keyword_score(final_answer, scenario.ground_truth)
        
        if not error:  # Only try LLM judge if no error
            result.llm_judge_score = self.llm_judge_score(
                scenario.question, 
                final_answer, 
                scenario.ground_truth
            )
        
        result.composite_score = self.composite_score(result)
        
        return result
    
    def keyword_score(self, answer: str, ground_truth: GroundTruth) -> float:
        """Score based on keyword/phrase matching."""
        if not answer:
            return 0.0
        
        answer_lower = answer.lower()
        matched_keywords = 0
        
        for keyword in ground_truth.keywords:
            keyword_lower = keyword.lower()
            if keyword_lower in answer_lower:
                matched_keywords += 1
        
        return matched_keywords / len(ground_truth.keywords) if ground_truth.keywords else 0.0
    
    def llm_judge_score(
        self, 
        question: str, 
        answer: str, 
        ground_truth: GroundTruth
    ) -> Optional[float]:
        """Score using LLM as judge (0-10 scale).
        
        Tries in order: Anthropic API, OpenRouter API, Claude Agent SDK (local CLI).
        """
        
        anthropic_key = get_api_key("anthropic")
        openrouter_key = get_api_key("openrouter")
        
        prompt = f"""You are an expert evaluator scoring technical answers about a codebase called ctxovrflw.

Question: {question}

Student Answer: {answer}

Expected Information: {ground_truth.description}

Key Facts to Look For: {', '.join(ground_truth.keywords)}

Score this answer from 0-10 based on:
- Accuracy (40%): Are the technical facts correct?
- Completeness (40%): Does it cover the key points?
- Relevance (20%): Does it directly answer the question?

Respond with ONLY a number from 0 to 10 (can include one decimal, e.g. 7.5). Nothing else."""
        
        try:
            if anthropic_key:
                score = self._call_anthropic(prompt, anthropic_key)
            elif openrouter_key:
                score = self._call_openrouter(prompt, openrouter_key)
            else:
                # Fall back to Claude Agent SDK (uses local CLI auth)
                score = self._call_claude_sdk(prompt)
            
            return float(score) if score is not None else None
            
        except Exception as e:
            print(f"    LLM judge scoring failed: {e}")
            return None
    
    def _call_anthropic(self, prompt: str, api_key: str) -> Optional[float]:
        """Call Anthropic API for scoring."""
        headers = {
            "Content-Type": "application/json",
            "X-API-Key": api_key,
            "anthropic-version": "2023-06-01"
        }
        
        data = {
            "model": "claude-3-haiku-20240307",
            "max_tokens": 10,
            "messages": [{"role": "user", "content": prompt}]
        }
        
        response = requests.post(
            "https://api.anthropic.com/v1/messages",
            headers=headers,
            json=data,
            timeout=30
        )
        
        if response.status_code == 200:
            result = response.json()
            score_text = result["content"][0]["text"].strip()
            return self._extract_score(score_text)
        
        return None
    
    def _call_claude_sdk(self, prompt: str) -> Optional[float]:
        """Score using Claude Code CLI (uses local auth, no API key needed)."""
        import subprocess
        try:
            result = subprocess.run(
                ["claude", "-p", prompt, "--max-turns", "1"],
                capture_output=True, text=True, timeout=30, cwd="/tmp",
            )
            if result.returncode == 0 and result.stdout.strip():
                return self._extract_score(result.stdout.strip())
            return None
        except Exception as e:
            print(f"    Claude CLI judge failed: {e}")
            return None

    def _call_openrouter(self, prompt: str, api_key: str) -> Optional[float]:
        """Call OpenRouter API for scoring."""
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
        
        data = {
            "model": SCORING_CONFIG["llm_judge_model"],
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 10
        }
        
        response = requests.post(
            "https://openrouter.ai/api/v1/chat/completions",
            headers=headers,
            json=data,
            timeout=30
        )
        
        if response.status_code == 200:
            result = response.json()
            score_text = result["choices"][0]["message"]["content"].strip()
            return self._extract_score(score_text)
        
        return None
    
    def _extract_score(self, text: str) -> Optional[float]:
        """Extract numeric score from LLM response."""
        # Look for numbers 0-10
        matches = re.findall(r'\b([0-9](?:\.[0-9])?|10)\b', text)
        if matches:
            try:
                score = float(matches[0])
                return min(10.0, max(0.0, score))  # Clamp to 0-10
            except ValueError:
                pass
        return None
    
    def composite_score(self, result: BenchmarkResult) -> Optional[float]:
        """Calculate composite score from keyword and LLM judge scores."""
        keyword_weight = SCORING_CONFIG["keyword_weight"]
        llm_weight = SCORING_CONFIG["llm_judge_weight"]
        
        # Always have keyword score
        weighted_score = result.keyword_score * keyword_weight
        
        if result.llm_judge_score is not None:
            # Normalize LLM score to 0-1 scale
            normalized_llm = result.llm_judge_score / 10.0
            weighted_score += normalized_llm * llm_weight
        else:
            # If no LLM score, use only keyword score (normalized to full scale)
            weighted_score = result.keyword_score
        
        return weighted_score
    
    def add_result(self, result: BenchmarkResult):
        """Add a result to the collection."""
        self.results.append(result)
    
    def get_results_by_scenario(self, scenario_id: str) -> List[BenchmarkResult]:
        """Get all results for a specific scenario."""
        return [r for r in self.results if r.scenario_id == scenario_id]
    
    def get_results_by_mode(self, mode: str) -> List[BenchmarkResult]:
        """Get all results for a specific mode."""
        return [r for r in self.results if r.mode == mode]
    
    def get_results_by_platform(self, platform: str) -> List[BenchmarkResult]:
        """Get all results for a specific platform."""
        return [r for r in self.results if r.platform == platform]
    
    def calculate_summary_stats(self) -> Dict[str, Any]:
        """Calculate summary statistics across all results."""
        if not self.results:
            return {}
        
        stats = {
            "total_scenarios": len(set(r.scenario_id for r in self.results)),
            "total_runs": len(self.results),
            "modes": list(set(r.mode for r in self.results)),
            "platforms": list(set(r.platform for r in self.results)),
            "avg_elapsed_ms": sum(r.elapsed_ms for r in self.results) / len(self.results),
            "avg_tool_calls": sum(r.tool_call_count for r in self.results) / len(self.results),
            "avg_total_tokens": sum(r.total_tokens for r in self.results) / len(self.results),
            "avg_composite_score": None,
            "errors": len([r for r in self.results if r.error])
        }
        
        scores = [r.composite_score for r in self.results if r.composite_score is not None]
        if scores:
            stats["avg_composite_score"] = sum(scores) / len(scores)
        
        # Per-mode stats
        stats["by_mode"] = {}
        for mode in stats["modes"]:
            mode_results = self.get_results_by_mode(mode)
            mode_scores = [r.composite_score for r in mode_results if r.composite_score is not None]
            
            stats["by_mode"][mode] = {
                "count": len(mode_results),
                "avg_elapsed_ms": sum(r.elapsed_ms for r in mode_results) / len(mode_results),
                "avg_tool_calls": sum(r.tool_call_count for r in mode_results) / len(mode_results),
                "avg_total_tokens": sum(r.total_tokens for r in mode_results) / len(mode_results),
                "avg_composite_score": sum(mode_scores) / len(mode_scores) if mode_scores else None,
                "errors": len([r for r in mode_results if r.error])
            }
        
        return stats
    
    def save_results(self, filepath: str):
        """Save results to JSON file."""
        data = {
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S UTC"),
            "results": [r.to_dict() for r in self.results],
            "summary": self.calculate_summary_stats()
        }
        
        with open(filepath, 'w') as f:
            json.dump(data, f, indent=2)
    
    def load_results(self, filepath: str):
        """Load results from JSON file."""
        with open(filepath, 'r') as f:
            data = json.load(f)
        
        self.results = []
        for result_data in data.get("results", []):
            result = BenchmarkResult(**result_data)
            self.results.append(result)