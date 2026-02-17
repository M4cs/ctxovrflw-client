use anyhow::{Context, Result};
use chrono::Utc;
use reqwest;
use serde_json::json;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::{config::Config, db, embed};

/// List available embedding models
pub fn list() -> Result<()> {
    let cfg = Config::load().unwrap_or_default();
    let current_model = &cfg.embedding_model;

    println!("Available Embedding Models:\n");

    for model in embed::models::MODELS.iter() {
        let is_current = model.id == current_model;
        let marker = if is_current { " ✓" } else { "" };
        
        println!("  {} {}{}", model.id, model.name, marker);
        println!("    Dimensions: {}", model.dim);
        println!("    Size:       ~{} MB", model.size_mb);
        println!("    Description: {}", model.description);
        if model.requires_prefix && model.query_prefix.is_some() {
            println!("    Query prefix: \"{}\"", model.query_prefix.unwrap());
        }
        println!();
    }

    if let Some(current) = embed::models::get_model(current_model) {
        println!("Current model: {} ({})", current.id, current.name);
    } else {
        println!("Current model: {} (unknown)", current_model);
    }

    Ok(())
}

/// Show current model details
pub fn current() -> Result<()> {
    let cfg = Config::load().unwrap_or_default();
    
    if let Some(model) = embed::models::get_model(&cfg.embedding_model) {
        println!("Current Model: {}", model.name);
        println!("ID:            {}", model.id);
        println!("Dimensions:    {}", model.dim);
        println!("Size:          ~{} MB", model.size_mb);
        println!("Description:   {}", model.description);
        
        if model.requires_prefix && model.query_prefix.is_some() {
            println!("Query prefix:  \"{}\"", model.query_prefix.unwrap());
        }
        
        // Check if model files exist
        let model_dir = Config::model_dir()?;
        let model_subdir = model_dir.join(&cfg.embedding_model);
        let (model_file, tokenizer_file) = if model_subdir.exists() {
            (model_subdir.join("model.onnx"), model_subdir.join("tokenizer.json"))
        } else {
            // Legacy files
            (model_dir.join("all-MiniLM-L6-v2-q8.onnx"), model_dir.join("tokenizer.json"))
        };
        
        println!("\nFiles:");
        if model_file.exists() {
            if let Ok(metadata) = model_file.metadata() {
                println!("  Model:     {} ({:.1} MB)", model_file.display(), metadata.len() as f64 / 1_048_576.0);
            } else {
                println!("  Model:     {} (exists)", model_file.display());
            }
        } else {
            println!("  Model:     {} (missing)", model_file.display());
        }
        
        if tokenizer_file.exists() {
            if let Ok(metadata) = tokenizer_file.metadata() {
                println!("  Tokenizer: {} ({:.1} KB)", tokenizer_file.display(), metadata.len() as f64 / 1024.0);
            } else {
                println!("  Tokenizer: {} (exists)", tokenizer_file.display());
            }
        } else {
            println!("  Tokenizer: {} (missing)", tokenizer_file.display());
        }
    } else {
        println!("Current model '{}' not found in registry", cfg.embedding_model);
    }
    
    Ok(())
}

/// Switch to a different embedding model
pub async fn switch(model_id: &str) -> Result<()> {
    // Validate model exists in registry
    let model_info = embed::models::get_model(model_id)
        .context(format!("Model '{}' not found in registry", model_id))?;
    
    let mut cfg = Config::load().unwrap_or_default();
    
    if cfg.embedding_model == model_id {
        println!("Already using model '{}'", model_id);
        return Ok(());
    }
    
    println!("Switching from '{}' to '{}'", cfg.embedding_model, model_id);
    println!("Model: {} ({} dims, ~{} MB)", model_info.name, model_info.dim, model_info.size_mb);
    println!();
    
    // Check if daemon is running and warn user
    if daemon_running(&cfg).await {
        eprintln!("⚠️  Warning: The ctxovrflw daemon is running. Please stop it first:");
        eprintln!("   ctxovrflw stop");
        eprintln!();
        print!("Continue anyway? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Cancelled.");
            return Ok(());
        }
        println!();
    }
    
    let current_model_id = cfg.embedding_model.clone();
    
    // Step 1: Download new model files
    println!("📥 Downloading model files...");
    download_model_files(model_info).await?;
    
    // Step 2: Export all data from database
    println!("💾 Exporting existing data...");
    let export_data = export_all_data(&current_model_id)?;
    
    // Step 3: Close database connection and delete database file
    println!("🗑️  Removing old database...");
    let db_path = Config::db_path()?;
    if db_path.exists() {
        fs::remove_file(&db_path)
            .context("Failed to remove old database")?;
    }
    
    // Step 4: Update config with new model
    println!("⚙️  Updating configuration...");
    cfg.embedding_model = model_id.to_string();
    cfg.embedding_dim = model_info.dim; // This will be recalculated on load, but set it for consistency
    cfg.save()?;
    
    // Step 5: Open new database (creates tables with new dimension)
    println!("🏗️  Creating new database...");
    let _conn = db::open()?; // This creates tables with new embedding dimension
    
    // Step 6: Import all data
    println!("📤 Importing data...");
    import_all_data(&export_data)?;
    
    // Step 7: Re-embed all memories
    println!("🔄 Re-embedding memories with new model...");
    let reembedded_count = reembed_all_memories()?;
    
    println!("✅ Successfully switched to model '{}'", model_id);
    println!("   {} memories re-embedded", reembedded_count);
    println!();
    println!("Next steps:");
    println!("   • Restart the daemon: ctxovrflw start");
    println!("   • Test semantic search: ctxovrflw recall \"your query\"");
    
    Ok(())
}

