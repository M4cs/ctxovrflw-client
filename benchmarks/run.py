#!/usr/bin/env python3
"""
Main entry point for ctxovrflw benchmark suite.

Usage: python run.py [--platform claude|openclaw|both] [--mode all|baseline|directed|ctxovrflw] [--quick]
"""

import argparse
import asyncio
import time
import json
import os
import sys

# Force unbuffered stdout for real-time output
sys.stdout.reconfigure(line_buffering=True)
from typing import List, Dict, Any, Optional
from datetime import datetime

from config import RESULTS_DIR, TEST_MODES, PLATFORMS
from scenarios import get_scenarios, get_memory_seeds_for_scenario
from metrics import MetricsCollector, BenchmarkResult
from platforms import ClaudeCodePlatform, OpenClawPlatform
from modes import BaselineMode, DirectedMode, CtxovrflwMode
from modes.tier_test import TierTestMode

class BenchmarkRunner:
    """Main benchmark runner."""
    
    def __init__(self):
        self.metrics = MetricsCollector()
        
        # Initialize platforms
        self.platforms = {}
        try:
            self.platforms["claude"] = ClaudeCodePlatform()
        except Exception as e:
            print(f"Warning: Claude Code platform not available: {e}")
        
        try:
            self.platforms["openclaw"] = OpenClawPlatform()
        except Exception as e:
            print(f"Warning: OpenClaw platform not available: {e}")
        
        # Initialize modes
        self.modes = {
            "baseline": BaselineMode(),
            "directed": DirectedMode(),
            "ctxovrflw": CtxovrflwMode()
        }
        
        # Tier test mode (for tier comparison scenarios)
        self.tier_test = TierTestMode()
    
    def check_prerequisites(self) -> Dict[str, bool]:
        """Check if all prerequisites are met."""
        status = {
            "claude_sdk": False,
            "openclaw_cli": False,
            "ctxovrflw_service": False
        }
        
        # Check Claude SDK
        try:
            import claude_agent_sdk
            status["claude_sdk"] = True
        except ImportError:
            pass
        
        # Check OpenClaw CLI
        try:
            import subprocess
            result = subprocess.run(["claude", "--version"], capture_output=True, timeout=5)
            status["openclaw_cli"] = result.returncode == 0
        except Exception:
            pass
        
        # Check ctxovrflw service
        try:
            status["ctxovrflw_service"] = self.modes["ctxovrflw"].check_ctxovrflw_status()["status"] == "healthy"
        except Exception:
            pass
        
        return status
    
    async def run_single_benchmark(
        self, 
        scenario_id: str, 
        mode_name: str, 
        platform_name: str,
        session_context: Optional[str] = None
    ) -> Optional[BenchmarkResult]:
        """Run a single benchmark scenario."""
        
        from scenarios import get_scenario_by_id
        
        try:
            scenario = get_scenario_by_id(scenario_id)
            mode = self.modes[mode_name]
            platform = self.platforms[platform_name]
            
            print(f"Running {scenario_id} on {platform_name} with {mode_name} mode...")
            
            # Prepare system prompt
            base_prompt = session_context
            system_prompt = mode.prepare_system_prompt(base_prompt)
            
            # Add directed context if applicable
            if mode_name == "directed":
                context = mode.prepare_context(scenario_id)
                if context:
                    system_prompt += f"\n\n{context}"
            
            # Seed conflict memories if needed
            if mode_name == "ctxovrflw" and scenario_id.startswith("cr_"):
                memory_seeds = get_memory_seeds_for_scenario(scenario_id)
                if memory_seeds:
                    mode.seed_memories(memory_seeds)
            
            # Get tools
            allowed_tools = mode.get_allowed_tools()
            cwd = mode.get_working_directory()
            
            # Run the task (both platforms are async)
            result = await platform.run_task(
                prompt=scenario.question,
                system_prompt=system_prompt,
                allowed_tools=allowed_tools,
                cwd=cwd
            )
            
            benchmark_result = self.metrics.create_result(
                scenario=scenario,
                mode=mode_name,
                platform=platform_name,
                elapsed_ms=result.elapsed_ms,
                tool_calls=result.tool_calls,
                input_tokens=result.input_tokens,
                output_tokens=result.output_tokens,
                final_answer=result.final_answer,
                error=result.error
            )
            
            print(f"  ✓ Completed in {benchmark_result.elapsed_ms:.0f}ms")
            print(f"  ✓ Tool calls: {benchmark_result.tool_call_count}")
            print(f"  ✓ Score: {benchmark_result.composite_score:.2f}" if benchmark_result.composite_score else "  ✓ Score: N/A")
            
            return benchmark_result
            
        except Exception as e:
            print(f"  ✗ Error: {e}")
            return None
    
    async def run_ttl_scenario(self, scenario_id: str, mode_name: str, platform_name: str) -> Optional[BenchmarkResult]:
        """Run Test-Time Learning scenario with two sessions."""
        from scenarios import get_scenario_by_id
        
        try:
            scenario = get_scenario_by_id(scenario_id)
            
            if not scenario.setup_instructions or not scenario.previous_session_context:
                print(f"  ✗ TTL scenario {scenario_id} missing setup instructions")
                return None
            
            print(f"Running TTL scenario {scenario_id}:")
            print(f"  Phase 1: Learning session")
            
            # Phase 1: Learning session
            # For baseline/directed modes, we simulate this but it won't help in Phase 2
            # For ctxovrflw mode, this should store information in persistent memory
            
            if mode_name == "ctxovrflw":
                mode = self.modes[mode_name]
                # Store the learning context in ctxovrflw memory
                learning_memory = {
                    "content": scenario.previous_session_context,
                    "type": "semantic",
                    "tags": ["ttl", "learning", scenario_id],
                    "subject": "deployment"
                }
                mode.seed_memories([learning_memory])
                print(f"    ✓ Seeded learning context in ctxovrflw")
            else:
                print(f"    ✓ Learning phase simulated (no cross-session memory for {mode_name})")
            
            print(f"  Phase 2: Recall session")
            
            # Phase 2: Recall session - now ask the actual question
            return await self.run_single_benchmark(
                scenario_id=scenario_id,
                mode_name=mode_name,
                platform_name=platform_name,
                session_context=None  # Fresh session
            )
            
        except Exception as e:
            print(f"  ✗ TTL Error: {e}")
            return None
    
    async def run_tier_scenario(
        self,
        scenario_id: str,
        tier: str,
        platform_name: str,
    ) -> Optional[BenchmarkResult]:
        """Run a tier comparison scenario — tests recall quality at a specific tier."""
        from scenarios import get_scenario_by_id

        try:
            scenario = get_scenario_by_id(scenario_id)
            platform = self.platforms[platform_name]

            print(f"    Recalling at {tier} tier...")
            recall = self.tier_test.recall_at_tier(scenario.question, tier, limit=5)
            recall_context = self.tier_test.format_recall_as_context(recall)
            system_prompt = self.tier_test.prepare_system_prompt(tier, recall_context)

            recall_ms = recall.get("elapsed_ms", 0)
            result_count = recall.get("result_count", 0)
            top_score = recall.get("top_score", 0)
            print(f"    Recall: {result_count} results, top_score={top_score:.3f}, {recall_ms:.0f}ms")

            # Now ask the LLM using only the recalled context (no file tools)
            result = await platform.run_task(
                prompt=scenario.question,
                system_prompt=system_prompt,
                allowed_tools=[],  # No tools — pure recall test
                max_turns=1,
                cwd=self.tier_test.get_working_directory(),
            )

            # Use tier as mode name for results
            mode_label = f"tier_{tier}"
            benchmark_result = self.metrics.create_result(
                scenario=scenario,
                mode=mode_label,
                platform=platform_name,
                elapsed_ms=result.elapsed_ms + recall_ms,
                tool_calls=result.tool_calls,
                input_tokens=result.input_tokens,
                output_tokens=result.output_tokens,
                final_answer=result.final_answer,
                error=result.error,
            )

            print(f"  ✓ Completed in {benchmark_result.elapsed_ms:.0f}ms")
            print(f"  ✓ Score: {benchmark_result.composite_score:.2f}" if benchmark_result.composite_score else "  ✓ Score: N/A")

            return benchmark_result

        except Exception as e:
            print(f"  ✗ Tier test error: {e}")
            import traceback
            traceback.print_exc()
            return None

    async def run_benchmarks(
        self,
        platforms: List[str],
        modes: List[str],
        quick_mode: bool = False
    ) -> Dict[str, Any]:
        """Run all benchmarks."""
        
        print("ctxovrflw Benchmark Suite")
        print("=" * 50)
        
        # Check prerequisites
        prereqs = self.check_prerequisites()
        print("Prerequisites:")
        for name, status in prereqs.items():
            print(f"  {name}: {'✅' if status else '❌'}")
        print()
        
        # Get scenarios
        scenarios = get_scenarios(quick_mode)
        
        # Split into regular and tier scenarios
        regular_scenarios = [s for s in scenarios if s.category != "Tier Comparison"]
        tier_scenarios = [s for s in scenarios if s.category == "Tier Comparison"]
        
        regular_runs = len(regular_scenarios) * len(platforms) * len(modes)
        tier_runs = len(tier_scenarios) * len(platforms) * 3  # 3 tiers each
        total_runs = regular_runs + tier_runs
        
        print(f"Running {len(scenarios)} scenarios in {'quick' if quick_mode else 'full'} mode")
        print(f"  Regular: {len(regular_scenarios)} scenarios × {len(modes)} modes × {len(platforms)} platforms = {regular_runs} runs")
        if tier_scenarios:
            print(f"  Tier:    {len(tier_scenarios)} scenarios × 3 tiers × {len(platforms)} platforms = {tier_runs} runs")
        print(f"  Total:   {total_runs} runs")
        print(f"Platforms: {platforms}")
        print(f"Modes: {modes}")
        print()
        
        # Seed ctxovrflw (always needed for tier tests too)
        if "ctxovrflw" in modes or tier_scenarios:
            print("Seeding ctxovrflw memories...")
            os.system("python3 seed_memories.py")
            print()
        
        results = []
        current_run = 0
        
        # Regular scenarios
        for scenario in regular_scenarios:
            print(f"\n📋 Scenario: {scenario.id} ({scenario.category})")
            print(f"   Question: {scenario.question}")
            
            for platform_name in platforms:
                if platform_name not in self.platforms:
                    print(f"   ⚠️  Skipping {platform_name} (not available)")
                    continue
                
                for mode_name in modes:
                    current_run += 1
                    print(f"\n   [{current_run}/{total_runs}] {platform_name} + {mode_name}")
                    
                    if scenario.category == "Test-Time Learning":
                        result = await self.run_ttl_scenario(scenario.id, mode_name, platform_name)
                    else:
                        result = await self.run_single_benchmark(scenario.id, mode_name, platform_name)
                    
                    if result:
                        results.append(result)
                        self.metrics.add_result(result)
                    
                    time.sleep(1)
        
        # Tier comparison scenarios
        for scenario in tier_scenarios:
            print(f"\n📋 Tier Scenario: {scenario.id} ({scenario.category})")
            print(f"   Question: {scenario.question}")
            
            for platform_name in platforms:
                if platform_name not in self.platforms:
                    print(f"   ⚠️  Skipping {platform_name} (not available)")
                    continue
                
                for tier in ["free", "standard", "pro"]:
                    current_run += 1
                    print(f"\n   [{current_run}/{total_runs}] {platform_name} + tier_{tier}")
                    
                    result = await self.run_tier_scenario(scenario.id, tier, platform_name)
                    
                    if result:
                        results.append(result)
                        self.metrics.add_result(result)
                    
                    time.sleep(1)
        
        # Generate summary
        summary = self.metrics.calculate_summary_stats()
        
        return {
            "completed_at": datetime.now().isoformat(),
            "total_runs": len(results),
            "summary": summary,
            "results": [r.to_dict() for r in results]
        }
    
    def save_results(self, data: Dict[str, Any], filename: Optional[str] = None):
        """Save benchmark results."""
        if not filename:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = f"benchmark_results_{timestamp}.json"
        
        filepath = os.path.join(RESULTS_DIR, filename)
        os.makedirs(RESULTS_DIR, exist_ok=True)
        
        with open(filepath, 'w') as f:
            json.dump(data, f, indent=2)
        
        print(f"\n💾 Results saved to: {filepath}")
        return filepath

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description="ctxovrflw Benchmark Suite")
    
    parser.add_argument(
        "--platform",
        choices=["claude", "openclaw", "both"],
        default="both",
        help="Platform(s) to run benchmarks on"
    )
    
    parser.add_argument(
        "--mode", 
        choices=["all", "baseline", "directed", "ctxovrflw"],
        default="all",
        help="Test mode(s) to run"
    )
    
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Run quick mode (subset of scenarios)"
    )
    
    parser.add_argument(
        "--output",
        help="Output filename for results"
    )
    
    args = parser.parse_args()
    
    # Determine platforms
    if args.platform == "both":
        platforms = ["claude", "openclaw"]
    else:
        platforms = [args.platform]
    
    # Determine modes
    if args.mode == "all":
        modes = TEST_MODES
    else:
        modes = [args.mode]
    
    # Run benchmarks
    runner = BenchmarkRunner()
    
    try:
        results = asyncio.run(runner.run_benchmarks(platforms, modes, args.quick))
        filepath = runner.save_results(results, args.output)
        
        print("\n📊 Summary:")
        summary = results["summary"]
        print(f"   Total runs: {summary.get('total_runs', 0)}")
        print(f"   Avg score: {summary.get('avg_composite_score', 0):.2f}" if summary.get('avg_composite_score') else "   Avg score: N/A")
        print(f"   Errors: {summary.get('errors', 0)}")
        
        print(f"\n🎯 Generate HTML report: python report.py {filepath}")
        
    except KeyboardInterrupt:
        print("\n\n⛔ Benchmark interrupted by user")
    except Exception as e:
        print(f"\n\n💥 Benchmark failed: {e}")
        raise

if __name__ == "__main__":
    main()