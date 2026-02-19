use anyhow::Result;
use std::path::PathBuf;

use crate::config::Config;

// ── Daemon lifecycle ─────────────────────────────────────────

pub async fn start(cfg: &Config, port: u16, foreground: bool) -> Result<()> {
    if cfg.is_remote_client() {
        println!("This instance is configured to use a remote daemon at:");
        println!("  {}", cfg.daemon_url());
        println!();
        println!("To start a local daemon instead, remove `remote_daemon_url` from config.toml");
        println!("  Config: {}", Config::config_path()?.display());
        return Ok(());
    }

    if !foreground {
        // If systemd service is installed, use that
        if is_service_installed() {
            println!("Starting ctxovrflw via systemd...");
            let status = std::process::Command::new("systemctl")
                .args(["--user", "start", "ctxovrflw"])
                .status()?;
            if status.success() {
                println!("✓ ctxovrflw daemon started");
                println!("  MCP SSE:  http://127.0.0.1:{port}/mcp/sse");
                println!("  REST API: http://127.0.0.1:{port}/v1/");
                println!("  Logs:     journalctl --user -u ctxovrflw -f");
            } else {
                println!("⚠ Failed to start via systemd. Try: ctxovrflw start --foreground");
            }
            return Ok(());
        }

        // No systemd — hint to install or run foreground
        println!("No systemd service installed. Options:");
        println!("  1. Install service: ctxovrflw service install");
        println!("  2. Run in foreground: ctxovrflw start --foreground");
        return Ok(());
    }

    // Foreground mode
    tracing::info!("Starting ctxovrflw daemon on port {port}");

    // Ensure auth token exists
    let mut cfg = cfg.clone();
    cfg.ensure_auth_token()?;

    let pid_path = Config::pid_path()?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let _conn = crate::db::open()?;
    tracing::info!("Database initialized");

    let http_handle = tokio::spawn(crate::http::serve(cfg.clone(), port));

    // Auto-sync background task
    let sync_handle = if cfg.auto_sync && cfg.is_logged_in() {
        let sync_cfg = cfg.clone();
        let interval_secs = cfg.sync_interval_secs;
        tracing::info!("Auto-sync enabled (every {interval_secs}s)");
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_secs),
            );
            // Skip the first immediate tick
            interval.tick().await;
            loop {
                interval.tick().await;
                match crate::sync::run_silent(&sync_cfg).await {
                    Ok((pushed, pulled, pull_purged)) => {
                        if pushed > 0 || pulled > 0 || pull_purged > 0 {
                            tracing::info!("Auto-sync: pushed {pushed}, pulled {pulled}, purged {pull_purged}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Auto-sync failed: {e}");
                    }
                }
            }
        }))
    } else {
        if cfg.is_logged_in() {
            tracing::info!("Auto-sync disabled");
        } else {
            tracing::info!("Not logged in — auto-sync inactive. Run `ctxovrflw login` to enable.");
        }
        None
    };

    // Expiry cleanup background task — runs every 5 minutes
    let cleanup_handle = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        interval.tick().await; // skip first immediate tick
        loop {
            interval.tick().await;
            if let Ok(conn) = crate::db::open() {
                match crate::db::memories::cleanup_expired(&conn) {
                    Ok(count) if count > 0 => {
                        tracing::info!("Cleaned up {count} expired memories");
                    }
                    _ => {}
                }
            }
        }
    });

    // Background consolidation task (Pro feature)
    let consolidation_handle = if cfg.feature_enabled("consolidation") && cfg.auto_consolidation {
        let interval_secs = cfg.consolidation_interval_secs.max(300);
        tracing::info!("Auto-consolidation enabled (every {interval_secs}s)");
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                match crate::maintenance::run_consolidation_pass() {
                    Ok(report) => {
                        if report.duplicates_removed > 0 {
                            tracing::info!(
                                "Auto-consolidation: scanned {} subjects / {} memories, removed {} duplicates",
                                report.subjects_scanned,
                                report.memories_scanned,
                                report.duplicates_removed
                            );
                        } else {
                            tracing::debug!(
                                "Auto-consolidation: scanned {} subjects / {} memories, no exact duplicates",
                                report.subjects_scanned,
                                report.memories_scanned
                            );
                        }
                    }
                    Err(e) => tracing::warn!("Auto-consolidation failed: {e}"),
                }
            }
        }))
    } else {
        tracing::info!("Auto-consolidation disabled");
        None
    };

    println!("ctxovrflw daemon running on port {port}");
    println!("  MCP SSE:  http://127.0.0.1:{port}/mcp/sse");
    println!("  REST API: http://127.0.0.1:{port}/v1/");
    if cfg.auto_sync && cfg.is_logged_in() {
        println!("  Sync:     every {}s", cfg.sync_interval_secs);
    }
    if cfg.feature_enabled("consolidation") && cfg.auto_consolidation {
        println!("  Consolidation: every {}s", cfg.consolidation_interval_secs.max(300));
    }
    println!("  Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down...");

    let _ = std::fs::remove_file(&pid_path);
    http_handle.abort();
    cleanup_handle.abort();
    if let Some(h) = sync_handle {
        h.abort();
    }
    if let Some(h) = consolidation_handle {
        h.abort();
    }

    Ok(())
}

