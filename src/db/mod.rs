pub mod memories;
pub mod search;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::Config;

pub fn open() -> Result<Connection> {
    let path = Config::db_path()?;

    // Register sqlite-vec as auto extension BEFORE opening connection
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }

    let conn = Connection::open(&path)?;

    // Performance pragmas
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA cache_size = -8000;
        ",
    )?;

    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS memories (
            id          TEXT PRIMARY KEY,
            content     TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'semantic',
            tags        TEXT NOT NULL DEFAULT '[]',
            subject     TEXT,
            source      TEXT,
            embedding   BLOB,
            expires_at  TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            synced_at   TEXT,
            deleted     INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(type);
        CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
        CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted);

        -- FTS5 for keyword search (free tier)
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            tags,
            content='memories',
            content_rowid='rowid'
        );

        -- Triggers to keep FTS in sync
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;
        ",
    )?;

    // Migrations for existing databases
    // Add subject column if missing
    let has_subject: bool = conn
        .prepare("SELECT subject FROM memories LIMIT 0")
        .is_ok();
    if !has_subject {
        conn.execute_batch("ALTER TABLE memories ADD COLUMN subject TEXT;")?;
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_memories_subject ON memories(subject);")?;

    // Add expires_at column if missing
    let has_expires_at: bool = conn
        .prepare("SELECT expires_at FROM memories LIMIT 0")
        .is_ok();
    if !has_expires_at {
        conn.execute_batch("ALTER TABLE memories ADD COLUMN expires_at TEXT;")?;
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_memories_expires_at ON memories(expires_at);")?;

    // sqlite-vec virtual table for vector search
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_vectors USING vec0(
            id TEXT PRIMARY KEY,
            embedding float[384]
        );
        ",
    )?;

    Ok(())
}
