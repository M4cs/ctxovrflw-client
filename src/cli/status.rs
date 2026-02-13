use anyhow::Result;
use crate::config::Config;

pub async fn run(cfg: &Config) -> Result<()> {
    let conn = crate::db::open()?;
    let count = crate::db::memories::count(&conn)?;
    let max = cfg.tier.max_memories()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "unlimited".to_string());

    println!("ctxovrflw v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Daemon status
    let service_installed = crate::daemon::is_service_installed();
    let service_running = crate::daemon::is_service_running();
    let pid_running = Config::pid_path().ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|pid| {
            let pid = pid.trim();
            std::fs::metadata(format!("/proc/{pid}")).ok().map(|_| pid.to_string())
        });

    let daemon_status = if service_running {
        "running (systemd) ✓".to_string()
    } else if let Some(pid) = &pid_running {
        format!("running (pid {pid})")
    } else {
        "stopped".to_string()
    };

    println!("Daemon:          {daemon_status}");
    if service_running || pid_running.is_some() {
        println!("  MCP SSE:       http://127.0.0.1:{}/mcp/sse", cfg.port);
        println!("  REST API:      http://127.0.0.1:{}/v1/", cfg.port);
    }
    println!("Service:         {}", if service_installed { "installed" } else { "not installed" });
    println!();

    // Memory stats
    println!("Tier:            {:?}", cfg.tier);
    println!("Memories:        {}/{}", count, max);
    println!("Semantic search: {}", if cfg.tier.semantic_search_enabled() { "enabled" } else { "keyword only" });
    println!("Cloud sync:      {}", if cfg.tier.cloud_sync_enabled() { "enabled" } else { "disabled" });
    println!();
    println!("Data dir:        {}", Config::data_dir()?.display());

    if !service_installed {
        println!();
        println!("💡 Install as service: ctxovrflw service install");
    } else if !service_running {
        println!();
        println!("💡 Start daemon: ctxovrflw start");
    }

    Ok(())
}
