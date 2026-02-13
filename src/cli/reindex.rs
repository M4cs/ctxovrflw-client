use anyhow::Result;

use crate::db;
use crate::embed::Embedder;

pub fn run() -> Result<()> {
    let conn = db::open()?;

    // Get all non-deleted memories
    let mut stmt = conn.prepare(
        "SELECT id, content FROM memories WHERE deleted = 0"
    )?;

    let memories: Vec<(String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let total = memories.len();
    if total == 0 {
        println!("No memories to reindex.");
        return Ok(());
    }

    println!("Reindexing {} memories...", total);

    let mut embedder = Embedder::new()?;
    let mut success = 0;
    let mut failed = 0;

    for (i, (id, content)) in memories.iter().enumerate() {
        match embedder.embed(content) {
            Ok(embedding) => {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
                    rusqlite::params![id, db::memories::bytemuck_cast_pub(&embedding)],
                );
                success += 1;
            }
            Err(e) => {
                eprintln!("  Failed to embed {}: {}", &id[..8], e);
                failed += 1;
            }
        }

        // Progress every 10 items
        if (i + 1) % 10 == 0 || i + 1 == total {
            print!("\r  [{}/{}] {} embedded, {} failed", i + 1, total, success, failed);
        }
    }

    println!();
    println!("✓ Reindex complete: {} embedded, {} failed out of {} total", success, failed, total);

    Ok(())
}
