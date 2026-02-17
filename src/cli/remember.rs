use anyhow::Result;
use crate::config::Config;

pub async fn run(cfg: &Config, text: &str, memory_type: Option<&str>, tags: Vec<String>, subject: Option<&str>) -> Result<()> {
    let conn = crate::db::open()?;
    let mtype = memory_type.unwrap_or("semantic").parse().unwrap_or_default();

    // Check limits
    let count = crate::db::memories::count(&conn)?;
    if let Some(max) = cfg.effective_max_memories() {
        if count >= max {
            eprintln!("Memory limit reached ({max}). Upgrade: https://ctxovrflw.dev/pricing");
            std::process::exit(1);
        }
    }

    let embedding = if cfg.tier.semantic_search_enabled() {
        match crate::embed::Embedder::new() {
            Ok(mut e) => match e.embed(text) {
                Ok(emb) => {
                    eprintln!("[debug] Embedding generated ({} dims)", emb.len());
                    Some(emb)
                }
                Err(e) => {
                    eprintln!("[debug] Embedding failed: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("[debug] Embedder init failed: {e}");
                None
            }
        }
    } else {
        None
    };

    let memory = crate::db::memories::store(&conn, text, &mtype, &tags, subject, Some("cli"), embedding.as_deref(), None)?;
    println!("Remembered [{}]: {}", memory.id, text);

    // Immediate push to cloud if logged in
    if cfg.is_logged_in() {
        match crate::sync::push_one(cfg, &memory.id).await {
            Ok(true) => println!("☁ Synced to cloud"),
            Ok(false) => {}
            Err(e) => eprintln!("☁ Cloud sync failed (will retry): {e}"),
        }
    }

    Ok(())
}
