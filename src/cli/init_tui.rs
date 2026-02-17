use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::Config;
use super::init;

// ── Constants ───────────────────────────────────────────────

const STEP_NAMES: &[&str] = &["Setup", "Model", "Tools", "Daemon", "Cloud", "Done"];
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// ── Types ───────────────────────────────────────────────────

#[derive(Clone)]
struct LogLine {
    spans: Vec<(String, Style)>,
}

impl LogLine {
    fn ok(text: impl Into<String>) -> Self {
        Self { spans: vec![
            ("  ✓ ".into(), Style::default().fg(Color::Green).bold()),
            (text.into(), Style::default()),
        ]}
    }
    fn err(text: impl Into<String>) -> Self {
        Self { spans: vec![
            ("  ✗ ".into(), Style::default().fg(Color::Red).bold()),
            (text.into(), Style::default().fg(Color::Red)),
        ]}
    }
    fn info(text: impl Into<String>) -> Self {
        Self { spans: vec![
            ("  ℹ ".into(), Style::default().fg(Color::Blue)),
            (text.into(), Style::default().fg(Color::DarkGray)),
        ]}
    }
    fn plain(text: impl Into<String>) -> Self {
        Self { spans: vec![(format!("  {}", text.into()), Style::default())] }
    }
    fn blank() -> Self {
        Self { spans: vec![(" ".into(), Style::default())] }
    }
    fn header(text: impl Into<String>) -> Self {
        Self { spans: vec![
            (format!("  {}", text.into()), Style::default().fg(Color::Cyan).bold()),
        ]}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum StepStatus {
    Pending,
    Active,
    Complete,
    Skipped,
}

enum Interaction {
    None,
    Checkbox {
        items: Vec<(String, bool)>,
        cursor: usize,
    },
    Menu {
        title: String,
        items: Vec<String>,
        cursor: usize,
    },
    YesNo {
        prompt: String,
        selected: bool, // true = Yes, false = No
    },
    TextInput {
        prompt: String,
        value: String,
    },
    Spinner {
        message: String,
    },
    PressAnyKey,
}

#[derive(Clone, Copy, PartialEq)]
enum FlowState {
    // Setup
    RunSetup,
    // Model
    CheckModel,
    SelectModel,
    DownloadingSelectedModel,
    // Tools
    DetectingTools,
    SelectingTools,
    InstallingTools,
    ConfirmOverwrite,
    AskRules,
    InstallingRules,
    OpenClawMenu,
    InstallingOpenClawPlugin,
    OpenClawIntegration,
    AskOpenClawMigrate,
    RunOpenClawMigrate,
    // Daemon
    DaemonMenu,
    InstallingService,
    AskStartService,
    StartingService,
    EnterRemoteUrl,
    TestingRemoteUrl,
    // Cloud
    AskCloud,
    // Done
    ShowSummary,
    Finished,
    // Error recovery
    RetryOpenClawPlugin,
}

enum AsyncMsg {
    ModelDownloaded(Result<()>),
    ConnectResult(Result<bool>),
    OpenClawPluginInstalled(Result<()>),
}

// ── App ─────────────────────────────────────────────────────

struct App {
    steps: [StepStatus; 6],
    current_step: usize,
    lines: Vec<LogLine>,
    interaction: Interaction,
    flow: FlowState,
    tick: usize,
    should_quit: bool,

    // Config (mutable for saves)
    cfg: Config,

    // Async channel
    async_tx: mpsc::UnboundedSender<AsyncMsg>,
    async_rx: mpsc::UnboundedReceiver<AsyncMsg>,

    // State carried between steps
    detected_agents: Vec<init::DetectedAgent>,
    selected_tools: Vec<usize>,
    openclaw_selected: bool,
    has_rules_agents: bool,
    remote_url: String,
    tools_installed: Vec<String>,
    pending_overwrites: Vec<(usize, PathBuf)>,
    current_overwrite_idx: usize,
    service_installed: bool,
    service_running: bool,
    openclaw_menu_choice: usize,
}

impl App {
    fn new(cfg: Config) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut steps = [StepStatus::Pending; 6];
        steps[0] = StepStatus::Active;
        Self {
            steps,
            current_step: 0,
            lines: Vec::new(),
            interaction: Interaction::None,
            flow: FlowState::RunSetup,
            tick: 0,
            should_quit: false,
            cfg,
            async_tx: tx,
            async_rx: rx,
            detected_agents: Vec::new(),
            selected_tools: Vec::new(),
            openclaw_selected: false,
            has_rules_agents: false,
            remote_url: String::new(),
            tools_installed: Vec::new(),
            pending_overwrites: Vec::new(),
            current_overwrite_idx: 0,
            service_installed: false,
            service_running: false,
            openclaw_menu_choice: 0,
        }
    }

    /// Clear all log lines (fresh screen for new section)
    fn clear_lines(&mut self) {
        self.lines.clear();
    }

    fn complete_step(&mut self) {
        self.steps[self.current_step] = StepStatus::Complete;
        self.current_step += 1;
        if self.current_step < 6 {
            self.steps[self.current_step] = StepStatus::Active;
        }
    }

    fn skip_step(&mut self) {
        self.steps[self.current_step] = StepStatus::Skipped;
        self.current_step += 1;
        if self.current_step < 6 {
            self.steps[self.current_step] = StepStatus::Active;
        }
    }

    // ── State Machine ───────────────────────────────────────

    fn advance(&mut self) {
        match self.flow {
            FlowState::RunSetup => self.run_setup(),
            FlowState::CheckModel => self.check_model(),
            FlowState::DetectingTools => self.detect_tools(),
            FlowState::InstallingTools => self.install_tools(),
            FlowState::AskRules => self.ask_rules(),
            FlowState::InstallingRules => self.install_rules(),
            FlowState::OpenClawIntegration => self.run_openclaw_integration(),
            FlowState::DaemonMenu => self.show_daemon_menu(),
            FlowState::AskCloud => self.ask_cloud(),
            FlowState::ShowSummary => self.show_summary(),
            _ => {} // Interactive/async states handled by key/poll
        }
    }

    fn run_setup(&mut self) {
        // Data directory
        match Config::data_dir() {
            Ok(dir) => self.lines.push(LogLine::ok(format!("Data directory: {}", dir.display()))),
            Err(e) => self.lines.push(LogLine::err(format!("Data directory: {e}"))),
        }

        // Config
        if Config::config_path().map(|p| p.exists()).unwrap_or(false) {
            self.lines.push(LogLine::ok("Config loaded"));
        } else {
            match self.cfg.save() {
                Ok(_) => self.lines.push(LogLine::ok("Config created")),
                Err(e) => self.lines.push(LogLine::err(format!("Config: {e}"))),
            }
        }

        // Database
        match crate::db::open() {
            Ok(_) => self.lines.push(LogLine::ok("Database initialized")),
            Err(e) => self.lines.push(LogLine::err(format!("Database: {e}"))),
        }

        self.complete_step();
        self.flow = FlowState::CheckModel;
        self.advance();
    }

    fn check_model(&mut self) {
        use crate::embed::models::MODELS;

        self.clear_lines();

        let model_ok = crate::embed::Embedder::model_path()
            .ok()
            .map(|p| p.exists() && std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false))
            .unwrap_or(false);

        if model_ok {
            let current = crate::embed::models::get_model(&self.cfg.embedding_model);
            let name = current.map(|m| m.name).unwrap_or("unknown");
            self.lines.push(LogLine::ok(format!("Current model: {name}")));
        } else {
            self.lines.push(LogLine::info("No embedding model installed"));
        }
        self.lines.push(LogLine::blank());

        let items: Vec<String> = MODELS.iter().map(|m| {
            let current_marker = if m.id == self.cfg.embedding_model && model_ok { " ✓" } else { "" };
            format!("{} — {}d, ~{}MB{}", m.name, m.dim, m.size_mb, current_marker)
        }).collect();

        self.interaction = Interaction::Menu {
            title: "Choose an embedding model:".into(),
            items,
            cursor: MODELS.iter().position(|m| m.id == self.cfg.embedding_model).unwrap_or(0),
        };
        self.flow = FlowState::SelectModel;
    }

