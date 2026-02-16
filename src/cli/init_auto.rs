//! Non-interactive init mode (`ctxovrflw init --yes`)
//!
//! Accepts all defaults without prompting:
//! - Creates data dir, config, database
//! - Downloads embedding model if missing
//! - Detects and configures ALL found AI tools
//! - Installs agent rules for all supported tools
//! - Runs OpenClaw integration if detected (AGENTS.md injection, no MEMORY.md migration)
//! - Installs agent skill
//! - Installs systemd service (if not already installed) and starts it
//! - Does NOT enable cloud sync (requires interactive login)
//!
//! Designed for agents and scripts that cannot interact with prompts.
//! All output is structured for easy parsing.

use anyhow::Result;
use std::path::PathBuf;

use crate::config::Config;
use super::init;

pub async fn run(cfg: &Config) -> Result<()> {
    println!("ctxovrflw init (non-interactive)");
    println!();

    // 1. Data directory
    let data_dir = Config::data_dir()?;
    println!("✓ Data directory: {}", data_dir.display());

    // 2. Config
    if !Config::config_path()?.exists() {
        cfg.save()?;
        println!("✓ Config created");
    } else {
        println!("✓ Config loaded");
    }

    // 3. Database
    let _conn = crate::db::open()?;
    println!("✓ Database initialized");

    // 4. Embedding model
    let model_path = crate::embed::Embedder::model_path()?;
    let needs_download = !model_path.exists()
        || std::fs::metadata(&model_path).map(|m| m.len() < 1_000_000).unwrap_or(true);

    if needs_download {
        println!("⬇ Downloading embedding model...");
        init::download_model().await?;
        println!("✓ Model downloaded");
    } else {
        let size = std::fs::metadata(&model_path)?.len() as f64 / 1_048_576.0;
        println!("✓ Model loaded ({size:.1} MB)");
    }

    // 5. Detect and configure ALL AI tools
    println!();
    let agents = init::detect_agents();

    if agents.is_empty() {
        println!("ℹ No AI tools detected");
    } else {
        println!("Found {} tool(s), configuring all...", agents.len());
        println!();

        let url = init::mcp_sse_url(cfg);

        for agent in &agents {
            let name = agent.def.name;

            // CLI install
            if let Some(cmd_template) = agent.def.cli_install {
                let cmd = cmd_template
                    .replace("{port}", &cfg.port.to_string())
                    .replace("http://127.0.0.1:{port}/mcp/sse", &url);
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ok = std::process::Command::new(parts[0])
                        .args(&parts[1..])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if ok {
                        println!("✓ {name} (CLI)");
                    } else {
                        println!("✗ {name} — manual: {cmd}");
                    }
                }
                continue;
            }

            // JSON config
            if !agent.def.config_paths.is_empty() {
                let config_path = agent.config_path.clone().unwrap_or_else(|| {
                    init::resolve_config_path(&agent.def.config_paths[0])
                });
                let mcp_entry = init::sse_mcp_json(cfg);
                match write_mcp_config_force(&config_path, &mcp_entry) {
                    Ok(_) => println!("✓ {name} → {}", config_path.display()),
                    Err(e) => println!("✗ {name}: {e}"),
                }
                continue;
            }

            // No config path — manual
            println!("ℹ {name} — add MCP URL: {url}");
        }

        println!();
        println!("ℹ MCP endpoint: {url}");

        // 6. Install agent rules for all tools (auto-accept)
        let home = dirs::home_dir().unwrap_or_default();
        let rules = init::ctxovrflw_rules_content();
        let mut rules_installed = false;

        for agent in &agents {
            if agent.def.name == "OpenClaw" { continue; }
            if let Some(rel) = agent.def.global_rules_path {
                let path = home.join(rel);
                match install_rules_force(&path, rules) {
                    Ok(action) => {
                        println!("✓ Rules: {} ({action})", agent.def.name);
                        rules_installed = true;
                    }
                    Err(e) => println!("✗ Rules: {} — {e}", agent.def.name),
                }
            }
        }

        // 7. OpenClaw integration (if detected)
        let openclaw = agents.iter().any(|a| a.def.name == "OpenClaw");
        if openclaw {
            println!();
            println!("🐾 OpenClaw integration...");

            let agents_md = home.join(".openclaw/workspace/AGENTS.md");
            match init::inject_openclaw_agents_md(&agents_md) {
                Ok(_) => println!("✓ AGENTS.md — ctxovrflw memory section injected"),
                Err(e) => println!("✗ AGENTS.md: {e}"),
            }

            // Note: MEMORY.md migration is skipped in non-interactive mode
            // (destructive operation, should be explicit)
            let memory_md = home.join(".openclaw/workspace/MEMORY.md");
            if memory_md.exists() {
                println!("ℹ MEMORY.md found — run `ctxovrflw init` interactively to migrate");
            }
        }
    }

    // 8. Agent skill
    match init::install_agent_skill() {
        Ok(_) => println!("✓ Agent Skill installed"),
        Err(e) => println!("✗ Agent Skill: {e}"),
    }

    // 9. Service installation
    println!();
    if cfg.is_remote_client() {
        println!("✓ Using remote daemon: {}", cfg.daemon_url());
    } else if crate::daemon::is_service_installed() {
        println!("✓ Service installed");
        if !crate::daemon::is_service_running() {
            match crate::daemon::service_start() {
                Ok(_) => println!("✓ Daemon started on port {}", cfg.port),
                Err(e) => println!("✗ Daemon start: {e}"),
            }
        } else {
            println!("✓ Daemon running on port {}", cfg.port);
        }
    } else {
        match crate::daemon::service_install() {
            Ok(_) => {
                println!("✓ Service installed");
                match crate::daemon::service_start() {
                    Ok(_) => println!("✓ Daemon started on port {}", cfg.port),
                    Err(e) => println!("✗ Daemon start: {e}"),
                }
            }
            Err(e) => println!("✗ Service install: {e}"),
        }
    }

    // 10. Cloud sync (skip in non-interactive — requires login flow)
    println!();
    if cfg.is_logged_in() {
        println!("✓ Cloud sync: {}", cfg.email.as_deref().unwrap_or("configured"));
    } else {
        println!("ℹ Cloud sync: run `ctxovrflw login` to enable");
    }

    // 11. Summary
    println!();
    println!("✅ ctxovrflw is ready!");
    println!();
    println!("  MCP endpoint: {}", init::mcp_sse_url(cfg));
    println!("  REST API:     {}/v1/", cfg.daemon_url());
    println!("  Data dir:     {}", Config::data_dir()?.display());

    Ok(())
}

/// Write MCP config, always overwriting existing ctxovrflw entries
fn write_mcp_config_force(path: &PathBuf, mcp_entry: &serde_json::Value) -> Result<()> {
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
    config["mcpServers"]["ctxovrflw"] = mcp_entry.clone();
    let formatted = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, formatted)?;
    Ok(())
}

/// Install rules, always writing (overwrite or append)
fn install_rules_force(path: &PathBuf, rules: &str) -> Result<String> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)?;
        if existing.contains(init::CTXOVRFLW_RULES_MARKER) {
            let updated = init::replace_ctxovrflw_section(&existing, rules);
            std::fs::write(path, updated)?;
            Ok("updated".into())
        } else {
            let mut content = existing;
            if !content.ends_with('\n') { content.push('\n'); }
            content.push_str(rules);
            std::fs::write(path, content)?;
            Ok("appended".into())
        }
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, rules.trim_start())?;
        Ok("created".into())
    }
}
