use anyhow::Result;
use console::style;
use dialoguer::{Confirm, MultiSelect};
use std::path::PathBuf;

use crate::config::Config;

// ── Agent Registry ───────────────────────────────────────────

#[derive(Debug, Clone)]
struct AgentDef {
    name: &'static str,
    detect: DetectMethod,
    /// Config paths for JSON-based MCP config (global/user level)
    config_paths: &'static [ConfigLocation],
    /// Uses CLI command for install instead of JSON config
    cli_install: Option<&'static str>,
}

#[derive(Debug, Clone)]
enum DetectMethod {
    Binary(&'static str),
    Dir(&'static str),
    ConfigDir(&'static str),
    Any(&'static [&'static str]),
}

#[derive(Debug, Clone)]
enum ConfigLocation {
    Home(&'static str),
    Config(&'static str),
    MacApp(&'static str),
    AppData(&'static str),
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Claude Code",
        detect: DetectMethod::Binary("claude"),
        config_paths: &[],
        cli_install: Some("claude mcp add --transport sse --scope user ctxovrflw http://127.0.0.1:{port}/mcp/sse"),
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
    },
    AgentDef {
        name: "Cursor",
        detect: DetectMethod::Dir(".cursor"),
        config_paths: &[ConfigLocation::Home(".cursor/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Cline",
        detect: DetectMethod::Dir(".cline"),
        config_paths: &[
            ConfigLocation::Home(".cline/mcp_settings.json"),
            ConfigLocation::Config("Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
        ],
        cli_install: None,
    },
    AgentDef {
        name: "Roo Code",
        detect: DetectMethod::Dir(".roo-code"),
        config_paths: &[
            ConfigLocation::Config("Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json"),
            ConfigLocation::Home(".roo-code/mcp.json"),
        ],
        cli_install: None,
    },
    AgentDef {
        name: "Windsurf",
        detect: DetectMethod::Dir(".windsurf"),
        config_paths: &[ConfigLocation::Home(".windsurf/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Continue",
        detect: DetectMethod::ConfigDir("continue"),
        config_paths: &[ConfigLocation::Config("continue/config.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Codex CLI",
        detect: DetectMethod::Binary("codex"),
        config_paths: &[ConfigLocation::Config("codex/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Goose",
        detect: DetectMethod::Any(&["goose", "goosed"]),
        config_paths: &[ConfigLocation::Config("goose/config.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Gemini CLI",
        detect: DetectMethod::Binary("gemini"),
        config_paths: &[ConfigLocation::Config("gemini/settings.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Antigravity",
        detect: DetectMethod::Dir(".antigravity"),
        config_paths: &[ConfigLocation::Home(".antigravity/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Amp",
        detect: DetectMethod::Binary("amp"),
        config_paths: &[ConfigLocation::Config("amp/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Kiro",
        detect: DetectMethod::Any(&["kiro", "kiro-cli"]),
        config_paths: &[ConfigLocation::Home(".kiro/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "OpenCode",
        detect: DetectMethod::Binary("opencode"),
        config_paths: &[ConfigLocation::Config("opencode/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Trae",
        detect: DetectMethod::Dir(".trae"),
        config_paths: &[ConfigLocation::Home(".trae/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "Kilo Code",
        detect: DetectMethod::Dir(".kilo"),
        config_paths: &[
            ConfigLocation::Config("Code/User/globalStorage/kilocode.kilo-code/settings/mcp_settings.json"),
            ConfigLocation::Home(".kilo/mcp.json"),
        ],
        cli_install: None,
    },
    AgentDef {
        name: "Factory (Drip)",
        detect: DetectMethod::Binary("drip"),
        config_paths: &[ConfigLocation::Config("factory/mcp.json")],
        cli_install: None,
    },
    AgentDef {
        name: "GitHub Copilot",
        detect: DetectMethod::Binary("gh-copilot"),
        config_paths: &[],
        cli_install: None,
    },
    AgentDef {
        name: "OpenClaw",
        detect: DetectMethod::Dir(".openclaw"),
        config_paths: &[],
        cli_install: None,
    },
];

// ── Detection ────────────────────────────────────────────────

struct DetectedAgent {
    def: &'static AgentDef,
    config_path: Option<PathBuf>,
}

fn detect_agents() -> Vec<DetectedAgent> {
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

fn resolve_config_path(loc: &ConfigLocation) -> PathBuf {
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

fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Installation ─────────────────────────────────────────────

fn sse_mcp_json(port: u16) -> serde_json::Value {
    serde_json::json!({
        "url": format!("http://127.0.0.1:{port}/mcp/sse")
    })
}

fn install_agent(agent: &DetectedAgent, port: u16) -> Result<()> {
    // CLI-based install (e.g., Claude Code)
    if let Some(cmd_template) = agent.def.cli_install {
        let cmd = cmd_template.replace("{port}", &port.to_string());
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
        println!("    http://127.0.0.1:{port}/mcp/sse");
        return Ok(());
    }

    // JSON config file
    let mcp_entry = sse_mcp_json(port);
    let config_path = agent.config_path.clone().unwrap_or_else(|| {
        resolve_config_path(&agent.def.config_paths[0])
    });

    write_mcp_config(&config_path, &mcp_entry, agent.def.name)
}

fn write_mcp_config(
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

// ── Agent Skill Installation ─────────────────────────────────

/// The bundled SKILL.md content (included at compile time from skill/SKILL.md)
const SKILL_MD: &str = include_str!("../../skill/SKILL.md");

/// Install the ctxovrflw Agent Skill to ~/.skills/ctxovrflw/
fn install_agent_skill() -> Result<bool> {
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

    if agents.is_empty() {
        println!("  {} No AI tools detected.", style("ℹ").blue());
        println!();
        println!("  Supported: Claude Code, Cursor, Cline, Windsurf, Codex,");
        println!("  Goose, Amp, Kiro, OpenCode, Roo Code, Trae, Gemini CLI,");
        println!("  Antigravity, Kilo, Factory, Continue, Claude Desktop, OpenClaw");
        println!();
        println!("  Install a tool and re-run: {}", style("ctxovrflw init").bold());
    } else {
        // Multi-select with all checked by default
        let agent_names: Vec<String> = agents
            .iter()
            .map(|a| a.def.name.to_string())
            .collect();
        let defaults: Vec<bool> = agents.iter().map(|_| true).collect();

        let selections = MultiSelect::new()
            .with_prompt(format!(
                "  Found {} tool(s) — select which to configure {}",
                agents.len(),
                style("(space to toggle, enter to confirm)").dim()
            ))
            .items(&agent_names)
            .defaults(&defaults)
            .interact()?;

        if selections.is_empty() {
            println!("  {} No tools selected", style("→").dim());
        } else {
            println!();
            let port = cfg.port;
            for &idx in &selections {
                if let Err(e) = install_agent(&agents[idx], port) {
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
                style(format!("http://127.0.0.1:{port}/mcp/sse")).underlined()
            );
        }
    }

    // 6. Agent Skill (agentskills.io spec)
    println!();
    println!("  {}", style("Installing Agent Skill...").bold());
    match install_agent_skill() {
        Ok(_) => {}
        Err(e) => println!("  {} Skill install failed: {e}", style("⚠").yellow()),
    }

    // 7. Service installation
    println!();
    if !crate::daemon::is_service_installed() {
        let install_service = Confirm::new()
            .with_prompt("  Install as background service? (recommended)")
            .default(true)
            .interact()?;

        if install_service {
            if let Err(e) = crate::daemon::service_install() {
                println!("  {} Service install failed: {e}", style("⚠").yellow());
            } else {
                // Start now?
                let start_now = Confirm::new()
                    .with_prompt("  Start the daemon now?")
                    .default(true)
                    .interact()?;

                if start_now {
                    if let Err(e) = crate::daemon::service_start() {
                        println!("  {} {e}", style("⚠").yellow());
                    } else {
                        println!("  {} Daemon running on port {}", style("✓").green().bold(), cfg.port);
                    }
                }
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

async fn download_model() -> Result<()> {
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
