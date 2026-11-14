//! SQLite storage primitives for bookmarks, history, tabs, and containers.

#[cfg(feature = "storage")]
use rusqlite::{Connection, Result, params};

#[cfg(feature = "storage")]
pub struct StorageEngine {
    conn: Connection,
}

#[cfg(feature = "storage")]
impl StorageEngine {
    /// In-memory SQLite database (used for Ephemeral Private Browsing mode)
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.initialize_tables()?;
        Ok(storage)
    }

    /// Persistent on-disk SQLite database
    pub fn open_path(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.initialize_tables()?;
        Ok(storage)
    }

    fn initialize_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                folder TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1,
                last_visit_time INTEGER NOT NULL,
                container_id TEXT NOT NULL DEFAULT 'default'
            );

            CREATE TABLE IF NOT EXISTS containers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                icon TEXT NOT NULL,
                color_hex TEXT NOT NULL,
                user_context_id INTEGER NOT NULL UNIQUE
            );
            "#,
        )?;
        Ok(())
    }

    pub fn insert_bookmark(
        &self,
        id: &str,
        url: &str,
        title: &str,
        folder: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bookmarks (id, url, title, folder, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, url, title, folder, created_at as i64],
        )?;
        Ok(())
    }
}