    fn detect_tools(&mut self) {
        self.clear_lines();
        self.lines.push(LogLine::header("Scanning for AI tools..."));
        self.lines.push(LogLine::blank());

        self.detected_agents = init::detect_agents();

        if self.detected_agents.is_empty() {
            self.lines.push(LogLine::info("No AI tools detected."));
            self.lines.push(LogLine::plain("Supported: Claude Code, Cursor, Cline, Windsurf, OpenClaw, ..."));
            self.lines.push(LogLine::plain("Install a tool and re-run: ctxovrflw init"));
            self.complete_step();
            self.flow = FlowState::DaemonMenu;
            self.advance();
        } else {
            for agent in &self.detected_agents {
                let path_info = agent.config_path.as_ref()
                    .map(|p| format!(" ({})", p.display()))
                    .or_else(|| agent.def.cli_install.map(|_| " (CLI)".to_string()))
                    .unwrap_or_default();
                self.lines.push(LogLine::ok(format!("Found {}{}", agent.def.name, path_info)));
            }
            self.lines.push(LogLine::blank());

            let items: Vec<(String, bool)> = self.detected_agents
                .iter()
                .map(|a| (a.def.name.to_string(), true))
                .collect();
            self.interaction = Interaction::Checkbox { items, cursor: 0 };
            self.flow = FlowState::SelectingTools;
        }
    }

    fn install_tools(&mut self) {
        self.interaction = Interaction::None;
        self.clear_lines();
        self.lines.push(LogLine::header("Configuring tools..."));
        self.lines.push(LogLine::blank());

        let url = init::mcp_sse_url(&self.cfg);

        // Collect tools that need overwrite confirmation
        self.pending_overwrites.clear();
        self.current_overwrite_idx = 0;

        for &idx in &self.selected_tools.clone() {
            let agent = &self.detected_agents[idx];
            let name = agent.def.name;

            // Skip OpenClaw — it uses plugin/skills, not MCP
            if name == "OpenClaw" {
                continue;
            }

            self.lines.push(LogLine::header(format!("─ {name}")));

            // Try CLI install
            if let Some(cmd_template) = agent.def.cli_install {
                let cmd = cmd_template
                    .replace("{port}", &self.cfg.port.to_string())
                    .replace("http://127.0.0.1:{port}/mcp/sse", &url);
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() >= 2 {
                    let output = std::process::Command::new(parts[0])
                        .args(&parts[1..])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output();
                    match output {
                        Ok(out) if out.status.success() => {
                            self.lines.push(LogLine::ok("MCP server registered via CLI"));
                            self.tools_installed.push(name.to_string());
                        }
                        Ok(_) => {
                            self.lines.push(LogLine::err("CLI registration failed"));
                            self.lines.push(LogLine::info(format!("Run manually: {cmd}")));
                        }
                        Err(_) => {
                            self.lines.push(LogLine::err("Command not found"));
                            self.lines.push(LogLine::info(format!("Run manually: {cmd}")));
                        }
                    }
                }
                self.lines.push(LogLine::blank());
                continue;
            }

            // JSON config — check if overwrite needed
            if !agent.def.config_paths.is_empty() {
                let config_path = agent.config_path.clone().unwrap_or_else(|| {
                    init::resolve_config_path(&agent.def.config_paths[0])
                });

                let needs_overwrite = config_path.exists() && {
                    std::fs::read_to_string(&config_path)
                        .ok()
                        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                        .and_then(|v| v.get("mcpServers")?.get("ctxovrflw").cloned())
                        .is_some()
                };

                if needs_overwrite {
                    self.pending_overwrites.push((idx, config_path));
                } else {
                    let mcp_entry = init::sse_mcp_json(&self.cfg);
                    match write_mcp_config_quiet(&config_path, &mcp_entry) {
                        Ok(_) => {
                            self.lines.push(LogLine::ok(format!(
                                "Config written: {}", config_path.display()
                            )));
                            self.tools_installed.push(name.to_string());
                        }
                        Err(e) => self.lines.push(LogLine::err(format!("Failed: {e}"))),
                    }
                }
                self.lines.push(LogLine::blank());
                continue;
            }

            // Manual
            self.lines.push(LogLine::info(format!("Add MCP URL manually: {url}")));
            self.tools_installed.push(name.to_string());
            self.lines.push(LogLine::blank());
        }

        // If there are overwrite prompts, show them one at a time
        if !self.pending_overwrites.is_empty() {
            self.current_overwrite_idx = 0;
            self.show_next_overwrite();
            return;
        }

        self.finish_tool_install();
    }

