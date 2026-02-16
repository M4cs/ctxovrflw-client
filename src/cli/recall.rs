use anyhow::Result;
use crate::config::Config;

pub async fn run(cfg: &Config, query: &str, limit: usize) -> Result<()> {
    // Sync before recall to get latest from other devices
    if cfg.is_logged_in() {
        let _ = crate::sync::run_silent(cfg).await;
    }

    let conn = crate::db::open()?;

    use crate::db::search::SearchMethod;

    let (results, method) = if cfg.tier.semantic_search_enabled() {
        match crate::embed::Embedder::new() {
            Ok(mut embedder) => match embedder.embed(query) {
                Ok(embedding) => {
                    #[cfg(feature = "pro")]
                    {
                        match crate::db::search::hybrid_search(&conn, query, &embedding, limit) {
                            Ok(r) if !r.is_empty() => (r, SearchMethod::Hybrid),
                            _ => (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword),
                        }
                    }
                    #[cfg(not(feature = "pro"))]
                    {
                        let sem = crate::db::search::semantic_search(&conn, &embedding, limit)?;
                        if !sem.is_empty() {
                            (sem, SearchMethod::Semantic)
                        } else {
                            (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
                        }
                    }
                }
                Err(_) => (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword),
            },
            Err(_) => (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword),
        }
    } else {
        (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
    };

    if results.is_empty() {
        println!("No memories found for: {query}");
        return Ok(());
    }

    println!("Search method: {method}\n");

    for (memory, score) in &results {
        println!("[{}] (score: {:.2}, type: {}) {}", memory.id, score, memory.memory_type, memory.content);
        if !memory.tags.is_empty() {
            println!("     tags: {}", memory.tags.join(", "));
        }
    }

    Ok(())
}
