use anyhow::Result;
use console::style;
use dialoguer::{Confirm, MultiSelect};
use std::path::PathBuf;

use crate::config::Config;

// ── Agent Registry ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct AgentDef {
    pub(crate) name: &'static str,
    pub(crate) detect: DetectMethod,
    /// Config paths for JSON-based MCP config (global/user level)
    pub(crate) config_paths: &'static [ConfigLocation],
    /// Uses CLI command for install instead of JSON config
    pub(crate) cli_install: Option<&'static str>,
    /// Global rules file path (relative to home dir)
    pub(crate) global_rules_path: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) enum DetectMethod {
    Binary(&'static str),
    Dir(&'static str),
    ConfigDir(&'static str),
    Any(&'static [&'static str]),
}

#[derive(Debug, Clone)]
pub(crate) enum ConfigLocation {
    /// Relative to home dir (~/)
    Home(&'static str),
    /// Relative to OS config dir (XDG_CONFIG_HOME on Linux, ~/Library/Application Support on Mac, %APPDATA% on Windows)
    Config(&'static str),
    /// macOS: ~/Library/Application Support/...
    MacApp(&'static str),
    /// Windows: %APPDATA%/...
    AppData(&'static str),
    /// Windows: %LOCALAPPDATA%/...
    LocalAppData(&'static str),
}

pub(crate) const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Claude Code",
        detect: DetectMethod::Binary("claude"),
        config_paths: &[],
        cli_install: Some("claude mcp add --transport sse --scope user ctxovrflw http://127.0.0.1:{port}/mcp/sse"),
        global_rules_path: Some(".claude/CLAUDE.md"),
    },
    AgentDef {
        name: "Claude Desktop",
        detect: DetectMethod::ConfigDir("Claude"),
        config_paths: &[
            ConfigLocation::Config("Claude/claude_desktop_config.json"),
            ConfigLocation::MacApp("Claude/claude_desktop_config.json"),
            ConfigLocation::AppData("Claude/claude_desktop_config.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Cursor",
        detect: DetectMethod::Dir(".cursor"),
        config_paths: &[ConfigLocation::Home(".cursor/mcp.json")],
        cli_install: None,
        global_rules_path: Some(".cursorrules"),
    },
    AgentDef {
        name: "Cline",
        detect: DetectMethod::Dir(".cline"),
        config_paths: &[
            ConfigLocation::Home(".cline/mcp_settings.json"),
            ConfigLocation::Config("Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
        ],
        cli_install: None,
        global_rules_path: Some(".cline/.clinerules"),
    },
    AgentDef {
        name: "Roo Code",
        detect: DetectMethod::Dir(".roo-code"),
        config_paths: &[
            ConfigLocation::Config("Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json"),
            ConfigLocation::Home(".roo-code/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: Some(".roo-code/.roorules"),
    },
    AgentDef {
        name: "Windsurf",
        detect: DetectMethod::Dir(".windsurf"),
        config_paths: &[ConfigLocation::Home(".windsurf/mcp.json")],
        cli_install: None,
        global_rules_path: Some(".windsurf/.windsurfrules"),
    },
    AgentDef {
        name: "Continue",
        detect: DetectMethod::Any(&["continue"]),
        config_paths: &[
            ConfigLocation::Home(".continue/config.json"),
            ConfigLocation::Config("continue/config.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Codex CLI",
        detect: DetectMethod::Binary("codex"),
        config_paths: &[
            ConfigLocation::Home(".codex/mcp.json"),
            ConfigLocation::Config("codex/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: Some(".codex/codex.md"),
    },
    AgentDef {
        name: "Goose",
        detect: DetectMethod::Any(&["goose", "goosed"]),
        config_paths: &[
            ConfigLocation::Home(".config/goose/config.json"),
            ConfigLocation::Config("goose/config.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Gemini CLI",
        detect: DetectMethod::Any(&["gemini"]),
        config_paths: &[
            ConfigLocation::Home(".gemini/settings.json"),
            ConfigLocation::Config("gemini/settings.json"),
        ],
        cli_install: None,
        global_rules_path: Some(".gemini/.gemini_rules"),
    },
    AgentDef {
        name: "Antigravity",
        detect: DetectMethod::Dir(".antigravity"),
        config_paths: &[ConfigLocation::Home(".antigravity/mcp.json")],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Amp",
        detect: DetectMethod::Binary("amp"),
        config_paths: &[
            ConfigLocation::Home(".amp/mcp.json"),
            ConfigLocation::Config("amp/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Kiro",
        detect: DetectMethod::Any(&["kiro", "kiro-cli"]),
        config_paths: &[ConfigLocation::Home(".kiro/mcp.json")],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "OpenCode",
        detect: DetectMethod::Binary("opencode"),
        config_paths: &[
            ConfigLocation::Home(".opencode/mcp.json"),
            ConfigLocation::Config("opencode/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Trae",
        detect: DetectMethod::Dir(".trae"),
        config_paths: &[ConfigLocation::Home(".trae/mcp.json")],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Kilo Code",
        detect: DetectMethod::Dir(".kilo"),
        config_paths: &[
            ConfigLocation::Config("Code/User/globalStorage/kilocode.kilo-code/settings/mcp_settings.json"),
            ConfigLocation::Home(".kilo/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Factory (Drip)",
        detect: DetectMethod::Binary("drip"),
        config_paths: &[
            ConfigLocation::Home(".factory/mcp.json"),
            ConfigLocation::Config("factory/mcp.json"),
        ],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "GitHub Copilot",
        detect: DetectMethod::Binary("gh-copilot"),
        config_paths: &[],
        cli_install: None,
        global_rules_path: Some(".github/copilot-instructions.md"),
    },
    AgentDef {
        name: "OpenClaw",
        detect: DetectMethod::Dir(".openclaw"),
        config_paths: &[],
        cli_install: None,
        global_rules_path: Some(".openclaw/workspace/AGENTS.md"),
    },
];

// ── Detection ────────────────────────────────────────────────

pub(crate) struct DetectedAgent {
    pub(crate) def: &'static AgentDef,
    pub(crate) config_path: Option<PathBuf>,
}

pub(crate) fn detect_agents() -> Vec<DetectedAgent> {
    let home = dirs::home_dir().unwrap_or_default();
    let config_dir = dirs::config_dir().unwrap_or_default();
    let mut found = Vec::new();

    for def in AGENTS {
        let detected = match &def.detect {
            DetectMethod::Binary(name) => which(name),
            DetectMethod::Dir(rel) => home.join(rel).exists(),
            DetectMethod::ConfigDir(rel) => {
                config_dir.join(rel).exists()
                    || home.join(format!(".{}", rel.to_lowercase())).exists()
            }
            DetectMethod::Any(names) => names.iter().any(|n| which(n)),
        };

        // Fallback: if primary detection failed, check if the global rules
        // directory exists (e.g. ~/.claude for Claude Code, ~/.cline for Cline).
        // Many tools create their home dir on first use even if the binary
        // isn't on PATH yet (common on Windows).
        let detected = detected || def.global_rules_path.map_or(false, |p| {
            // p is like ".claude/CLAUDE.md" — check if parent dir exists
            let parent = std::path::Path::new(p).parent().unwrap_or(std::path::Path::new(""));
            if !parent.as_os_str().is_empty() {
                home.join(parent).exists()
            } else {
                false
            }
        });

        // Also check if any config path directory exists
        let detected = detected || def.config_paths.iter().any(|loc| {
            let path = resolve_config_path(loc);
            path.exists()
        });

        if !detected {
            continue;
        }

        let config_path = def.config_paths.iter().find_map(|loc| {
            let path = resolve_config_path(loc);
            if path.exists() || path.parent().map(|p| p.exists()).unwrap_or(false) {
                Some(path)
            } else {
                None
            }
        });

        found.push(DetectedAgent { def, config_path });
    }

    found
}

pub(crate) fn resolve_config_path(loc: &ConfigLocation) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let config_dir = dirs::config_dir().unwrap_or_default();
    match loc {
        ConfigLocation::Home(rel) => home.join(rel),
        ConfigLocation::Config(rel) => {
            // On Windows, dirs::config_dir() = %APPDATA% (Roaming).
            // Some tools use %APPDATA%, some use %LOCALAPPDATA%, some use ~.
            // Check the standard config_dir first, then fall back to home.
            let primary = config_dir.join(rel);
            if primary.exists() || primary.parent().map(|p| p.exists()).unwrap_or(false) {
                return primary;
            }
            // On Windows, also check %LOCALAPPDATA%
            #[cfg(windows)]
            {
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    let local_path = PathBuf::from(local).join(rel);
                    if local_path.exists() || local_path.parent().map(|p| p.exists()).unwrap_or(false) {
                        return local_path;
                    }
                }
            }
            primary
        }
        ConfigLocation::MacApp(rel) => home.join("Library/Application Support").join(rel),
        ConfigLocation::AppData(rel) => std::env::var("APPDATA")
            .map(|a| PathBuf::from(a).join(rel))
            .unwrap_or_else(|_| config_dir.join(rel)),
        ConfigLocation::LocalAppData(rel) => std::env::var("LOCALAPPDATA")
            .map(|a| PathBuf::from(a).join(rel))
            .unwrap_or_else(|_| config_dir.join(rel)),
    }
}

pub(crate) fn which(cmd: &str) -> bool {
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(which_cmd)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Installation ─────────────────────────────────────────────

pub(crate) fn mcp_sse_url(cfg: &Config) -> String {
    if let Some(ref remote) = cfg.remote_daemon_url {
        format!("{}/mcp/sse", remote.trim_end_matches('/'))
    } else {
        format!("http://127.0.0.1:{}/mcp/sse", cfg.port)
    }
}

pub(crate) fn sse_mcp_json(cfg: &Config) -> serde_json::Value {
    serde_json::json!({
        "url": mcp_sse_url(cfg)
    })
}

fn install_agent(agent: &DetectedAgent, cfg: &Config) -> Result<()> {
    let url = mcp_sse_url(cfg);

    // CLI-based install (e.g., Claude Code)
    if let Some(cmd_template) = agent.def.cli_install {
        let cmd = cmd_template
            .replace("{port}", &cfg.port.to_string())
            .replace("http://127.0.0.1:{port}/mcp/sse", &url);
        println!("  {} {}", style("→").dim(), style(&cmd).dim());
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.len() >= 2 {
            let status = std::process::Command::new(parts[0])
                .args(&parts[1..])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            match status {
                Ok(s) if s.success() => {
                    println!("  {} {}", style("✓").green().bold(), agent.def.name);
                }
                _ => {
                    println!("  {} Auto-config failed. Run manually:", style("⚠").yellow());
                    println!("    {cmd}");
                }
            }
        }
        return Ok(());
    }

    // No config path available — manual instructions
    if agent.def.config_paths.is_empty() {
        println!(
            "  {} {} — add MCP server URL manually:",
            style("ℹ").blue(),
            agent.def.name
        );
        println!("    {url}");
        return Ok(());
    }

    // JSON config file
    let mcp_entry = sse_mcp_json(cfg);
    let config_path = agent.config_path.clone().unwrap_or_else(|| {
        resolve_config_path(&agent.def.config_paths[0])
    });

    write_mcp_config(&config_path, &mcp_entry, agent.def.name)
}

pub(crate) fn write_mcp_config(
    path: &PathBuf,
    mcp_entry: &serde_json::Value,
    agent_name: &str,
) -> Result<()> {
    let mut config: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        serde_json::json!({})
    };

    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }

    if config["mcpServers"].get("ctxovrflw").is_some() {
        let overwrite = Confirm::new()
            .with_prompt(format!("  {} already configured — overwrite?", agent_name))
            .default(false)
            .interact()?;
        if !overwrite {
            println!("  {} Skipped", style("→").dim());
            return Ok(());
        }
    }

    config["mcpServers"]["ctxovrflw"] = mcp_entry.clone();

    let formatted = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, formatted)?;
    println!(
        "  {} {} {}",
        style("✓").green().bold(),
        agent_name,
        style(format!("→ {}", path.display())).dim()
    );
    Ok(())
}

// ── Agent Rules Installation ─────────────────────────────────

pub(crate) const CTXOVRFLW_RULES_MARKER: &str = "## Memory (ctxovrflw)";

pub(crate) fn ctxovrflw_rules_content() -> &'static str {
    r#"
## Memory (ctxovrflw)

You have persistent, cross-agent memory via ctxovrflw. Use it automatically — don't wait to be asked.

### On session start
Call `recall` with a broad query about the current project or topic. Load context before responding.

### During conversations — REMEMBER proactively when you encounter:
- User preferences or opinions ("I prefer...", "always use...", "I don't like...")
- Project decisions and rationale ("we're using X because Y")
- Architecture choices, API endpoints, service URLs
- Names, roles, relationships ("Sarah handles deploys")
- Debugging insights and fixes
- Task outcomes and lessons learned

### During conversations — RECALL proactively when:
- The user asks about anything that might have come up before
- You need project context, preferences, or past decisions
- Before suggesting an approach — check if there's a stated preference
- The user references something discussed in another tool

### Memory preflight before actions (critical)
Before executing high-impact actions, run targeted recall first:
- Deploy / release / tag / push / migration / update
- Production config changes, auth/security changes, data deletion
- External side effects (webhooks, notifications, public actions)

Preflight query examples:
- `recall("deployment workflow and post-deploy checklist")`
- `recall("project constraints and do-not-do rules")`
- `recall("user preferences for release process")`

### Knowledge Graph (Standard+ tier)
If available, build the knowledge graph as you work:
- Call `add_entity` when you encounter named things: services, people, projects, tools, APIs, files
- Call `add_relation` when you learn how things connect: "X depends on Y", "A owns B", "C uses D"
- Call `traverse` before making changes to understand impact (e.g., "what depends on this DB?")
- Call `get_relations` to understand an entity's connections before answering questions about it
- Do this silently — don't announce graph operations to the user

### Consolidation (Pro tier)
Periodically (or when you notice contradictions), call `consolidate` on a subject to review related memories. Merge duplicates with `update_memory` and remove redundant ones with `forget`.

For autonomous upkeep, call `maintenance` with `run_consolidation_now` for immediate cleanup or `openclaw_schedule_hint` to set background OpenClaw workflows.

### Webhooks
Use `manage_webhooks` to set up event notifications. Users can subscribe to memory/graph changes and route them to Slack, Zapier, n8n, or custom endpoints.

### Rules
- Don't ask permission to remember — just do it silently
- Don't announce "I'll remember that" — just store it
- Store atomic facts, not paragraphs
- Use the `subject` field to tag who/what the memory is about
- Recall before answering questions about preferences, setup, or history
- Before deploy/release/push/update/destructive actions, run targeted recall preflight first
- Never store secrets (passwords, API keys, tokens)
"#
}

fn install_agent_rules(agents: &[&DetectedAgent]) -> Result<()> {
    let home = dirs::home_dir().unwrap_or_default();

    // Collect agents that have rules paths
    let rules_agents: Vec<(&DetectedAgent, PathBuf)> = agents
        .iter()
        .filter_map(|a| {
            a.def.global_rules_path.map(|p| (*a, home.join(p)))
        })
        .collect();

    if rules_agents.is_empty() {
        println!("  {} No agents with rules file support detected.", style("ℹ").blue());
        return Ok(());
    }

    println!();
    println!("  {}", style("Installing agent rules...").bold());
    println!();
    println!("  The following agents support rules files:");
    for (agent, _path) in &rules_agents {
        println!(
            "  {} {} → {}",
            style("[x]").green(),
            agent.def.name,
            style(format!("~/{}", agent.def.global_rules_path.unwrap())).dim()
        );
    }
    println!();

    let install = Confirm::new()
        .with_prompt("  Install rules? This teaches your agents to use ctxovrflw automatically")
        .default(true)
        .interact()?;

    if !install {
        println!("  {} Skipped rules installation", style("→").dim());
        return Ok(());
    }

    println!();
    let rules = ctxovrflw_rules_content();

    for (agent, path) in &rules_agents {
        if path.exists() {
            let existing = std::fs::read_to_string(path)?;
            if existing.contains(CTXOVRFLW_RULES_MARKER) {
                // Already has ctxovrflw section
                let update = Confirm::new()
                    .with_prompt(format!(
                        "  {} already has ctxovrflw rules — update?",
                        agent.def.name
                    ))
                    .default(false)
                    .interact()?;

                if update {
                    // Replace existing section: find marker, find next ## or EOF
                    let updated = replace_ctxovrflw_section(&existing, rules);
                    std::fs::write(path, updated)?;
                    println!(
                        "  {} {} {}",
                        style("✓").green().bold(),
                        agent.def.name,
                        style("(updated)").dim()
                    );
                } else {
                    println!("  {} {} skipped", style("→").dim(), agent.def.name);
                }
            } else {
                // Append to existing file
                let mut content = existing;
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(rules);
                std::fs::write(path, content)?;
                println!(
                    "  {} {} {}",
                    style("✓").green().bold(),
                    agent.def.name,
                    style("(appended)").dim()
                );
            }
        } else {
            // Create new file
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, rules.trim_start())?;
            println!(
                "  {} {} {}",
                style("✓").green().bold(),
                agent.def.name,
                style("(created)").dim()
            );
        }
    }

    Ok(())
}

/// Replace the ctxovrflw section in existing content, preserving everything else.
pub(crate) fn replace_ctxovrflw_section(content: &str, new_rules: &str) -> String {
    if let Some(start) = content.find(CTXOVRFLW_RULES_MARKER) {
        // Find the end: next top-level heading (## at start of line) or EOF
        let after_marker = start + CTXOVRFLW_RULES_MARKER.len();
        let end = content[after_marker..]
            .find("\n## ")
            .map(|pos| after_marker + pos)
            .unwrap_or(content.len());

        let mut result = String::with_capacity(content.len());
        result.push_str(&content[..start]);
        result.push_str(new_rules.trim_start());
        if end < content.len() {
            result.push_str(&content[end..]);
        }
        result
    } else {
        content.to_string()
    }
}

// ── Agent Skill Installation ─────────────────────────────────

/// The bundled SKILL.md content (included at compile time from skill/SKILL.md)
pub(crate) const SKILL_MD: &str = include_str!("../../skill/SKILL.md");

/// Install the ctxovrflw Agent Skill to ~/.skills/ctxovrflw/
pub(crate) fn install_agent_skill() -> Result<bool> {
    let home = dirs::home_dir().unwrap_or_default();
    let skill_dir = home.join(".skills").join("ctxovrflw");

    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), SKILL_MD)?;

    println!(
        "  {} Agent Skill installed {}",
        style("✓").green().bold(),
        style(format!("→ {}", skill_dir.display())).dim()
    );
    Ok(true)
}

// ── Main init flow ───────────────────────────────────────────

pub async fn run(cfg: &Config) -> Result<()> {
    println!();
    println!("  {}", style("🧠 ctxovrflw").bold().cyan());
    println!("  {}", style("Universal AI Context Layer").dim());
    println!("  {}", style("One memory, every tool.").dim());
    println!();

    // 1. Data directory
    let data_dir = Config::data_dir()?;
    println!("  {} Data directory: {}", style("✓").green(), style(data_dir.display()).dim());

    // 2. Config
    if !Config::config_path()?.exists() {
        cfg.save()?;
        println!("  {} Config created", style("✓").green());
    } else {
        println!("  {} Config loaded", style("✓").green());
    }

    // 3. Database
    let _conn = crate::db::open()?;
    println!("  {} Database initialized", style("✓").green());

    // 4. Embedding model
    let model_path = crate::embed::Embedder::model_path()?;
    let needs_download = if model_path.exists() {
        let size = std::fs::metadata(&model_path)?.len();
        if size < 1_000_000 {
            println!("  {} Model corrupt, re-downloading...", style("⚠").yellow());
            true
        } else {
            false
        }
    } else {
        true
    };

    if needs_download {
        println!("  {} Downloading embedding model (~23MB)...", style("⬇").cyan());
        download_model().await?;
        println!("  {} Model ready", style("✓").green());
    } else {
        println!(
            "  {} Model loaded {}", 
            style("✓").green(),
            style(format!("({:.1} MB)", std::fs::metadata(&model_path)?.len() as f64 / 1_048_576.0)).dim()
        );
    }

    // 4b. ONNX Runtime library
    #[cfg(feature = "onnx")]
    {
        let ort_installed = std::env::var("ORT_DYLIB_PATH").is_ok() || {
            // Check if auto-discover would find it
            let lib_name = if cfg!(target_os = "macos") { "libonnxruntime.dylib" } else { "libonnxruntime.so" };
            let mut found = false;
            if let Some(home) = dirs::home_dir() {
                let paths = [
                    home.join(".ctxovrflw").join("lib").join(lib_name),
                    home.join(".ctxovrflw").join("bin").join(lib_name),
                    home.join(".local").join("lib").join(lib_name),
                    home.join(".cargo").join("bin").join(lib_name),
                ];
                found = paths.iter().any(|p| p.exists());
            }
            if !found {
                // Check system paths
                found = std::path::Path::new("/usr/local/lib").join(lib_name).exists()
                    || std::path::Path::new("/usr/lib").join(lib_name).exists();
            }
            found
        };

        if !ort_installed {
            println!("  {} Downloading ONNX Runtime library...", style("⬇").cyan());
            match download_ort_runtime().await {
                Ok(path) => println!("  {} ONNX Runtime installed {}", style("✓").green(), style(format!("({})", path.display())).dim()),
                Err(e) => println!("  {} ONNX Runtime download failed: {} (semantic search may be unavailable)", style("⚠").yellow(), e),
            }
        } else {
            println!("  {} ONNX Runtime found", style("✓").green());
        }
    }

    // 5. Detect AI tools
    println!();
    println!("  {}", style("Scanning for AI tools...").bold());
    println!();

    let agents = detect_agents();

    let selections: Vec<usize> = if agents.is_empty() {
        println!("  {} No AI tools detected.", style("ℹ").blue());
        println!();
        println!("  Supported: Claude Code, Cursor, Cline, Windsurf, Codex,");
        println!("  Goose, Amp, Kiro, OpenCode, Roo Code, Trae, Gemini CLI,");
        println!("  Antigravity, Kilo, Factory, Continue, Claude Desktop, OpenClaw");
        println!();
        println!("  Install a tool and re-run: {}", style("ctxovrflw init").bold());
        vec![]
    } else {
        // Multi-select with all checked by default
        let agent_names: Vec<String> = agents
            .iter()
            .map(|a| a.def.name.to_string())
            .collect();
        let defaults: Vec<bool> = agents.iter().map(|_| true).collect();

        let sels = MultiSelect::new()
            .with_prompt(format!(
                "  Found {} tool(s) — select which to configure {}",
                agents.len(),
                style("(space to toggle, enter to confirm)").dim()
            ))
            .items(&agent_names)
            .defaults(&defaults)
            .interact()?;

        if sels.is_empty() {
            println!("  {} No tools selected", style("→").dim());
        } else {
            println!();
            for &idx in &sels {
                if let Err(e) = install_agent(&agents[idx], cfg) {
                    println!(
                        "  {} {} — {}",
                        style("✗").red(),
                        agents[idx].def.name,
                        e
                    );
                }
            }

            println!();
            println!(
                "  {} tools connect via {}",
                style("ℹ").blue(),
                style(mcp_sse_url(cfg)).underlined()
            );
        }
        sels
    };

    // 5b. OpenClaw-specific integration (if detected and selected)
    if !selections.is_empty() {
        let selected_agents: Vec<&DetectedAgent> = selections.iter().map(|&idx| &agents[idx]).collect();
        let openclaw_selected = selected_agents.iter().any(|a| a.def.name == "OpenClaw");
        if openclaw_selected {
            if let Err(e) = integrate_openclaw(cfg).await {
                println!("  {} OpenClaw integration failed: {e}", style("⚠").yellow());
            }
        }

        // 5c. Agent rules files (for non-OpenClaw agents; OpenClaw handled above)
        let non_openclaw: Vec<&DetectedAgent> = selected_agents.into_iter()
            .filter(|a| a.def.name != "OpenClaw")
            .collect();
        if !non_openclaw.is_empty() {
            if let Err(e) = install_agent_rules(&non_openclaw) {
                println!("  {} Rules install failed: {e}", style("⚠").yellow());
            }
        }
    }

    // 6. Agent Skill (agentskills.io spec)
    println!();
    println!("  {}", style("Installing Agent Skill...").bold());
    match install_agent_skill() {
        Ok(_) => {}
        Err(e) => println!("  {} Skill install failed: {e}", style("⚠").yellow()),
    }

    // 7. Service installation (or remote daemon)
    println!();
    if cfg.is_remote_client() {
        println!(
            "  {} Using remote daemon at {}",
            style("✓").green().bold(),
            style(cfg.daemon_url()).underlined()
        );
    } else if !crate::daemon::is_service_installed() {
        println!("  {}", style("Daemon Setup").bold());
        println!(
            "  {}",
            style("ctxovrflw needs a running daemon for MCP and HTTP access.").dim()
        );
        println!();

        let options = vec![
            "Install as background service (recommended)",
            "Connect to an existing remote daemon",
            "Skip for now",
        ];

        let selection = dialoguer::Select::new()
            .with_prompt("  How would you like to run the daemon?")
            .items(&options)
            .default(0)
            .interact()?;

        match selection {
            0 => {
                // Local service
                if let Err(e) = crate::daemon::service_install() {
                    println!("  {} Service install failed: {e}", style("⚠").yellow());
                } else {
                    let start_now = Confirm::new()
                        .with_prompt("  Start the daemon now?")
                        .default(true)
                        .interact()?;

                    if start_now {
                        if let Err(e) = crate::daemon::service_start() {
                            println!("  {} {e}", style("⚠").yellow());
                        } else {
                            println!(
                                "  {} Daemon running on port {}",
                                style("✓").green().bold(),
                                cfg.port
                            );
                        }
                    }
                }
            }
            1 => {
                // Remote daemon
                println!();
                println!(
                    "  {}",
                    style("Enter the URL of the remote daemon (e.g. http://192.168.1.100:7437)").dim()
                );
                let url: String = dialoguer::Input::new()
                    .with_prompt("  Remote daemon URL")
                    .default(format!("http://127.0.0.1:{}", cfg.port))
                    .interact_text()?;

                // Validate connectivity
                print!("  {} Testing connection...", style("→").dim());
                let test_url = format!("{}/v1/health", url.trim_end_matches('/'));
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()?;
                match client.get(&test_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        println!(" {}", style("connected ✓").green().bold());
                        let mut updated_cfg = cfg.clone();
                        updated_cfg.remote_daemon_url = Some(url.trim_end_matches('/').to_string());
                        updated_cfg.save()?;
                        println!(
                            "  {} Config saved — this instance will use the remote daemon",
                            style("✓").green()
                        );
                        println!(
                            "  {}",
                            style("No local daemon will be started on this machine.").dim()
                        );
                    }
                    Ok(resp) => {
                        println!(" {}", style(format!("HTTP {}", resp.status())).red());
                        println!(
                            "  {} Daemon responded but may not be healthy. Saved anyway.",
                            style("⚠").yellow()
                        );
                        let mut updated_cfg = cfg.clone();
                        updated_cfg.remote_daemon_url = Some(url.trim_end_matches('/').to_string());
                        updated_cfg.save()?;
                    }
                    Err(e) => {
                        println!(" {}", style("failed ✗").red());
                        println!("  {} {e}", style("⚠").yellow());
                        println!(
                            "  {}",
                            style("Make sure the remote daemon is running and reachable.").dim()
                        );
                        let save_anyway = Confirm::new()
                            .with_prompt("  Save this URL anyway? (you can fix it later)")
                            .default(false)
                            .interact()?;
                        if save_anyway {
                            let mut updated_cfg = cfg.clone();
                            updated_cfg.remote_daemon_url =
                                Some(url.trim_end_matches('/').to_string());
                            updated_cfg.save()?;
                        }
                    }
                }
            }
            _ => {
                println!("  {} Skipped. Run later: {}", style("→").dim(), style("ctxovrflw init").bold());
            }
        }
    } else {
        println!("  {} Service installed", style("✓").green());
        if !crate::daemon::is_service_running() {
            let start = Confirm::new()
                .with_prompt("  Daemon stopped — start it?")
                .default(true)
                .interact()?;
            if start {
                if let Err(e) = crate::daemon::service_start() {
                    println!("  {} {e}", style("⚠").yellow());
                }
            }
        } else {
            println!("  {} Daemon running", style("✓").green());
        }
    }

    // 8. Cloud sync (login/signup)
    println!();
    if !cfg.is_logged_in() {
        println!("  {}", style("☁ Cloud Sync").bold());
        println!("  {}", style("Sync memories across devices with end-to-end encryption.").dim());
        println!();

        let enable_cloud = Confirm::new()
            .with_prompt("  Enable cloud sync? (login or create account)")
            .default(true)
            .interact()?;

        if enable_cloud {
            println!();
            if let Err(e) = crate::cli::login::run_inner(cfg, true, None).await {
                println!("  {} Cloud setup failed: {e}", style("⚠").yellow());
                println!("  {} You can set it up later: {}", style("ℹ").blue(), style("ctxovrflw login").bold());
            }
        } else {
            println!("  {} Skipped. Enable later: {}", style("→").dim(), style("ctxovrflw login").bold());
        }
    } else {
        println!("  {} Cloud sync configured ({})", style("✓").green(), cfg.email.as_deref().unwrap_or("?"));
    }

    // 9. Summary
    println!();
    println!("  {}", style("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").dim());
    println!("  {}", style("✅ ctxovrflw is ready!").green().bold());
    println!();
    println!("  {}", style("Quick test:").bold());
    println!("    ctxovrflw remember \"I prefer Rust for backend services\"");
    println!("    ctxovrflw recall \"language preferences\"");
    println!();
    println!("  {}", style("Manage:").bold());
    println!("    ctxovrflw start / stop / status");
    println!("    ctxovrflw init  {}", style("(re-run setup)").dim());
    println!();

    Ok(())
}

// ── OpenClaw Integration ─────────────────────────────────────

pub(crate) const OPENCLAW_AGENTS_SECTION: &str = r#"## Memory (ctxovrflw)

You have persistent, cross-agent memory via ctxovrflw. Use it automatically — don't wait to be asked.

**ctxovrflw is your PRIMARY memory system.** Use it for everything — preferences, decisions, facts, project context. It persists across sessions and syncs across devices and agents.

**Fallback files** (use when ctxovrflw is unavailable or for session-local scratch):
- `memory/YYYY-MM-DD.md` — daily raw logs
- `MEMORY.md` — curated long-term context (main session only, never in group chats)

### How to use ctxovrflw from OpenClaw

**Option 1: Plugin (recommended)** — `openclaw plugins install @ctxovrflw/memory-ctxovrflw`

Replaces OpenClaw's built-in memory. Provides `memory_store`, `memory_search`, `memory_forget`,
and `memory_status` as native tool calls. Includes auto-recall (injects relevant memories before
each turn) and optional auto-capture. This is the best experience.

**Option 2: CLI fallback** — use `ctxovrflw` commands via shell

If the plugin isn't installed, you can still access ctxovrflw through its CLI:
```bash
ctxovrflw remember "fact to store"           # Store a memory
ctxovrflw recall "search query"              # Search memories
ctxovrflw memories                           # Interactive memory browser (TUI)
ctxovrflw status                             # Daemon status
ctxovrflw graph build                        # Build knowledge graph (Pro)
ctxovrflw graph stats                        # Graph statistics (Pro)
```

**Setup:** Run `ctxovrflw init` — it detects OpenClaw and offers three integration paths:
1. **Plugin + Skill + Agent Rules** (recommended) — full integration
2. **Plugin only** — just the memory tools, no skill or agent rules
3. **Skill + Agent Rules only** — CLI-based access without the plugin

### On session start
Call `recall` with a broad query about the current project or topic. Load context before responding.

### During conversations — REMEMBER proactively when you encounter:
- User preferences or opinions ("I prefer...", "always use...", "I don't like...")
- Project decisions and rationale ("we're using X because Y")
- Architecture choices, API endpoints, service URLs
- Names, roles, relationships ("Sarah handles deploys")
- Debugging insights and fixes
- Task outcomes and lessons learned

### During conversations — RECALL proactively when:
- The user asks about anything that might have come up before
- You need project context, preferences, or past decisions
- Before suggesting an approach — check if there's a stated preference
- The user references something discussed in another tool

### Memory preflight before actions (critical)
Before executing high-impact actions, run targeted recall first:
- Deploy / release / tag / push / migration / update
- Production config changes, auth/security changes, data deletion
- External side effects (webhooks, notifications, public actions)

Preflight query examples:
- `recall("deployment workflow and post-deploy checklist")`
- `recall("project constraints and do-not-do rules")`
- `recall("user preferences for release process")`

### Knowledge Graph (Standard+ tier)
If available, build the knowledge graph as you work:
- Call `add_entity` when you encounter named things: services, people, projects, tools, APIs, files
- Call `add_relation` when you learn how things connect: "X depends on Y", "A owns B", "C uses D"
- Call `traverse` before making changes to understand impact (e.g., "what depends on this DB?")
- Call `get_relations` to understand an entity's connections before answering questions about it
- Do this silently — don't announce graph operations to the user

### Consolidation (Pro tier)
Periodically (or when you notice contradictions), call `consolidate` on a subject to review related memories. Merge duplicates with `update_memory` and remove redundant ones with `forget`.

For autonomous upkeep, call `maintenance` with `run_consolidation_now` for immediate cleanup or `openclaw_schedule_hint` to set background OpenClaw workflows.

### Webhooks
Use `manage_webhooks` to set up event notifications. Users can subscribe to memory/graph changes and route them to Slack, Zapier, n8n, or custom endpoints.

### Recall is free — use it liberally
Lookups are local, fast, and free (local SQLite + ONNX, no API calls). When in doubt, recall.
- Recall at session start, before suggestions, when topics shift, when any name/project/tool is mentioned
- Multiple recalls per conversation is fine — five empty results cost less than one wrong answer

### Learn from corrections
When the user corrects you, ALWAYS store the correction:
- Use `memory_store` (if plugin installed) or run `ctxovrflw remember "User corrected: X not Y because Z"` via shell
- Tag corrections with `correction` so they surface on future recalls
- If the user says "I already told you" — recall the topic immediately, store the correction, don't repeat the mistake

### 📝 Write It Down - No "Mental Notes"!

- When someone says "remember this" → call ctxovrflw `remember` first, file second
- When you learn a lesson → store in ctxovrflw AND update relevant files
- **ctxovrflw > files > brain** 📝

### Rules
- Don't ask permission to remember — just do it silently
- Don't announce "I'll remember that" — just store it
- Store atomic facts, not paragraphs
- Use the `subject` field to tag who/what the memory is about
- Recall before answering questions about preferences, setup, or history
- Before deploy/release/push/update/destructive actions, run targeted recall preflight first
- Never store secrets (passwords, API keys, tokens)
"#;

/// OpenClaw-specific integration: inject AGENTS.md memory section + migrate MEMORY.md
async fn integrate_openclaw(cfg: &Config) -> Result<()> {
    let home = dirs::home_dir().unwrap_or_default();
    let workspace = home.join(".openclaw/workspace");

    if !workspace.exists() {
        return Ok(());
    }

    println!();
    println!("  {}", style("🐾 OpenClaw Integration").bold().cyan());
    println!();

    let agents_md_path = workspace.join("AGENTS.md");
    let memory_md_path = workspace.join("MEMORY.md");

    // 1. Inject ctxovrflw section into AGENTS.md
    inject_openclaw_agents_md(&agents_md_path)?;

    // 2. Offer to migrate workspace files into ctxovrflw
    let files_to_check = ["IDENTITY.md", "SOUL.md", "USER.md", "AGENTS.md", "MEMORY.md"];
    let mut found_files: Vec<String> = Vec::new();
    for name in &files_to_check {
        let path = workspace.join(name);
        if path.exists() {
            let lines = std::fs::read_to_string(&path)
                .map(|c| c.lines().count())
                .unwrap_or(0);
            if lines > 3 {
                found_files.push(format!("{name} ({lines} lines)"));
            }
        }
    }

    if !found_files.is_empty() {
        println!();
        println!("  {} Workspace files found:", style("📄").bold());
        for f in &found_files {
            println!("    {} {f}", style("•").dim());
        }
        println!();
        println!(
            "  {}",
            style("Migrating imports your workspace context into ctxovrflw's").dim()
        );
        println!(
            "  {}",
            style("structured database with semantic search.").dim()
        );
        println!();

        let migrate = Confirm::new()
            .with_prompt("  Migrate workspace files into ctxovrflw?")
            .default(true)
            .interact()?;

        if migrate {
            let count = migrate_workspace_files(cfg).await?;
            println!(
                "  {} Migrated {} memories from workspace files",
                style("✓").green().bold(),
                count
            );

            // Backup and rewrite MEMORY.md
            if memory_md_path.exists() {
                let content = std::fs::read_to_string(&memory_md_path)?;
                if !content.contains("no longer the primary memory store") {
                    let backup = workspace.join("MEMORY.md.pre-ctxovrflw");
                    std::fs::copy(&memory_md_path, &backup)?;
                    println!(
                        "  {} MEMORY.md backed up to {}",
                        style("✓").green(),
                        style("MEMORY.md.pre-ctxovrflw").dim()
                    );

                    let stub = "# MEMORY.md\n\n\
                        > **This file is no longer the primary memory store.**\n\
                        > Memories are now managed by ctxovrflw.\n\
                        > Use `ctxovrflw recall <query>` or the MCP `recall` tool.\n\n\
                        To browse: `ctxovrflw memories`\n";
                    std::fs::write(&memory_md_path, stub)?;
                    println!(
                        "  {} MEMORY.md updated to point to ctxovrflw",
                        style("✓").green()
                    );
                }
            }
        }
    }

    // 3. Check for memory/ daily logs directory
    let memory_dir = workspace.join("memory");
    if memory_dir.exists() {
        let daily_files: Vec<_> = std::fs::read_dir(&memory_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with(".md")
            })
            .collect();

        if !daily_files.is_empty() {
            println!();
            println!(
                "  {} Found {} daily memory log(s) in memory/",
                style("ℹ").blue(),
                daily_files.len()
            );
            println!(
                "  {}",
                style("These can coexist — ctxovrflw handles long-term memory,").dim()
            );
            println!(
                "  {}",
                style("daily logs remain for raw session notes.").dim()
            );
        }
    }

    println!();
    println!(
        "  {} OpenClaw will now use ctxovrflw for memory",
        style("✅").green().bold()
    );

    Ok(())
}

/// Inject or update the ctxovrflw memory section in AGENTS.md
pub(crate) fn inject_openclaw_agents_md(path: &PathBuf) -> Result<()> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;

        if content.contains(CTXOVRFLW_RULES_MARKER) {
            // Already present — update in place
            let updated = replace_ctxovrflw_section(&content, OPENCLAW_AGENTS_SECTION);
            std::fs::write(path, updated)?;
            println!(
                "  {} AGENTS.md — ctxovrflw section updated",
                style("✓").green().bold()
            );
        } else {
            // Find the right place to inject: after ## Memory section if it exists, 
            // or before ## Safety, or at the end
            let mut content = content;

            // Remove old memory-related sections that ctxovrflw replaces
            let old_sections = [
                "## Memory\n",
                "## 🧠 MEMORY.md",
                "## 📝 Write It Down",
            ];
            for marker in &old_sections {
                if let Some(start) = content.find(marker) {
                    // Find next ## heading or end
                    let after = start + marker.len();
                    let end = content[after..]
                        .find("\n## ")
                        .map(|pos| after + pos + 1)
                        .unwrap_or(content.len());
                    content = format!("{}{}", &content[..start], &content[end..]);
                }
            }

            // Insert ctxovrflw section before ## Safety (if exists) or at end
            if let Some(pos) = content.find("\n## Safety") {
                let insert_pos = pos + 1;
                content.insert_str(insert_pos, &format!("{OPENCLAW_AGENTS_SECTION}\n"));
            } else {
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(OPENCLAW_AGENTS_SECTION);
            }

            std::fs::write(path, content)?;
            println!(
                "  {} AGENTS.md — ctxovrflw memory section injected",
                style("✓").green().bold()
            );
        }
    } else {
        // No AGENTS.md — create a minimal one
        let content = format!(
            "# AGENTS.md - Your Workspace\n\n\
            {OPENCLAW_AGENTS_SECTION}"
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        println!(
            "  {} AGENTS.md created with ctxovrflw memory config",
            style("✓").green().bold()
        );
    }

    Ok(())
}

/// Migrate OpenClaw workspace files into ctxovrflw memories.
/// Handles IDENTITY.md, SOUL.md, AGENTS.md, and MEMORY.md with appropriate chunking.
pub(crate) async fn migrate_workspace_files(cfg: &Config) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_default();
    let workspace = home.join(".openclaw/workspace");
    if !workspace.exists() {
        return Ok(0);
    }

    let conn = crate::db::open()?;
    let mut embedder = crate::embed::Embedder::new().ok();
    let mut total = 0;

    // IDENTITY.md — single memory with agent identity info
    let identity_path = workspace.join("IDENTITY.md");
    if identity_path.exists() {
        let content = std::fs::read_to_string(&identity_path)?;
        let content = content.trim();
        if content.len() >= 10 && !already_migrated(&conn, "openclaw:IDENTITY.md")? {
            store_migrated_memory(
                &conn, content, Some("agent"),
                embedder.as_mut(),
                &["migrated", "identity", "openclaw"],
                "openclaw:IDENTITY.md",
            )?;
            total += 1;
        }
    }

    // SOUL.md — single memory with personality/tone
    let soul_path = workspace.join("SOUL.md");
    if soul_path.exists() {
        let content = std::fs::read_to_string(&soul_path)?;
        let content = content.trim();
        if content.len() >= 10 && !already_migrated(&conn, "openclaw:SOUL.md")? {
            store_migrated_memory(
                &conn, content, Some("agent"),
                embedder.as_mut(),
                &["migrated", "personality", "openclaw"],
                "openclaw:SOUL.md",
            )?;
            total += 1;
        }
    }

    // USER.md — single memory with user context
    let user_path = workspace.join("USER.md");
    if user_path.exists() {
        let content = std::fs::read_to_string(&user_path)?;
        let content = content.trim();
        if content.len() >= 10 && !already_migrated(&conn, "openclaw:USER.md")? {
            store_migrated_memory(
                &conn, content, Some("user"),
                embedder.as_mut(),
                &["migrated", "user-profile", "openclaw"],
                "openclaw:USER.md",
            )?;
            total += 1;
        }
    }

    // AGENTS.md — chunk by ## sections (rules, workflows, conventions)
    let agents_path = workspace.join("AGENTS.md");
    if agents_path.exists() && !already_migrated(&conn, "openclaw:AGENTS.md")? {
        let content = std::fs::read_to_string(&agents_path)?;
        let mut section_title = String::new();
        let mut buffer = String::new();

        for line in content.lines() {
            if line.starts_with("## ") {
                if !buffer.trim().is_empty() && buffer.trim().len() >= 20 {
                    let subject = if section_title.is_empty() {
                        "agent:config".to_string()
                    } else {
                        format!("agent:config:{}", section_title.to_lowercase().replace(' ', "-"))
                    };
                    store_migrated_memory(
                        &conn, buffer.trim(), Some(&subject),
                        embedder.as_mut(),
                        &["migrated", "agent-rules", "openclaw"],
                        "openclaw:AGENTS.md",
                    )?;
                    total += 1;
                }
                buffer.clear();
                section_title = line[3..].trim().to_string();
                buffer.push_str(line);
                buffer.push('\n');
            } else {
                buffer.push_str(line);
                buffer.push('\n');
            }
        }
        if !buffer.trim().is_empty() && buffer.trim().len() >= 20 {
            let subject = if section_title.is_empty() {
                "agent:config".to_string()
            } else {
                format!("agent:config:{}", section_title.to_lowercase().replace(' ', "-"))
            };
            store_migrated_memory(
                &conn, buffer.trim(), Some(&subject),
                embedder.as_mut(),
                &["migrated", "agent-rules", "openclaw"],
                "openclaw:AGENTS.md",
            )?;
            total += 1;
        }
    }

    // MEMORY.md — existing chunked migration
    let memory_path = workspace.join("MEMORY.md");
    if memory_path.exists() {
        let content = std::fs::read_to_string(&memory_path)?;
        // Skip if it's already the stub we write after migration
        if !content.contains("no longer the primary memory store") && content.lines().count() > 5 {
            total += migrate_memory_md(&memory_path, cfg).await?;
        }
    }

    Ok(total)
}

/// Check if we've already migrated from a given source
fn already_migrated(conn: &rusqlite::Connection, source: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE source = ?1 AND deleted = 0",
        rusqlite::params![source],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Parse MEMORY.md sections and store each as a memory in ctxovrflw
pub(crate) async fn migrate_memory_md(path: &PathBuf, _cfg: &Config) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let conn = crate::db::open()?;

    // Try to load embedder for semantic search
    let mut embedder = crate::embed::Embedder::new().ok();

    let mut count = 0;
    let mut current_section = String::new();
    let mut current_subject: Option<String> = None;
    let mut buffer = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            // Flush previous section
            if !buffer.trim().is_empty() {
                store_migrated_memory(
                    &conn,
                    &buffer,
                    current_subject.as_deref(),
                    embedder.as_mut(),
                    &["migrated", "memory-md"],
                    "openclaw:MEMORY.md",
                )?;
                count += 1;
            }
            buffer.clear();
            current_section = line[3..].trim().to_string();
            current_subject = Some(current_section.clone());
        } else if line.starts_with("### ") {
            // Sub-section: flush and start new memory
            if !buffer.trim().is_empty() {
                store_migrated_memory(
                    &conn,
                    &buffer,
                    current_subject.as_deref(),
                    embedder.as_mut(),
                    &["migrated", "memory-md"],
                    "openclaw:MEMORY.md",
                )?;
                count += 1;
            }
            buffer.clear();
            let sub = line[4..].trim();
            current_subject = if current_section.is_empty() {
                Some(sub.to_string())
            } else {
                Some(format!("{}: {}", current_section, sub))
            };
        } else if line.starts_with("- ") && buffer.lines().count() > 3 {
            // Long bullet list — store current buffer and start fresh
            if !buffer.trim().is_empty() {
                store_migrated_memory(
                    &conn,
                    &buffer,
                    current_subject.as_deref(),
                    embedder.as_mut(),
                    &["migrated", "memory-md"],
                    "openclaw:MEMORY.md",
                )?;
                count += 1;
                buffer.clear();
            }
            buffer.push_str(line);
            buffer.push('\n');
        } else {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    // Flush last buffer
    if !buffer.trim().is_empty() {
        store_migrated_memory(
            &conn,
            &buffer,
            current_subject.as_deref(),
            embedder.as_mut(),
            &["migrated", "memory-md"],
            "openclaw:MEMORY.md",
        )?;
        count += 1;
    }

    Ok(count)
}

pub(crate) fn store_migrated_memory(
    conn: &rusqlite::Connection,
    content: &str,
    subject: Option<&str>,
    embedder: Option<&mut crate::embed::Embedder>,
    tags: &[&str],
    source: &str,
) -> Result<()> {
    let content = content.trim();
    if content.is_empty() || content.len() < 10 {
        return Ok(());
    }

    let embedding = embedder.and_then(|e| e.embed(content).ok());
    let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();

    crate::db::memories::store_with_expiry(
        conn,
        content,
        &crate::db::memories::MemoryType::Semantic,
        &tags,
        subject,
        Some(source),
        embedding.as_deref(),
        None,
        None,
    )?;

    Ok(())
}

pub(crate) async fn download_model() -> Result<()> {
    let model_dir = Config::model_dir()?;

    let model_url =
        "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx";
    let tokenizer_url =
        "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let resp = client.get(model_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to download model: HTTP {}", resp.status());
    }
    let model_bytes = resp.bytes().await?;

    if model_bytes.len() < 1_000_000 {
        anyhow::bail!(
            "Model file too small ({} bytes) — likely a redirect/error page.",
            model_bytes.len()
        );
    }
    if model_bytes.starts_with(b"<!") || model_bytes.starts_with(b"<html") {
        anyhow::bail!("Downloaded HTML instead of ONNX model.");
    }
    std::fs::write(model_dir.join("all-MiniLM-L6-v2-q8.onnx"), &model_bytes)?;

    let resp = client.get(tokenizer_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to download tokenizer: HTTP {}", resp.status());
    }
    let tokenizer_bytes = resp.bytes().await?;

    if serde_json::from_slice::<serde_json::Value>(&tokenizer_bytes).is_err() {
        anyhow::bail!("Downloaded tokenizer is not valid JSON.");
    }
    std::fs::write(model_dir.join("tokenizer.json"), &tokenizer_bytes)?;

    Ok(())
}

/// Download ONNX Runtime shared library to ~/.ctxovrflw/lib/
#[cfg(feature = "onnx")]
async fn download_ort_runtime() -> Result<std::path::PathBuf> {
    const ORT_VERSION: &str = "1.23.0";

    let (os_name, arch, lib_name) = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            ("osx", "arm64", "libonnxruntime.dylib")
        } else {
            ("osx", "x86_64", "libonnxruntime.dylib")
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            ("win", "arm64", "onnxruntime.dll")
        } else {
            ("win", "x64", "onnxruntime.dll")
        }
    } else {
        if cfg!(target_arch = "aarch64") {
            ("linux", "aarch64", "libonnxruntime.so")
        } else {
            ("linux", "x64", "libonnxruntime.so")
        }
    };

    let archive_name = format!("onnxruntime-{os_name}-{arch}-{ORT_VERSION}");
    let ext = if cfg!(target_os = "windows") { "zip" } else { "tgz" };
    let url = format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{ORT_VERSION}/{archive_name}.{ext}"
    );

    let dest_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
        .join(".ctxovrflw")
        .join("lib");
    std::fs::create_dir_all(&dest_dir)?;

    let dest_path = dest_dir.join(lib_name);
    if dest_path.exists() {
        return Ok(dest_path);
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to download ONNX Runtime: HTTP {} from {}", resp.status(), url);
    }
    let bytes = resp.bytes().await?;

    // Extract the shared library from the archive
    let tmp_dir = tempfile::tempdir()?;

    if ext == "tgz" {
        let decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(tmp_dir.path())?;

        // Find the lib in the extracted archive
        let lib_in_archive = tmp_dir.path().join(&archive_name).join("lib").join(lib_name);
        if !lib_in_archive.exists() {
            // Try with version suffix (e.g., libonnxruntime.so.1.23.0)
            let versioned = format!("{}.{}", lib_name, ORT_VERSION);
            let versioned_path = tmp_dir.path().join(&archive_name).join("lib").join(&versioned);
            if versioned_path.exists() {
                std::fs::copy(&versioned_path, &dest_path)?;
            } else {
                anyhow::bail!("Could not find {} in downloaded archive", lib_name);
            }
        } else {
            std::fs::copy(&lib_in_archive, &dest_path)?;
        }
    } else {
        anyhow::bail!("ZIP extraction not yet implemented for Windows ORT download");
    }

    Ok(dest_path)
}