    fn show_next_overwrite(&mut self) {
        if self.current_overwrite_idx >= self.pending_overwrites.len() {
            self.finish_tool_install();
            return;
        }

        let (idx, ref path) = self.pending_overwrites[self.current_overwrite_idx];
        let name = self.detected_agents[idx].def.name;
        let path_display = path.display().to_string();

        // Clear and show fresh screen for this overwrite prompt
        self.clear_lines();
        self.lines.push(LogLine::header("Existing Configuration Found"));
        self.lines.push(LogLine::blank());
        self.lines.push(LogLine::plain(format!("Agent: {name}")));
        self.lines.push(LogLine::plain(format!("Path:  {}", path_display)));
        self.lines.push(LogLine::blank());
        self.lines.push(LogLine::info("ctxovrflw is already configured for this agent."));
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::YesNo {
            prompt: format!("Overwrite {name} config?"),
            selected: true,
        };
        self.flow = FlowState::ConfirmOverwrite;
    }

    fn handle_overwrite_response(&mut self, overwrite: bool) {
        let (idx, config_path) = self.pending_overwrites[self.current_overwrite_idx].clone();
        let name = self.detected_agents[idx].def.name;

        if overwrite {
            let mcp_entry = init::sse_mcp_json(&self.cfg);
            match write_mcp_config_quiet(&config_path, &mcp_entry) {
                Ok(_) => {
                    self.tools_installed.push(name.to_string());
                }
                Err(_) => {}
            }
        }

        self.current_overwrite_idx += 1;
        self.show_next_overwrite();
    }

    fn finish_tool_install(&mut self) {
        self.clear_lines();

        if !self.tools_installed.is_empty() {
            self.lines.push(LogLine::header("Tools configured:"));
            for name in &self.tools_installed {
                self.lines.push(LogLine::ok(name.clone()));
            }
            self.lines.push(LogLine::blank());
        }

        let url = init::mcp_sse_url(&self.cfg);
        self.lines.push(LogLine::info(format!("Tools connect via {url}")));
        self.lines.push(LogLine::blank());

        // Check if we have rules-capable agents (non-OpenClaw)
        self.has_rules_agents = self.selected_tools.iter().any(|&idx| {
            let a = &self.detected_agents[idx];
            a.def.global_rules_path.is_some() && a.def.name != "OpenClaw"
        });

        self.openclaw_selected = self.selected_tools.iter().any(|&idx| {
            self.detected_agents[idx].def.name == "OpenClaw"
        });

        if self.has_rules_agents {
            self.flow = FlowState::AskRules;
            self.advance();
        } else if self.openclaw_selected {
            self.flow = FlowState::OpenClawIntegration;
            self.advance();
        } else {
            self.complete_step();
            self.flow = FlowState::DaemonMenu;
            self.advance();
        }
    }

    fn ask_rules(&mut self) {
        let agents_with_rules: Vec<String> = self.selected_tools.iter()
            .filter_map(|&idx| {
                let a = &self.detected_agents[idx];
                if a.def.global_rules_path.is_some() && a.def.name != "OpenClaw" {
                    Some(format!("{} → ~/{}", a.def.name, a.def.global_rules_path.unwrap()))
                } else {
                    None
                }
            })
            .collect();

        for a in &agents_with_rules {
            self.lines.push(LogLine::plain(a));
        }
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::YesNo {
            prompt: "Install agent rules? (teaches agents to use ctxovrflw)".into(),
            selected: true,
        };
        self.flow = FlowState::AskRules;
    }

    fn install_rules(&mut self) {
        self.interaction = Interaction::None;
        let home = dirs::home_dir().unwrap_or_default();
        let rules = init::ctxovrflw_rules_content();

        for &idx in &self.selected_tools.clone() {
            let a = &self.detected_agents[idx];
            if a.def.name == "OpenClaw" { continue; }
            if let Some(rel) = a.def.global_rules_path {
                let path = home.join(rel);
                match install_rules_quiet(&path, rules) {
                    Ok(action) => self.lines.push(LogLine::ok(format!("{} ({})", a.def.name, action))),
                    Err(e) => self.lines.push(LogLine::err(format!("{}: {e}", a.def.name))),
                }
            }
        }

        if self.openclaw_selected {
            self.flow = FlowState::OpenClawIntegration;
            self.advance();
        } else {
            self.complete_step();
            self.flow = FlowState::DaemonMenu;
            self.advance();
        }
    }