pub async fn stop(_cfg: &Config) -> Result<()> {
    // Try systemd first
    if is_service_installed() {
        let status = std::process::Command::new("systemctl")
            .args(["--user", "is-active", "ctxovrflw"])
            .output()?;
        if String::from_utf8_lossy(&status.stdout).trim() == "active" {
            let stop = std::process::Command::new("systemctl")
                .args(["--user", "stop", "ctxovrflw"])
                .status()?;
            if stop.success() {
                println!("✓ ctxovrflw daemon stopped");
                return Ok(());
            }
        }
    }

    // Fall back to PID file
    let pid_path = Config::pid_path()?;
    if !pid_path.exists() {
        println!("ctxovrflw is not running.");
        return Ok(());
    }

    let pid: u32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;

    #[cfg(unix)]
    {
        std::process::Command::new("kill").arg(pid.to_string()).output()?;
    }

    let _ = std::fs::remove_file(&pid_path);
    println!("✓ Stopped ctxovrflw (pid {pid}).");
    Ok(())
}

// ── Service management ───────────────────────────────────────

fn service_unit_path() -> PathBuf {
    let config_dir = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir().unwrap_or_default().join(".config")
    });
    config_dir.join("systemd/user/ctxovrflw.service")
}

pub fn is_service_installed() -> bool {
    service_unit_path().exists()
}

pub fn is_service_running() -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "ctxovrflw"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn service_install() -> Result<()> {
    let binary = std::env::current_exe()?
        .to_string_lossy()
        .to_string();

    let unit = format!(
r#"[Unit]
Description=ctxovrflw — Universal AI Context Layer
After=network.target

[Service]
Type=simple
ExecStart={binary} start --foreground
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=ctxovrflw=info

[Install]
WantedBy=default.target
"#
    );

    let path = service_unit_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, unit)?;
    println!("✓ Service file written to {}", path.display());

    // Reload systemd and enable
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", "ctxovrflw"])
        .status();

    // Enable lingering so service runs without active login session
    if let Ok(user) = std::env::var("USER") {
        let _ = std::process::Command::new("loginctl")
            .args(["enable-linger", &user])
            .status();
    }

    println!("✓ Service enabled (starts on login)");
    println!("  Start now:  ctxovrflw start");
    println!("  View logs:  journalctl --user -u ctxovrflw -f");
    println!("  Uninstall:  ctxovrflw service uninstall");

    Ok(())
}

pub fn service_uninstall() -> Result<()> {
    // Stop and disable
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", "ctxovrflw"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "ctxovrflw"])
        .status();

    let path = service_unit_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("✓ ctxovrflw service removed");
    Ok(())
}

pub fn service_start() -> Result<()> {
    if !is_service_installed() {
        anyhow::bail!("Service not installed. Run: ctxovrflw service install");
    }

    let status = std::process::Command::new("systemctl")
        .args(["--user", "start", "ctxovrflw"])
        .status()?;

    if status.success() {
        println!("✓ ctxovrflw daemon started");
    } else {
        println!("⚠ Failed to start. Check: journalctl --user -u ctxovrflw -f");
    }
    Ok(())
}