async fn download_model_files(model_info: &embed::models::EmbeddingModel) -> Result<()> {
    let model_dir = Config::model_dir()?;
    let model_subdir = model_dir.join(model_info.id);
    fs::create_dir_all(&model_subdir)?;
    
    let client = reqwest::Client::new();
    
    // Download ONNX model
    let model_file = model_subdir.join("model.onnx");
    if !model_file.exists() {
        println!("  Downloading ONNX model...");
        download_file(&client, model_info.onnx_url, &model_file).await?;
    } else {
        println!("  ONNX model already exists");
    }
    
    // Download tokenizer
    let tokenizer_file = model_subdir.join("tokenizer.json");
    if !tokenizer_file.exists() {
        println!("  Downloading tokenizer...");
        download_file(&client, model_info.tokenizer_url, &tokenizer_file).await?;
    } else {
        println!("  Tokenizer already exists");
    }
    
    Ok(())
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &PathBuf) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .context(format!("Failed to fetch {}", url))?;
    
    if !response.status().is_success() {
        anyhow::bail!("HTTP {} when downloading {}", response.status(), url);
    }
    
    let content_length = response.content_length().unwrap_or(0);
    
    let bytes = response.bytes().await
        .context("Failed to download response body")?;
    
    fs::write(dest, &bytes)
        .context(format!("Failed to write file {}", dest.display()))?;
    
    if content_length > 0 {
        println!("    Downloaded {} MB", content_length / 1_048_576);
    } else {
        println!("    Downloaded {} KB", bytes.len() / 1024);
    }
    
    Ok(())
}

fn export_all_data(source_model: &str) -> Result<serde_json::Value> {
    let conn = db::open()?;
    
    // Export memories
    let memories = export_all_memories(&conn)?;
    
    // Export entities (Pro feature)
    let entities = export_all_entities(&conn)?;
    
    // Export relations (Pro feature)  
    let relations = export_all_relations(&conn)?;
    
    Ok(json!({
        "memories": memories,
        "entities": entities,
        "relations": relations,
        "exported_at": Utc::now().to_rfc3339(),
        "source_model": source_model,
    }))
}

fn export_all_memories(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at, deleted, synced_at
         FROM memories ORDER BY created_at"
    )?;
    
    let results = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "content": row.get::<_, String>(1)?,
            "type": row.get::<_, String>(2)?,
            "tags": row.get::<_, String>(3)?,
            "subject": row.get::<_, Option<String>>(4)?,
            "source": row.get::<_, Option<String>>(5)?,
            "agent_id": row.get::<_, Option<String>>(6)?,
            "expires_at": row.get::<_, Option<String>>(7)?,
            "created_at": row.get::<_, String>(8)?,
            "updated_at": row.get::<_, String>(9)?,
            "deleted": row.get::<_, i32>(10)?,
            "synced_at": row.get::<_, Option<String>>(11)?,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(results)
}