    fn run_openclaw_integration(&mut self) {
        self.clear_lines();
        self.lines.push(LogLine::header("🐾 OpenClaw Integration"));
        self.lines.push(LogLine::blank());
        self.lines.push(LogLine::info("OpenClaw uses a plugin (not MCP) for ctxovrflw integration."));
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::Menu {
            title: "How to integrate ctxovrflw with OpenClaw?".into(),
            items: vec![
                "Plugin + Skill + Agent Rules (recommended)".into(),
                "Plugin only".into(),
                "Skill + Agent Rules only".into(),
                "Skip OpenClaw integration".into(),
            ],
            cursor: 0,
        };
        self.flow = FlowState::OpenClawMenu;
    }

    fn run_openclaw_plugin_install(&mut self) {
        self.clear_lines();
        self.lines.push(LogLine::header("🐾 OpenClaw Plugin"));
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::Spinner {
            message: "Installing @ctxovrflw/memory-ctxovrflw...".into(),
        };
        self.flow = FlowState::InstallingOpenClawPlugin;

        let tx = self.async_tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                // Install plugin
                let output = std::process::Command::new("openclaw")
                    .args(["plugins", "install", "@ctxovrflw/memory-ctxovrflw"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("Plugin install failed: {}", stderr.trim());
                }

                // Restart gateway
                let output = std::process::Command::new("openclaw")
                    .args(["gateway", "restart"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("Gateway restart failed: {}", stderr.trim());
                }

                Ok(())
            })();
            let _ = tx.send(AsyncMsg::OpenClawPluginInstalled(result));
        });
    }

    fn run_openclaw_skill_and_rules(&mut self) {
        // Install agent skill
        match init::install_agent_skill() {
            Ok(_) => self.lines.push(LogLine::ok("Agent Skill installed")),
            Err(e) => self.lines.push(LogLine::err(format!("Skill install: {e}"))),
        }

        // Inject AGENTS.md
        let home = dirs::home_dir().unwrap_or_default();
        let agents_md = home.join(".openclaw/workspace/AGENTS.md");
        match init::inject_openclaw_agents_md(&agents_md) {
            Ok(_) => self.lines.push(LogLine::ok("AGENTS.md — ctxovrflw memory section injected")),
            Err(e) => self.lines.push(LogLine::err(format!("AGENTS.md: {e}"))),
        }
    }

    fn finish_openclaw_integration(&mut self) {
        self.clear_lines();
        self.lines.push(LogLine::header("🐾 OpenClaw Integration"));
        self.lines.push(LogLine::blank());

        // Check for workspace files to migrate
        let home = dirs::home_dir().unwrap_or_default();
        let workspace = home.join(".openclaw/workspace");

        let files_to_check = ["IDENTITY.md", "SOUL.md", "USER.md", "AGENTS.md", "MEMORY.md"];
        let mut found: Vec<String> = Vec::new();
        for name in &files_to_check {
            let path = workspace.join(name);
            if path.exists() {
                let lines = std::fs::read_to_string(&path)
                    .map(|c| c.lines().count())
                    .unwrap_or(0);
                if lines > 3 {
                    found.push(format!("{name} ({lines} lines)"));
                }
            }
        }

        if !found.is_empty() {
            self.lines.push(LogLine::header("Workspace files found:"));
            for f in &found {
                self.lines.push(LogLine::info(f.clone()));
            }
            self.lines.push(LogLine::blank());
            self.interaction = Interaction::YesNo {
                prompt: "Migrate workspace files into ctxovrflw memories?".into(),
                selected: true,
            };
            self.flow = FlowState::AskOpenClawMigrate;
            return;
        }

        self.lines.push(LogLine::ok("OpenClaw integration complete"));
        self.complete_step();
        self.flow = FlowState::DaemonMenu;
        self.advance();
    }

    fn show_daemon_menu(&mut self) {
        self.clear_lines();

        self.service_installed = crate::daemon::is_service_installed();
        self.service_running = crate::daemon::is_service_running();

        if self.cfg.is_remote_client() {
            self.lines.push(LogLine::ok(format!(
                "Using remote daemon at {}", self.cfg.daemon_url()
            )));
            self.complete_step();
            self.flow = FlowState::AskCloud;
            self.advance();
            return;
        }

        if self.service_installed {
            self.lines.push(LogLine::ok("Service installed"));
            if self.service_running {
                self.lines.push(LogLine::ok("Daemon running"));
                self.complete_step();
                self.flow = FlowState::AskCloud;
                self.advance();
            } else {
                self.interaction = Interaction::YesNo {
                    prompt: "Daemon stopped — start it?".into(),
                    selected: true,
                };
                self.flow = FlowState::AskStartService;
            }
            return;
        }

        self.lines.push(LogLine::header("Daemon Setup"));
        self.lines.push(LogLine::info("ctxovrflw needs a running daemon for MCP and HTTP access."));
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::Menu {
            title: "How would you like to run the daemon?".into(),
            items: vec![
                "Install as background service (recommended)".into(),
                "Connect to an existing remote daemon".into(),
                "Skip for now".into(),
            ],
            cursor: 0,
        };
        self.flow = FlowState::DaemonMenu;
    }

    fn ask_cloud(&mut self) {
        self.clear_lines();

        if self.cfg.is_logged_in() {
            self.lines.push(LogLine::ok(format!(
                "Cloud sync configured ({})",
                self.cfg.email.as_deref().unwrap_or("?")
            )));
            self.complete_step();
            self.flow = FlowState::ShowSummary;
            self.advance();
            return;
        }

        self.lines.push(LogLine::header("☁ Cloud Sync"));
        self.lines.push(LogLine::info("Sync memories across devices with end-to-end encryption."));
        self.lines.push(LogLine::blank());

        self.interaction = Interaction::YesNo {
            prompt: "Enable cloud sync?".into(),
            selected: true,
        };
    }

    fn show_summary(&mut self) {
        self.clear_lines();
        self.interaction = Interaction::None;

        self.lines.push(LogLine { spans: vec![
            ("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".into(),
             Style::default().fg(Color::DarkGray)),
        ]});
        self.lines.push(LogLine { spans: vec![
            ("  ✅ ctxovrflw is ready!".into(),
             Style::default().fg(Color::Green).bold()),
        ]});
        self.lines.push(LogLine::blank());
        self.lines.push(LogLine::header("Quick test:"));
        self.lines.push(LogLine::plain("  ctxovrflw remember \"I prefer Rust for backend services\""));
        self.lines.push(LogLine::plain("  ctxovrflw recall \"language preferences\""));
        self.lines.push(LogLine::blank());
        self.lines.push(LogLine::header("Manage:"));
        self.lines.push(LogLine::plain("  ctxovrflw start / stop / status"));
        self.lines.push(LogLine::plain("  ctxovrflw memories    (interactive TUI)"));

        if !self.cfg.is_logged_in() {
            self.lines.push(LogLine::plain("  ctxovrflw login       (enable cloud sync)"));
        }

        self.lines.push(LogLine::blank());
        self.interaction = Interaction::PressAnyKey;
        self.flow = FlowState::Finished;
    }

    // ── Key Handling ────────────────────────────────────────

    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        // Global quit
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return Ok(());
        }

        match &mut self.interaction {
            Interaction::Checkbox { items, cursor } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *cursor > 0 { *cursor -= 1; }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if *cursor + 1 < items.len() { *cursor += 1; }
                    }
                    KeyCode::Char(' ') => {
                        items[*cursor].1 = !items[*cursor].1;
                    }
                    KeyCode::Enter => {
                        // Tool selection (SelectingTools)
                        self.selected_tools = items.iter().enumerate()
                            .filter(|(_, (_, sel))| *sel)
                            .map(|(i, _)| i)
                            .collect();

                        if self.selected_tools.is_empty() {
                            self.lines.push(LogLine::info("No tools selected"));
                            self.complete_step();
                            self.flow = FlowState::DaemonMenu;
                            self.advance();
                        } else {
                            self.flow = FlowState::InstallingTools;
                            self.advance();
                        }
                    }
                    _ => {}
                }
            }

            Interaction::Menu { items, cursor, .. } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *cursor > 0 { *cursor -= 1; }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if *cursor + 1 < items.len() { *cursor += 1; }
                    }
                    KeyCode::Enter => {
                        let choice = *cursor;
                        self.interaction = Interaction::None;

                        match self.flow {
                            FlowState::SelectModel => {
                                use crate::embed::models::MODELS;
                                let model = &MODELS[choice];

                                let already_installed = model.id == self.cfg.embedding_model && {
                                    crate::embed::Embedder::model_path()
                                        .ok()
                                        .map(|p| p.exists() && std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false))
                                        .unwrap_or(false)
                                };

                                if already_installed {
                                    self.lines.push(LogLine::ok(format!("Keeping current model: {}", model.name)));
                                    self.complete_step();
                                    self.flow = FlowState::DetectingTools;
                                    self.advance();
                                } else {
                                    self.clear_lines();
                                    self.lines.push(LogLine::info(format!(
                                        "Downloading {} (~{}MB)...", model.name, model.size_mb
                                    )));
                                    self.interaction = Interaction::Spinner {
                                        message: format!("Downloading {}...", model.name),
                                    };
                                    self.flow = FlowState::DownloadingSelectedModel;

                                    let model_id = model.id.to_string();
                                    let model_dim = model.dim;
                                    let onnx_url = model.onnx_url.to_string();
                                    let tokenizer_url = model.tokenizer_url.to_string();
                                    let tx = self.async_tx.clone();
                                    tokio::spawn(async move {
                                        let result = download_model_quiet(
                                            &model_id, model_dim, &onnx_url, &tokenizer_url
                                        ).await;
                                        let _ = tx.send(AsyncMsg::ModelDownloaded(result));
                                    });
                                }
                            }
                            FlowState::OpenClawMenu => {
                                self.openclaw_menu_choice = choice;
                                match choice {
                                    0 => {
                                        // Plugin + Skill + Agent Rules
                                        self.clear_lines();
                                        self.lines.push(LogLine::header("🐾 OpenClaw Integration"));
                                        self.lines.push(LogLine::blank());
                                        self.run_openclaw_skill_and_rules();
                                        self.lines.push(LogLine::blank());
                                        self.run_openclaw_plugin_install();
                                    }
                                    1 => {
                                        // Plugin only
                                        self.run_openclaw_plugin_install();
                                    }
                                    2 => {
                                        // Skill + Agent Rules only
                                        self.clear_lines();
                                        self.lines.push(LogLine::header("🐾 OpenClaw Integration"));
                                        self.lines.push(LogLine::blank());
                                        self.run_openclaw_skill_and_rules();
                                        self.finish_openclaw_integration();
                                    }
                                    _ => {
                                        // Skip
                                        self.lines.push(LogLine::info("Skipped OpenClaw integration"));
                                        self.complete_step();
                                        self.flow = FlowState::DaemonMenu;
                                        self.advance();
                                    }
                                }
                            }
                            FlowState::DaemonMenu => {
                                match choice {
                                    0 => {
                                        match crate::daemon::service_install() {
                                            Ok(_) => {
                                                self.lines.push(LogLine::ok("Service installed"));
                                                self.service_installed = true;
                                                self.interaction = Interaction::YesNo {
                                                    prompt: "Start the daemon now?".into(),
                                                    selected: true,
                                                };
                                                self.flow = FlowState::AskStartService;
                                            }
                                            Err(e) => {
                                                self.lines.push(LogLine::err(format!("Service install: {e}")));
                                                self.complete_step();
                                                self.flow = FlowState::AskCloud;
                                                self.advance();
                                            }
                                        }
                                    }
                                    1 => {
                                        self.lines.push(LogLine::info("Enter the URL of the remote daemon"));
                                        self.interaction = Interaction::TextInput {
                                            prompt: "Remote daemon URL".into(),
                                            value: format!("http://127.0.0.1:{}", self.cfg.port),
                                        };
                                        self.flow = FlowState::EnterRemoteUrl;
                                    }
                                    _ => {
                                        self.lines.push(LogLine::info("Skipped. Run later: ctxovrflw init"));
                                        self.skip_step();
                                        self.flow = FlowState::AskCloud;
                                        self.advance();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            Interaction::YesNo { selected, .. } => {
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') |
                    KeyCode::Tab => {
                        *selected = !*selected;
                    }
                    KeyCode::Enter => {
                        let yes = *selected;
                        self.on_yesno(yes).await?;
                    }
                    _ => {}
                }
            }

            Interaction::TextInput { value, .. } => {
                match key.code {
                    KeyCode::Char(c) => { value.push(c); }
                    KeyCode::Backspace => { value.pop(); }
                    KeyCode::Enter => {
                        let val = value.clone();
                        self.on_text_submit(val).await?;
                    }
                    KeyCode::Esc => {
                        self.interaction = Interaction::None;
                        self.skip_step();
                        self.flow = FlowState::AskCloud;
                        self.advance();
                    }
                    _ => {}
                }
            }

            Interaction::PressAnyKey => {
                self.should_quit = true;
            }

            Interaction::Spinner { .. } | Interaction::None => {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    self.should_quit = true;
                }
            }
        }

        Ok(())
    }

    async fn on_yesno(&mut self, yes: bool) -> Result<()> {
        self.interaction = Interaction::None;

        match self.flow {
            FlowState::ConfirmOverwrite => {
                self.handle_overwrite_response(yes);
            }
            FlowState::AskRules => {
                if yes {
                    self.flow = FlowState::InstallingRules;
                    self.advance();
                } else {
                    self.lines.push(LogLine::info("Skipped rules"));
                    if self.openclaw_selected {
                        self.flow = FlowState::OpenClawIntegration;
                        self.advance();
                    } else {
                        self.complete_step();
                        self.flow = FlowState::DaemonMenu;
                        self.advance();
                    }
                }
            }
            FlowState::AskOpenClawMigrate => {
                if yes {
                    self.flow = FlowState::RunOpenClawMigrate;
                    self.interaction = Interaction::Spinner {
                        message: "Migrating workspace files...".into(),
                    };
                    match init::migrate_workspace_files(&self.cfg).await {
                        Ok(count) => {
                            self.interaction = Interaction::None;
                            self.lines.push(LogLine::ok(format!("Migrated {count} memories from workspace files")));

                            let home = dirs::home_dir().unwrap_or_default();
                            let memory_md = home.join(".openclaw/workspace/MEMORY.md");
                            if memory_md.exists() {
                                let content = std::fs::read_to_string(&memory_md).unwrap_or_default();
                                if !content.contains("no longer the primary memory store") {
                                    let backup = home.join(".openclaw/workspace/MEMORY.md.pre-ctxovrflw");
                                    let _ = std::fs::copy(&memory_md, &backup);
                                    let stub = "# MEMORY.md\n\n\
                                        > **This file is no longer the primary memory store.**\n\
                                        > Memories are now managed by ctxovrflw.\n\
                                        > Use `ctxovrflw recall <query>` or the MCP `recall` tool.\n\n\
                                        To browse: `ctxovrflw memories`\n";
                                    let _ = std::fs::write(&memory_md, stub);
                                    self.lines.push(LogLine::ok("MEMORY.md backed up and updated"));
                                }
                            }
                        }
                        Err(e) => {
                            self.interaction = Interaction::None;
                            self.lines.push(LogLine::err(format!("Migration failed: {e}")));
                        }
                    }
                } else {
                    self.lines.push(LogLine::info("Skipped workspace migration"));
                }
                self.lines.push(LogLine::ok("OpenClaw integration complete"));
                self.complete_step();
                self.flow = FlowState::DaemonMenu;
                self.advance();
            }
            FlowState::AskStartService => {
                if yes {
                    match crate::daemon::service_start() {
                        Ok(_) => {
                            self.service_running = true;
                            self.lines.push(LogLine::ok(format!(
                                "Daemon running on port {}", self.cfg.port
                            )));
                        }
                        Err(e) => self.lines.push(LogLine::err(format!("Start failed: {e}"))),
                    }
                }
                self.complete_step();
                self.flow = FlowState::AskCloud;
                self.advance();
            }
            FlowState::AskCloud => {
                if yes {
                    self.lines.push(LogLine::info("Run after setup: ctxovrflw login"));
                } else {
                    self.lines.push(LogLine::info("Skipped. Enable later: ctxovrflw login"));
                }
                self.complete_step();
                self.flow = FlowState::ShowSummary;
                self.advance();
            }
            FlowState::RetryOpenClawPlugin => {
                if yes {
                    self.run_openclaw_plugin_install();
                } else {
                    self.lines.push(LogLine::info("Skipped plugin install"));
                    self.finish_openclaw_integration();
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_text_submit(&mut self, value: String) -> Result<()> {
        match self.flow {
            FlowState::EnterRemoteUrl => {
                self.remote_url = value.trim_end_matches('/').to_string();
                self.lines.push(LogLine::info("Testing connection..."));
                self.interaction = Interaction::Spinner {
                    message: "Connecting...".into(),
                };
                self.flow = FlowState::TestingRemoteUrl;

                let url = self.remote_url.clone();
                let tx = self.async_tx.clone();
                tokio::spawn(async move {
                    let test_url = format!("{url}/v1/health");
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(5))
                        .build()
                        .unwrap();
                    let result = client.get(&test_url).send().await
                        .map(|r| r.status().is_success());
                    let _ = tx.send(AsyncMsg::ConnectResult(result.map_err(|e| e.into())));
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn poll_async(&mut self) {
        while let Ok(msg) = self.async_rx.try_recv() {
            match msg {
                AsyncMsg::ModelDownloaded(result) => {
                    self.interaction = Interaction::None;
                    self.clear_lines();
                    match result {
                        Ok(_) => {
                            self.cfg = Config::load().unwrap_or_default();
                            let name = crate::embed::models::get_model(&self.cfg.embedding_model)
                                .map(|m| m.name).unwrap_or("model");
                            self.lines.push(LogLine::ok(format!("{name} ready")));
                        }
                        Err(e) => self.lines.push(LogLine::err(format!("Model setup failed: {e}"))),
                    }
                    self.complete_step();
                    self.flow = FlowState::DetectingTools;
                    self.advance();
                }
                AsyncMsg::OpenClawPluginInstalled(result) => {
                    self.interaction = Interaction::None;
                    self.clear_lines();
                    self.lines.push(LogLine::header("🐾 OpenClaw Plugin"));
                    self.lines.push(LogLine::blank());
                    match result {
                        Ok(_) => {
                            self.lines.push(LogLine::ok("Plugin installed successfully"));
                            self.lines.push(LogLine::ok("Gateway restarted"));
                            self.lines.push(LogLine::blank());
                            self.finish_openclaw_integration();
                        }
                        Err(e) => {
                            self.lines.push(LogLine::err(format!("{e}")));
                            self.lines.push(LogLine::blank());
                            self.lines.push(LogLine::info(
                                "Manual: openclaw plugins install @ctxovrflw/memory-ctxovrflw"
                            ));
                            self.lines.push(LogLine::blank());
                            self.interaction = Interaction::YesNo {
                                prompt: "Try again?".into(),
                                selected: true,
                            };
                            self.flow = FlowState::RetryOpenClawPlugin;
                        }
                    }
                }
                AsyncMsg::ConnectResult(result) => {
                    self.interaction = Interaction::None;
                    match result {
                        Ok(true) => {
                            self.lines.push(LogLine::ok(format!(
                                "Connected to remote daemon at {}", self.remote_url
                            )));
                            self.cfg.remote_daemon_url = Some(self.remote_url.clone());
                            let _ = self.cfg.save();
                            self.lines.push(LogLine::info("No local daemon will be started"));
                        }
                        Ok(false) => {
                            self.lines.push(LogLine::err("Daemon responded but may not be healthy"));
                            self.cfg.remote_daemon_url = Some(self.remote_url.clone());
                            let _ = self.cfg.save();
                        }
                        Err(e) => {
                            self.lines.push(LogLine::err(format!("Connection failed: {e}")));
                            self.lines.push(LogLine::info("URL saved anyway — fix and retry"));
                            self.cfg.remote_daemon_url = Some(self.remote_url.clone());
                            let _ = self.cfg.save();
                        }
                    }
                    self.complete_step();
                    self.flow = FlowState::AskCloud;
                    self.advance();
                }
            }
        }
    }
}

// ── Non-interactive helpers ─────────────────────────────────

fn write_mcp_config_quiet(path: &PathBuf, mcp_entry: &serde_json::Value) -> Result<()> {
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

fn install_rules_quiet(path: &PathBuf, rules: &str) -> Result<String> {
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

// ── UI Rendering ────────────────────────────────────────────

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(5),    // content
            Constraint::Length(3), // tabs
        ])
        .split(f.area());

    render_header(f, chunks[0]);
    render_content(f, app, chunks[1]);
    render_tabs(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, area: Rect) {
    let header = Line::from(vec![
        Span::styled(" 🧠 ctxovrflw init", Style::default().fg(Color::Cyan).bold()),
        Span::styled("  —  Universal AI Context Layer", Style::default().fg(Color::DarkGray)),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(header).block(block), area);
}

fn render_content(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_visible = inner.height as usize;

    // Build all content lines
    let mut all_lines: Vec<Line> = Vec::new();

    for log in &app.lines {
        let spans: Vec<Span> = log.spans.iter()
            .map(|(text, style)| Span::styled(text.clone(), *style))
            .collect();
        all_lines.push(Line::from(spans));
    }

    // Add interaction widget
    match &app.interaction {
        Interaction::Checkbox { items, cursor } => {
            all_lines.push(Line::from(""));
            for (i, (name, selected)) in items.iter().enumerate() {
                let is_cursor = i == *cursor;
                let check = if *selected { "x" } else { " " };
                let prefix = if is_cursor { "▸" } else { " " };
                let style = if is_cursor {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
                };
                all_lines.push(Line::from(Span::styled(
                    format!("  {prefix} [{check}] {name}"),
                    style,
                )));
            }
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Color::DarkGray)),
                Span::raw(" navigate  "),
                Span::styled("Space", Style::default().fg(Color::DarkGray)),
                Span::raw(" toggle  "),
                Span::styled("Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" confirm"),
            ]));
        }

        Interaction::Menu { title, items, cursor } => {
            all_lines.push(Line::from(Span::styled(
                format!("  {title}"),
                Style::default().fg(Color::Cyan).bold(),
            )));
            all_lines.push(Line::from(""));
            for (i, item) in items.iter().enumerate() {
                let is_cursor = i == *cursor;
                let prefix = if is_cursor { "▸" } else { " " };
                let style = if is_cursor {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
                };
                all_lines.push(Line::from(Span::styled(
                    format!("  {prefix} {item}"),
                    style,
                )));
            }
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Color::DarkGray)),
                Span::raw(" navigate  "),
                Span::styled("Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" select"),
            ]));
        }

        Interaction::YesNo { prompt, selected } => {
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(Span::styled(
                format!("  {prompt}"),
                Style::default().bold(),
            )));
            all_lines.push(Line::from(""));

            let yes_style = if *selected {
                Style::default().fg(Color::Black).bg(Color::Green).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let no_style = if !*selected {
                Style::default().fg(Color::Black).bg(Color::Red).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };

            all_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(" Yes ", yes_style),
                Span::raw("   "),
                Span::styled(" No ", no_style),
            ]));

            all_lines.push(Line::from(""));
            all_lines.push(Line::from(vec![
                Span::styled("  ←→", Style::default().fg(Color::DarkGray)),
                Span::raw(" select  "),
                Span::styled("Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" confirm"),
            ]));
        }

        Interaction::TextInput { prompt, value } => {
            all_lines.push(Line::from(vec![
                Span::styled(format!("  {prompt}: "), Style::default().fg(Color::Cyan)),
                Span::raw(value),
                Span::styled("▌", Style::default().fg(Color::Cyan)),
            ]));
            all_lines.push(Line::from(vec![
                Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" confirm  "),
                Span::styled("Esc", Style::default().fg(Color::DarkGray)),
                Span::raw(" skip"),
            ]));
        }

        Interaction::Spinner { message } => {
            let frame = app.tick / 3 % SPINNER.len();
            all_lines.push(Line::from(vec![
                Span::styled(format!("  {} ", SPINNER[frame]), Style::default().fg(Color::Cyan)),
                Span::styled(message.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }

        Interaction::PressAnyKey => {
            all_lines.push(Line::from(Span::styled(
                "  Press any key to exit",
                Style::default().fg(Color::DarkGray),
            )));
        }

        Interaction::None => {}
    }

    // Scroll to bottom
    let skip = if all_lines.len() > max_visible {
        all_lines.len() - max_visible
    } else {
        0
    };

    let visible: Vec<Line> = all_lines.into_iter().skip(skip).collect();
    f.render_widget(
        Paragraph::new(visible).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));

    for (i, &name) in STEP_NAMES.iter().enumerate() {
        let (icon, style) = match app.steps[i] {
            StepStatus::Complete => ("✓", Style::default().fg(Color::Green)),
            StepStatus::Active => ("●", Style::default().fg(Color::Cyan).bold()),
            StepStatus::Skipped => ("○", Style::default().fg(Color::DarkGray)),
            StepStatus::Pending => ("○", Style::default().fg(Color::DarkGray)),
        };
        spans.push(Span::styled(format!("{icon} {name}"), style));

        if i < STEP_NAMES.len() - 1 {
            spans.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

// ── Quiet model download (no stdout) ────────────────────────

async fn download_model_quiet(
    model_id: &str,
    model_dim: usize,
    onnx_url: &str,
    tokenizer_url: &str,
) -> Result<()> {
    use crate::embed::set_embedding_dim;

    let model_dir = Config::model_dir()?;
    let model_subdir = model_dir.join(model_id);
    std::fs::create_dir_all(&model_subdir)?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    // Download ONNX model
    let model_file = model_subdir.join("model.onnx");
    if !model_file.exists() {
        let resp = client.get(onnx_url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} downloading model from {}", resp.status(), onnx_url);
        }
        let bytes = resp.bytes().await?;
        if bytes.len() < 100_000 {
            anyhow::bail!("Model file too small ({} bytes)", bytes.len());
        }
        std::fs::write(&model_file, &bytes)?;
    }

    // Download tokenizer
    let tokenizer_file = model_subdir.join("tokenizer.json");
    if !tokenizer_file.exists() {
        let resp = client.get(tokenizer_url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} downloading tokenizer", resp.status());
        }
        let bytes = resp.bytes().await?;
        std::fs::write(&tokenizer_file, &bytes)?;
    }

    // Update config
    let mut cfg = Config::load().unwrap_or_default();
    cfg.embedding_model = model_id.to_string();
    cfg.embedding_dim = model_dim;
    cfg.save()?;

    // Set runtime dimension for db::open
    set_embedding_dim(model_dim);

    // If there's an existing database, we need to recreate it
    let db_path = Config::db_path()?;
    if db_path.exists() {
        let conn = crate::db::open()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at, deleted, synced_at FROM memories ORDER BY created_at"
        )?;
        let rows: Vec<(String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, String, String, i32, Option<String>)> = stmt.query_map([], |row| {
            Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
            ))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        // Nuke and recreate
        std::fs::remove_file(&db_path)?;
        let conn = crate::db::open()?;

        for row in &rows {
            conn.execute(
                "INSERT INTO memories (id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at, deleted, synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                rusqlite::params![row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10, row.11],
            )?;
        }

        // Re-embed
        let embedder_arc = crate::embed::get_or_init()?;
        let mut embedder = embedder_arc.lock().unwrap_or_else(|e| e.into_inner());
        for row in &rows {
            if row.10 != 0 { continue; }
            let embedding = embedder.embed(&row.1)?;
            let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
            conn.execute("UPDATE memories SET embedding = ?1 WHERE id = ?2", rusqlite::params![bytes, row.0])?;
            conn.execute("INSERT OR REPLACE INTO memory_vectors (id, embedding) VALUES (?1, ?2)", rusqlite::params![row.0, bytes])?;
        }
    }

    Ok(())
}

// ── Entry Point ─────────────────────────────────────────────

pub async fn run(cfg: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(cfg.clone());
    app.advance();

    let result = run_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        app.poll_async();

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key).await?;
                }
            }
        }

        app.tick += 1;

        if app.should_quit {
            return Ok(());
        }
    }
}
