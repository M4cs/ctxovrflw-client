use anyhow::Result;
use crate::config::Config;

pub async fn run(_cfg: &Config, id: &str, dry_run: bool) -> Result<()> {
    let conn = crate::db::open()?;

    if dry_run {
        if let Some(memory) = crate::db::memories::get(&conn, id)? {
            println!("Would delete: [{}] {}", memory.id, memory.content);
            println!("Run with --no-dry-run to confirm.");
        } else {
            println!("Memory {id} not found.");
        }
        return Ok(());
    }

    if crate::db::memories::delete(&conn, id)? {
        println!("Deleted memory {id}.");
    } else {
        println!("Memory {id} not found.");
    }

    Ok(())
}
