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
    Home(&'static str),
    Config(&'static str),
    MacApp(&'static str),
    AppData(&'static str),
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
        detect: DetectMethod::ConfigDir("continue"),
        config_paths: &[ConfigLocation::Config("continue/config.json")],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Codex CLI",
        detect: DetectMethod::Binary("codex"),
        config_paths: &[ConfigLocation::Config("codex/mcp.json")],
        cli_install: None,
        global_rules_path: Some("codex.md"),
    },
    AgentDef {
        name: "Goose",
        detect: DetectMethod::Any(&["goose", "goosed"]),
        config_paths: &[ConfigLocation::Config("goose/config.json")],
        cli_install: None,
        global_rules_path: None,
    },
    AgentDef {
        name: "Gemini CLI",
        detect: DetectMethod::Binary("gemini"),
        config_paths: &[ConfigLocation::Config("gemini/settings.json")],
        cli_install: None,
        global_rules_path: None,
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
        config_paths: &[ConfigLocation::Config("amp/mcp.json")],
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
        config_paths: &[ConfigLocation::Config("opencode/mcp.json")],
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
        config_paths: &[ConfigLocation::Config("factory/mcp.json")],
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
            DetectMethod::ConfigDir(rel) => config_dir.join(rel).exists(),
            DetectMethod::Any(names) => names.iter().any(|n| which(n)),
        };

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
        ConfigLocation::Config(rel) => config_dir.join(rel),
        ConfigLocation::MacApp(rel) => home.join("Library/Application Support").join(rel),
        ConfigLocation::AppData(rel) => std::env::var("APPDATA")
            .map(|a| PathBuf::from(a).join(rel))
            .unwrap_or_else(|_| home.join(rel)),
    }
}

pub(crate) fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
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

### Knowledge Graph (Pro tier)
If available, build the knowledge graph as you work:
- Call `add_entity` when you encounter named things: services, people, projects, tools, APIs, files
- Call `add_relation` when you learn how things connect: "X depends on Y", "A owns B", "C uses D"
- Call `traverse` before making changes to understand impact (e.g., "what depends on this DB?")
- Call `get_relations` to understand an entity's connections before answering questions about it
- Do this silently — don't announce graph operations to the user

### Consolidation (Pro tier)
Periodically (or when you notice contradictions), call `consolidate` on a subject to review related memories. Merge duplicates with `update_memory` and remove redundant ones with `forget`.

### Webhooks
Use `manage_webhooks` to set up event notifications. Users can subscribe to memory/graph changes and route them to Slack, Zapier, n8n, or custom endpoints.

### Rules
- Don't ask permission to remember — just do it silently
- Don't announce "I'll remember that" — just store it
- Store atomic facts, not paragraphs
- Use the `subject` field to tag who/what the memory is about
- Recall before answering questions about preferences, setup, or history
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

### Knowledge Graph (Pro tier)
If available, build the knowledge graph as you work:
- Call `add_entity` when you encounter named things: services, people, projects, tools, APIs, files
- Call `add_relation` when you learn how things connect: "X depends on Y", "A owns B", "C uses D"
- Call `traverse` before making changes to understand impact (e.g., "what depends on this DB?")
- Call `get_relations` to understand an entity's connections before answering questions about it
- Do this silently — don't announce graph operations to the user

### Consolidation (Pro tier)
Periodically (or when you notice contradictions), call `consolidate` on a subject to review related memories. Merge duplicates with `update_memory` and remove redundant ones with `forget`.

### Webhooks
Use `manage_webhooks` to set up event notifications. Users can subscribe to memory/graph changes and route them to Slack, Zapier, n8n, or custom endpoints.

### Rules
- Don't ask permission to remember — just do it silently
- Don't announce "I'll remember that" — just store it
- Store atomic facts, not paragraphs
- Use the `subject` field to tag who/what the memory is about
- Recall before answering questions about preferences, setup, or history
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

    // 2. Offer to migrate MEMORY.md into ctxovrflw
    if memory_md_path.exists() {
        let content = std::fs::read_to_string(&memory_md_path)?;
        let line_count = content.lines().count();

        if line_count > 5 {
            println!();
            println!(
                "  {} Found MEMORY.md ({} lines)",
                style("📄").bold(),
                line_count,
            );
            println!(
                "  {}",
                style("Migrating imports your existing memories into ctxovrflw's").dim()
            );
            println!(
                "  {}",
                style("structured database with semantic search.").dim()
            );
            println!();

            let migrate = Confirm::new()
                .with_prompt("  Migrate MEMORY.md into ctxovrflw?")
                .default(true)
                .interact()?;

            if migrate {
                let count = migrate_memory_md(&memory_md_path, cfg).await?;
                println!(
                    "  {} Migrated {} memories from MEMORY.md",
                    style("✓").green().bold(),
                    count
                );

                // Backup original
                let backup = workspace.join("MEMORY.md.pre-ctxovrflw");
                std::fs::copy(&memory_md_path, &backup)?;
                println!(
                    "  {} Original backed up to {}",
                    style("✓").green(),
                    style("MEMORY.md.pre-ctxovrflw").dim()
                );

                // Rewrite MEMORY.md to point to ctxovrflw
                let stub = format!(
                    "# MEMORY.md\n\n\
                    > **This file is no longer the primary memory store.**\n\
                    > Memories are now managed by ctxovrflw (semantic search, cross-device sync).\n\
                    > Use `ctxovrflw recall <query>` or the MCP `recall` tool.\n\
                    > Original content backed up to `MEMORY.md.pre-ctxovrflw`.\n\n\
                    To browse memories: `ctxovrflw memories`\n\
                    To search: `ctxovrflw recall \"<query>\"`\n"
                );
                std::fs::write(&memory_md_path, stub)?;
                println!(
                    "  {} MEMORY.md updated to point to ctxovrflw",
                    style("✓").green()
                );
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
) -> Result<()> {
    let content = content.trim();
    if content.is_empty() || content.len() < 10 {
        return Ok(());
    }

    // Generate embedding if we have an embedder
    let embedding = embedder.and_then(|e| e.embed(content).ok());

    let tags = vec!["migrated".to_string(), "memory-md".to_string()];

    crate::db::memories::store_with_expiry(
        conn,
        content,
        &crate::db::memories::MemoryType::Semantic,
        &tags,
        subject,
        Some("openclaw:MEMORY.md"),
        embedding.as_deref(),
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
