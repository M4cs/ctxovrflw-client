#!/usr/bin/env python3
"""
Generate HTML visualization from benchmark results JSON.
"""

import argparse
import json
import os
from datetime import datetime
from typing import Dict, List, Any, Optional
from jinja2 import Template

from config import RESULTS_DIR, TEMPLATES_DIR

class ReportGenerator:
    """Generate HTML reports from benchmark results."""
    
    def __init__(self):
        self.template_path = os.path.join(TEMPLATES_DIR, "report.html")
    
    def load_results(self, filepath: str) -> Dict[str, Any]:
        """Load results from JSON file."""
        with open(filepath, 'r') as f:
            return json.load(f)
    
    def analyze_results(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Analyze results for chart data."""
        results = data.get("results", [])
        
        # Group by mode and platform
        by_mode = {}
        by_platform = {}
        by_scenario = {}
        
        for result in results:
            mode = result["mode"]
            platform = result["platform"] 
            scenario = result["scenario_id"]
            
            # By mode
            if mode not in by_mode:
                by_mode[mode] = []
            by_mode[mode].append(result)
            
            # By platform  
            if platform not in by_platform:
                by_platform[platform] = []
            by_platform[platform].append(result)
            
            # By scenario
            if scenario not in by_scenario:
                by_scenario[scenario] = []
            by_scenario[scenario].append(result)
        
        # Calculate metrics for charts
        analysis = {
            "by_mode": {},
            "by_platform": {},
            "by_scenario": {},
            "tool_call_comparison": {},
            "latency_comparison": {},
            "token_comparison": {},
            "score_comparison": {}
        }
        
        # Mode analysis
        for mode, mode_results in by_mode.items():
            analysis["by_mode"][mode] = {
                "count": len(mode_results),
                "avg_tool_calls": sum(r["tool_call_count"] for r in mode_results) / len(mode_results),
                "avg_latency_ms": sum(r["elapsed_ms"] for r in mode_results) / len(mode_results), 
                "avg_input_tokens": sum(r["input_tokens"] for r in mode_results) / len(mode_results),
                "avg_output_tokens": sum(r["output_tokens"] for r in mode_results) / len(mode_results),
                "avg_total_tokens": sum(r["total_tokens"] for r in mode_results) / len(mode_results),
                "avg_score": sum(r["composite_score"] for r in mode_results if r["composite_score"]) / len([r for r in mode_results if r["composite_score"]]) if any(r["composite_score"] for r in mode_results) else 0,
                "error_rate": len([r for r in mode_results if r["error"]]) / len(mode_results)
            }
        
        # Platform analysis
        for platform, platform_results in by_platform.items():
            analysis["by_platform"][platform] = {
                "count": len(platform_results),
                "avg_tool_calls": sum(r["tool_call_count"] for r in platform_results) / len(platform_results),
                "avg_latency_ms": sum(r["elapsed_ms"] for r in platform_results) / len(platform_results),
                "avg_total_tokens": sum(r["total_tokens"] for r in platform_results) / len(platform_results),
                "avg_score": sum(r["composite_score"] for r in platform_results if r["composite_score"]) / len([r for r in platform_results if r["composite_score"]]) if any(r["composite_score"] for r in platform_results) else 0,
                "error_rate": len([r for r in platform_results if r["error"]]) / len(platform_results)
            }
        
        # Scenario analysis  
        for scenario, scenario_results in by_scenario.items():
            analysis["by_scenario"][scenario] = {
                "results": scenario_results,
                "modes": list(set(r["mode"] for r in scenario_results)),
                "platforms": list(set(r["platform"] for r in scenario_results))
            }
        
        return analysis
    
    def create_chart_data(self, analysis: Dict[str, Any]) -> Dict[str, Any]:
        """Create data structures for Chart.js."""
        
        modes = list(analysis["by_mode"].keys())
        
        chart_data = {
            "tool_calls_chart": {
                "labels": modes,
                "data": [analysis["by_mode"][mode]["avg_tool_calls"] for mode in modes],
                "title": "Average Tool Calls by Mode"
            },
            "latency_chart": {
                "labels": modes,
                "data": [analysis["by_mode"][mode]["avg_latency_ms"] for mode in modes],
                "title": "Average Response Latency (ms) by Mode"
            },
            "token_chart": {
                "labels": modes,
                "input_data": [analysis["by_mode"][mode]["avg_input_tokens"] for mode in modes],
                "output_data": [analysis["by_mode"][mode]["avg_output_tokens"] for mode in modes],
                "title": "Token Usage by Mode"
            },
            "score_chart": {
                "labels": modes,
                "data": [analysis["by_mode"][mode]["avg_score"] for mode in modes],
                "title": "Average Composite Score by Mode"
            }
        }
        
        return chart_data
    
    def generate_html(self, data: Dict[str, Any], output_path: str):
        """Generate HTML report."""
        
        # Analyze results
        analysis = self.analyze_results(data)
        chart_data = self.create_chart_data(analysis)
        
        # Load template
        template_content = """
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ctxovrflw Benchmark Report</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background: #f8f9fa;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }
        .header {
            text-align: center;
            margin-bottom: 40px;
            padding-bottom: 20px;
            border-bottom: 2px solid #e9ecef;
        }
        .header h1 {
            color: #2c3e50;
            margin-bottom: 10px;
        }
        .header .subtitle {
            color: #6c757d;
            font-size: 1.1em;
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-bottom: 40px;
        }
        .metric-card {
            background: #f8f9fa;
            padding: 20px;
            border-radius: 6px;
            border-left: 4px solid #007bff;
        }
        .metric-title {
            font-weight: 600;
            color: #495057;
            margin-bottom: 10px;
        }
        .metric-value {
            font-size: 2em;
            font-weight: bold;
            color: #007bff;
        }
        .chart-section {
            margin: 40px 0;
        }
        .chart-title {
            font-size: 1.3em;
            font-weight: 600;
            margin-bottom: 20px;
            color: #2c3e50;
        }
        .chart-container {
            position: relative;
            height: 400px;
            margin-bottom: 20px;
        }
        .chart-interpretation {
            background: #e3f2fd;
            padding: 15px;
            border-radius: 6px;
            border-left: 4px solid #2196f3;
            margin-bottom: 20px;
        }
        .results-table {
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }
        .results-table th,
        .results-table td {
            text-align: left;
            padding: 12px;
            border-bottom: 1px solid #dee2e6;
        }
        .results-table th {
            background: #f8f9fa;
            font-weight: 600;
            color: #495057;
        }
        .error {
            color: #dc3545;
        }
        .success {
            color: #28a745;
        }
        .mode-badge {
            display: inline-block;
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 0.8em;
            font-weight: 600;
        }
        .mode-baseline { background: #ffc107; color: #000; }
        .mode-directed { background: #17a2b8; color: #fff; }
        .mode-ctxovrflw { background: #28a745; color: #fff; }
        .raw-data {
            margin-top: 40px;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 6px;
        }
        .raw-data details {
            margin-top: 10px;
        }
        .raw-data pre {
            background: #ffffff;
            padding: 15px;
            border-radius: 4px;
            overflow-x: auto;
            font-size: 0.9em;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>ctxovrflw Benchmark Report</h1>
            <div class="subtitle">
                Generated on {{ timestamp }}<br>
                Total Scenarios: {{ total_scenarios }} | Total Runs: {{ total_runs }}
            </div>
        </div>

        <div class="metrics-grid">
            {% for mode, stats in summary.by_mode.items() %}
            <div class="metric-card">
                <div class="metric-title">{{ mode.title() }} Mode</div>
                <div class="metric-value">{{ "%.2f"|format(stats.avg_score) }}</div>
                <div>Average Score</div>
            </div>
            {% endfor %}
        </div>

        <div class="chart-section">
            <div class="chart-title">Tool Call Efficiency</div>
            <div class="chart-container">
                <canvas id="toolCallsChart"></canvas>
            </div>
            <div class="chart-interpretation">
                <strong>Key Insight:</strong> ctxovrflw mode should show significantly fewer tool calls due to semantic memory, 
                avoiding redundant file reads and searches.
            </div>
        </div>

        <div class="chart-section">
            <div class="chart-title">Response Latency</div>
            <div class="chart-container">
                <canvas id="latencyChart"></canvas>
            </div>
            <div class="chart-interpretation">
                <strong>Key Insight:</strong> Faster response times indicate more efficient information retrieval. 
                ctxovrflw should show lower latency due to direct memory access.
            </div>
        </div>

        <div class="chart-section">
            <div class="chart-title">Token Efficiency</div>
            <div class="chart-container">
                <canvas id="tokenChart"></canvas>
            </div>
            <div class="chart-interpretation">
                <strong>Key Insight:</strong> Lower token usage indicates more efficient context utilization. 
                ctxovrflw should use fewer tokens by avoiding large file context injection.
            </div>
        </div>

        <div class="chart-section">
            <div class="chart-title">Accuracy Scores</div>
            <div class="chart-container">
                <canvas id="scoreChart"></canvas>
            </div>
            <div class="chart-interpretation">
                <strong>Key Insight:</strong> Higher scores indicate better answer quality. 
                All modes should perform similarly on accuracy, proving ctxovrflw doesn't sacrifice quality for efficiency.
            </div>
        </div>

        <div class="raw-data">
            <h3>Detailed Results</h3>
            <details>
                <summary>Click to expand raw data (JSON)</summary>
                <pre>{{ raw_data | tojson(indent=2) }}</pre>
            </details>
        </div>
    </div>

    <script>
        // Chart.js configuration
        const chartOptions = {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    position: 'top',
                }
            },
            scales: {
                y: {
                    beginAtZero: true
                }
            }
        };

        // Tool Calls Chart
        new Chart(document.getElementById('toolCallsChart'), {
            type: 'bar',
            data: {
                labels: {{ chart_data.tool_calls_chart.labels | tojson }},
                datasets: [{
                    label: 'Average Tool Calls',
                    data: {{ chart_data.tool_calls_chart.data | tojson }},
                    backgroundColor: ['#ffc107', '#17a2b8', '#28a745'],
                    borderColor: ['#e0a800', '#138496', '#1e7e34'],
                    borderWidth: 1
                }]
            },
            options: chartOptions
        });

        // Latency Chart  
        new Chart(document.getElementById('latencyChart'), {
            type: 'bar',
            data: {
                labels: {{ chart_data.latency_chart.labels | tojson }},
                datasets: [{
                    label: 'Average Latency (ms)',
                    data: {{ chart_data.latency_chart.data | tojson }},
                    backgroundColor: ['#ffc107', '#17a2b8', '#28a745'],
                    borderColor: ['#e0a800', '#138496', '#1e7e34'],
                    borderWidth: 1
                }]
            },
            options: chartOptions
        });

        // Token Chart (stacked)
        new Chart(document.getElementById('tokenChart'), {
            type: 'bar',
            data: {
                labels: {{ chart_data.token_chart.labels | tojson }},
                datasets: [
                    {
                        label: 'Input Tokens',
                        data: {{ chart_data.token_chart.input_data | tojson }},
                        backgroundColor: '#36a2eb',
                        stack: 'tokens'
                    },
                    {
                        label: 'Output Tokens', 
                        data: {{ chart_data.token_chart.output_data | tojson }},
                        backgroundColor: '#ff6384',
                        stack: 'tokens'
                    }
                ]
            },
            options: {
                ...chartOptions,
                scales: {
                    ...chartOptions.scales,
                    x: {
                        stacked: true
                    },
                    y: {
                        stacked: true,
                        beginAtZero: true
                    }
                }
            }
        });

        // Score Chart
        new Chart(document.getElementById('scoreChart'), {
            type: 'bar',
            data: {
                labels: {{ chart_data.score_chart.labels | tojson }},
                datasets: [{
                    label: 'Average Composite Score',
                    data: {{ chart_data.score_chart.data | tojson }},
                    backgroundColor: ['#ffc107', '#17a2b8', '#28a745'],
                    borderColor: ['#e0a800', '#138496', '#1e7e34'],
                    borderWidth: 1
                }]
            },
            options: {
                ...chartOptions,
                scales: {
                    y: {
                        beginAtZero: true,
                        max: 1.0
                    }
                }
            }
        });
    </script>
</body>
</html>
        """
        
        template = Template(template_content)
        
        html_content = template.render(
            timestamp=datetime.now().strftime("%Y-%m-%d %H:%M:%S UTC"),
            total_scenarios=data.get("summary", {}).get("total_scenarios", 0),
            total_runs=data.get("summary", {}).get("total_runs", 0),
            summary=analysis,
            chart_data=chart_data,
            raw_data=data
        )
        
        with open(output_path, 'w') as f:
            f.write(html_content)
        
        print(f"HTML report generated: {output_path}")

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description="Generate HTML report from benchmark results")
    parser.add_argument("results_file", help="Path to benchmark results JSON file")
    parser.add_argument("-o", "--output", help="Output HTML file path")
    
    args = parser.parse_args()
    
    # Determine output path
    if args.output:
        output_path = args.output
    else:
        base_name = os.path.splitext(os.path.basename(args.results_file))[0]
        output_path = os.path.join(RESULTS_DIR, f"{base_name}_report.html")
    
    # Generate report
    generator = ReportGenerator()
    data = generator.load_results(args.results_file)
    generator.generate_html(data, output_path)
    
    print(f"\n🎯 Open the report: {output_path}")

if __name__ == "__main__":
    main()