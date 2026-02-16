use anyhow::Result;
use crate::config::{Config, Tier};

pub async fn run(cfg: &Config) -> Result<()> {
    // Sync tier from cloud if logged in
    let cfg = if cfg.is_logged_in() {
        match sync_tier_from_cloud(cfg).await {
            Ok(Some(updated)) => updated,
            _ => cfg.clone(),
        }
    } else {
        cfg.clone()
    };
    let cfg = &cfg;

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

    let daemon_status = if cfg.is_remote_client() {
        format!("remote → {}", cfg.daemon_url())
    } else if service_running {
        "running (systemd) ✓".to_string()
    } else if let Some(pid) = &pid_running {
        format!("running (pid {pid})")
    } else {
        "stopped".to_string()
    };

    println!("Version:         v{}", env!("CARGO_PKG_VERSION"));
    println!("Daemon:          {daemon_status}");
    if cfg.is_remote_client() {
        println!("  REST API:      {}/v1/", cfg.daemon_url());
        println!("  MCP SSE:       {}/mcp/sse", cfg.daemon_url());
    } else if service_running || pid_running.is_some() {
        println!("  REST API:      http://localhost:{}/v1/", cfg.port);
        println!("  MCP SSE:       http://localhost:{}/mcp/sse", cfg.port);
    }
    if !cfg.is_remote_client() {
        println!("Service:         {}", if service_installed { "installed" } else { "not installed" });
    }
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

/// Fetch the user's tier from cloud and update local config if it changed.
/// Returns Some(updated_config) if tier changed, None if no change.
async fn sync_tier_from_cloud(cfg: &Config) -> Result<Option<Config>> {
    let api_key = cfg.api_key.as_deref().unwrap();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(format!("{}/v1/auth/profile", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let body: serde_json::Value = resp.json().await?;
    let tier_str = body["user"]["tier"].as_str().unwrap_or("free");
    let cloud_tier = match tier_str {
        "standard" => Tier::Standard,
        "pro" => Tier::Pro,
        _ => Tier::Free,
    };

    if cfg.tier != cloud_tier {
        let mut updated = Config::load()?;
        let old_tier = updated.tier.clone();
        updated.tier = cloud_tier;
        updated.save()?;
        println!("  ✓ Tier synced from cloud: {:?} → {:?}\n", old_tier, updated.tier);
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}
