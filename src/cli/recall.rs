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
                    eprintln!("[debug] Query embedding generated, searching vectors...");
                    match crate::db::search::semantic_search(&conn, &embedding, limit) {
                        Ok(r) if !r.is_empty() => {
                            eprintln!("[debug] Semantic search returned {} results", r.len());
                            (r, SearchMethod::Semantic)
                        }
                        Ok(_) => {
                            eprintln!("[debug] Semantic search returned 0 results, falling back to keyword");
                            (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
                        }
                        Err(e) => {
                            eprintln!("[debug] Semantic search failed: {e}, falling back to keyword");
                            (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[debug] Embed failed: {e}, falling back to keyword");
                    (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
                }
            },
            Err(e) => {
                eprintln!("[debug] Embedder init failed: {e}, falling back to keyword");
                (crate::db::search::keyword_search(&conn, query, limit)?, SearchMethod::Keyword)
            }
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
