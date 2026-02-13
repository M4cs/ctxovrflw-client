use anyhow::Result;
use crate::config::Config;

pub async fn run(cfg: &Config) -> Result<()> {
    if !cfg.is_logged_in() {
        println!("Not logged in.");
        return Ok(());
    }

    let mut cfg = cfg.clone();
    cfg.api_key = None;
    cfg.device_id = None;
    cfg.save()?;

    println!("✓ Logged out. Cloud sync disabled.");
    Ok(())
}