#[cfg(feature = "pro")]
fn export_all_entities(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>> {
    let stmt_result = conn.prepare(
        "SELECT id, name, type, properties, created_at, updated_at
         FROM entities ORDER BY created_at"
    );
    
    let mut stmt = match stmt_result {
        Ok(stmt) => stmt,
        Err(_) => return Ok(vec![]), // Table doesn't exist
    };
    
    let results = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "type": row.get::<_, String>(2)?,
            "properties": row.get::<_, String>(3)?,
            "created_at": row.get::<_, String>(4)?,
            "updated_at": row.get::<_, String>(5)?,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(results)
}

#[cfg(not(feature = "pro"))]
fn export_all_entities(_conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>> {
    Ok(vec![])
}

#[cfg(feature = "pro")]
fn export_all_relations(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>> {
    let stmt_result = conn.prepare(
        "SELECT id, source_id, source_type, target_id, target_type, relation_type, properties, created_at
         FROM relations ORDER BY created_at"
    );
    
    let mut stmt = match stmt_result {
        Ok(stmt) => stmt,
        Err(_) => return Ok(vec![]), // Table doesn't exist
    };
    
    let results = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "source_id": row.get::<_, String>(1)?,
            "source_type": row.get::<_, String>(2)?,
            "target_id": row.get::<_, String>(3)?,
            "target_type": row.get::<_, String>(4)?,
            "relation_type": row.get::<_, String>(5)?,
            "properties": row.get::<_, String>(6)?,
            "created_at": row.get::<_, String>(7)?,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(results)
}

#[cfg(not(feature = "pro"))]
fn export_all_relations(_conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>> {
    Ok(vec![])
}

fn import_all_data(export_data: &serde_json::Value) -> Result<()> {
    let conn = db::open()?;
    
    // Import memories
    let empty_memories = vec![];
    let memories = export_data["memories"].as_array().unwrap_or(&empty_memories);
    for memory in memories {
        import_memory(&conn, memory)?;
    }
    
    // Import entities (Pro feature)
    let empty_entities = vec![];
    let entities = export_data["entities"].as_array().unwrap_or(&empty_entities);
    for entity in entities {
        import_entity(&conn, entity)?;
    }
    
    // Import relations (Pro feature)
    let empty_relations = vec![];
    let relations = export_data["relations"].as_array().unwrap_or(&empty_relations);
    for relation in relations {
        import_relation(&conn, relation)?;
    }
    
    Ok(())
}

fn import_memory(conn: &rusqlite::Connection, memory: &serde_json::Value) -> Result<()> {
    conn.execute(
        "INSERT INTO memories (id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at, deleted, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            memory["id"].as_str().unwrap(),
            memory["content"].as_str().unwrap(),
            memory["type"].as_str().unwrap(),
            memory["tags"].as_str().unwrap(),
            memory["subject"].as_str(),
            memory["source"].as_str(),
            memory["agent_id"].as_str(),
            memory["expires_at"].as_str(),
            memory["created_at"].as_str().unwrap(),
            memory["updated_at"].as_str().unwrap(),
            memory["deleted"].as_i64().unwrap_or(0),
            memory["synced_at"].as_str(),
        ]
    )?;
    
    Ok(())
}

#[cfg(feature = "pro")]
fn import_entity(conn: &rusqlite::Connection, entity: &serde_json::Value) -> Result<()> {
    let result = conn.execute(
        "INSERT INTO entities (id, name, type, properties, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            entity["id"].as_str().unwrap(),
            entity["name"].as_str().unwrap(),
            entity["type"].as_str().unwrap(),
            entity["properties"].as_str().unwrap(),
            entity["created_at"].as_str().unwrap(),
            entity["updated_at"].as_str().unwrap(),
        ]
    );
    
    // Ignore errors (table might not exist)
    let _ = result;
    Ok(())
}

#[cfg(not(feature = "pro"))]
fn import_entity(_conn: &rusqlite::Connection, _entity: &serde_json::Value) -> Result<()> {
    Ok(())
}

#[cfg(feature = "pro")]
fn import_relation(conn: &rusqlite::Connection, relation: &serde_json::Value) -> Result<()> {
    let result = conn.execute(
        "INSERT INTO relations (id, source_id, source_type, target_id, target_type, relation_type, properties, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            relation["id"].as_str().unwrap(),
            relation["source_id"].as_str().unwrap(),
            relation["source_type"].as_str().unwrap(),
            relation["target_id"].as_str().unwrap(),
            relation["target_type"].as_str().unwrap(),
            relation["relation_type"].as_str().unwrap(),
            relation["properties"].as_str().unwrap(),
            relation["created_at"].as_str().unwrap(),
        ]
    );
    
    // Ignore errors (table might not exist)
    let _ = result;
    Ok(())
}

#[cfg(not(feature = "pro"))]
fn import_relation(_conn: &rusqlite::Connection, _relation: &serde_json::Value) -> Result<()> {
    Ok(())
}

fn reembed_all_memories() -> Result<usize> {
    use rusqlite::params;
    
    let conn = db::open()?;
    let embedder_arc = embed::get_or_init()?;
    let mut embedder = embedder_arc.lock().unwrap();
    
    // Get all memory IDs and content
    let mut stmt = conn.prepare(
        "SELECT id, content FROM memories WHERE deleted = 0"
    )?;
    
    let memories: Vec<(String, String)> = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    let total = memories.len();
    let mut processed = 0;
    
    for (id, content) in memories {
        // Generate embedding
        let embedding = embedder.embed(&content)?;
        let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        
        // Update memory table with new embedding
        conn.execute(
            "UPDATE memories SET embedding = ?1 WHERE id = ?2",
            params![embedding_bytes, id]
        )?;
        
        // Insert/update vector table
        conn.execute(
            "INSERT OR REPLACE INTO memory_vectors (id, embedding) VALUES (?1, ?2)",
            params![id, embedding_bytes]
        )?;
        
        processed += 1;
        if processed % 10 == 0 || processed == total {
            print!("\r  Progress: {}/{} memories", processed, total);
            io::stdout().flush()?;
        }
    }
    
    if total > 0 {
        println!(); // New line after progress
    }
    
    Ok(total)
}

async fn daemon_running(cfg: &Config) -> bool {
    let daemon_url = cfg.daemon_url();
    if let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build() 
    {
        client.get(&format!("{}/health", daemon_url)).send().await.is_ok()
    } else {
        false
    }
}