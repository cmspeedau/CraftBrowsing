//! # craft-storage
//!
//! Local SQLite storage layer for CraftBrowsing covering bookmarks, browsing
//! history, persisted tabs/sessions, user-selected search engines, the
//! container-aware download manager, and Firefox-style Multi-Account
//! Containers partitioning (cookie jars, origin storage, cache).
//!
//! The layer is schema-versioned and migrated transactionally, supports
//! file-backed profiles as well as memory-only databases (`:memory:`) for the
//! ephemeral Private Browsing mode (`PrivateSession`).

pub mod containers;
pub mod downloads;
pub mod privacy_settings;

use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};

pub use containers::{CacheEntry, Container, CookieEntry, OriginEntry};
pub use downloads::{DownloadRecord, DownloadState, DownloadStore};
pub use privacy_settings::{ClearDataOptions, ClearDataResult, PrivacySettings};

/// Current schema version. Bump and add a migration arm in [`Storage::migrate`]
/// when the layout changes.
pub const SCHEMA_VERSION: i64 = 3;

/// A user bookmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub container_id: Option<i64>,
    pub created_at_epoch_ms: i64,
}

/// One visited URL in browsing history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub container_id: Option<i64>,
    pub visited_at_epoch_ms: i64,
}

/// A persisted tab belonging to a saved session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredTab {
    pub id: i64,
    pub session_id: i64,
    pub url: String,
    pub title: String,
    /// Parent tab id for tree restoration; `None` for roots.
    pub parent_tab_id: Option<i64>,
    /// Sibling order index for deterministic tree reconstruction.
    pub position: i64,
    pub pinned: bool,
    pub hibernated: bool,
    pub group_id: Option<i64>,
    pub container_id: Option<i64>,
    pub last_active_epoch_ms: i64,
}

/// A user-configured search engine selectable from the address bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchEngine {
    pub id: i64,
    pub name: String,
    /// URL template containing a `{searchTerms}` placeholder.
    pub query_template: String,
    pub is_default: bool,
    pub position: i64,
}

/// Handle to an open profile database.
pub struct Storage {
    pub(crate) conn: Connection,
}

impl Storage {
    /// Open (or create) a file-backed profile database and run migrations.
    pub fn open(path: &str) -> SqlResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    /// Open an ephemeral, memory-only database (Private Browsing mode).
    pub fn open_in_memory() -> SqlResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    /// Open a hardened, memory-only Private Browsing database.
    ///
    /// Guarantees:
    /// - Every byte lives in RAM: SQLite never touches a disk file.
    /// - `journal_mode = MEMORY` — no rollback journal is written anywhere.
    /// - `locking_mode = EXCLUSIVE` — the private store cannot be attached to
    ///   by any other connection for the lifetime of the session.
    /// - `secure_delete = ON` — purged rows are overwritten in place before
    ///   the memory is released.
    ///
    /// Pair with [`PrivateSession`] so the store is fully purged on drop.
    pub fn open_private() -> SqlResult<Self> {
        let storage = Self::open_in_memory()?;
        storage.conn.execute_batch(
            "PRAGMA journal_mode = MEMORY;
             PRAGMA locking_mode = EXCLUSIVE;
             PRAGMA secure_delete = ON;",
        )?;
        Ok(storage)
    }

    fn from_connection(conn: Connection) -> SqlResult<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        let storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    /// Transactional, versioned schema migrations.
    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version INTEGER PRIMARY KEY NOT NULL
             );",
        )?;
        let current: Option<i64> = self
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .optional()?
            .filter(|v| *v >= 1);

        if current.is_none() {
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE bookmarks (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     url TEXT NOT NULL,
                     title TEXT NOT NULL DEFAULT '',
                     container_id INTEGER,
                     created_at_epoch_ms INTEGER NOT NULL
                 );
                 CREATE INDEX idx_bookmarks_url ON bookmarks(url);
                 CREATE TABLE history (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     url TEXT NOT NULL,
                     title TEXT NOT NULL DEFAULT '',
                     container_id INTEGER,
                     visited_at_epoch_ms INTEGER NOT NULL
                 );
                 CREATE INDEX idx_history_visited ON history(visited_at_epoch_ms DESC);
                 CREATE TABLE search_engines (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     name TEXT NOT NULL,
                     query_template TEXT NOT NULL,
                     is_default INTEGER NOT NULL DEFAULT 0,
                     position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE sessions (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     name TEXT NOT NULL DEFAULT '',
                     active_tab_id INTEGER,
                     created_at_epoch_ms INTEGER NOT NULL
                 );
                 CREATE TABLE tab_groups (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                     remote_group_id INTEGER NOT NULL,
                     name TEXT NOT NULL DEFAULT '',
                     color_hex TEXT NOT NULL DEFAULT '#4a90d9',
                     container_id INTEGER,
                     collapsed INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE tabs (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                     url TEXT NOT NULL,
                     title TEXT NOT NULL DEFAULT '',
                     parent_tab_id INTEGER,
                     position INTEGER NOT NULL DEFAULT 0,
                     pinned INTEGER NOT NULL DEFAULT 0,
                     hibernated INTEGER NOT NULL DEFAULT 0,
                     group_id INTEGER,
                     container_id INTEGER,
                     last_active_epoch_ms INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX idx_tabs_session ON tabs(session_id, position);
                 INSERT INTO schema_version(version) VALUES (1);
                 COMMIT;",
            )?;
        }

        // v2: Multi-Account Containers — partitioned cookie jars, origin
        // storage, and cache keyed by UserContextId (`container_id`, where
        // NULL means the default jar).
        if current.unwrap_or(0) < 2 {
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE containers (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     name TEXT NOT NULL,
                     color_hex TEXT NOT NULL DEFAULT '#4a90d9',
                     icon TEXT NOT NULL DEFAULT 'fingerprint',
                     created_at_epoch_ms INTEGER NOT NULL
                 );
                 CREATE TABLE cookies (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     container_id INTEGER,
                     domain TEXT NOT NULL,
                     name TEXT NOT NULL,
                     value TEXT NOT NULL,
                     created_at_epoch_ms INTEGER NOT NULL,
                     UNIQUE (container_id, domain, name)
                 );
                 CREATE INDEX idx_cookies_jar ON cookies(container_id, domain);
                 CREATE TABLE origin_storage (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     container_id INTEGER,
                     origin TEXT NOT NULL,
                     key TEXT NOT NULL,
                     value TEXT NOT NULL,
                     UNIQUE (container_id, origin, key)
                 );
                 CREATE INDEX idx_origin_partition ON origin_storage(container_id, origin);
                 CREATE TABLE cache_entries (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     container_id INTEGER,
                     url TEXT NOT NULL,
                     size_bytes INTEGER NOT NULL DEFAULT 0,
                     cached_at_epoch_ms INTEGER NOT NULL
                 );
                 CREATE INDEX idx_cache_partition ON cache_entries(container_id);
                 INSERT INTO schema_version(version) VALUES (2);
                 COMMIT;",
            )?;
        }
        // v3: persistent key/value preferences backing the privacy dashboard.
        if current.unwrap_or(0) < 3 {
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE privacy_settings (
                     key TEXT PRIMARY KEY NOT NULL,
                     value TEXT NOT NULL
                 );
                 INSERT INTO schema_version(version) VALUES (3);
                 COMMIT;",
            )?;
        }
        Ok(())
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Hard purge of every user-data row (used when a private session ends).
    pub fn purge_all(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "DELETE FROM cache_entries;
             DELETE FROM origin_storage;
             DELETE FROM cookies;
             DELETE FROM containers;
             DELETE FROM tabs;
             DELETE FROM tab_groups;
             DELETE FROM sessions;
             DELETE FROM history;
             DELETE FROM bookmarks;
             VACUUM;",
        )?;
        Ok(())
    }

    // -- Bookmarks -----------------------------------------------------------

    /// Add a bookmark and return its new row id.
    pub fn add_bookmark(&self, url: &str, title: &str, container_id: Option<i64>) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO bookmarks (url, title, container_id, created_at_epoch_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![url, title, container_id, Self::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Remove a bookmark by id.
    pub fn remove_bookmark(&self, id: i64) -> SqlResult<bool> {
        Ok(self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", [id])? > 0)
    }

    /// List bookmarks in insertion order.
    pub fn list_bookmarks(&self) -> SqlResult<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, container_id, created_at_epoch_ms FROM bookmarks ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                container_id: row.get(3)?,
                created_at_epoch_ms: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Whether `url` is bookmarked.
    pub fn is_bookmarked(&self, url: &str) -> SqlResult<bool> {
        self.conn
            .query_row("SELECT 1 FROM bookmarks WHERE url = ?1 LIMIT 1", [url], |_| Ok(()))
            .optional()
            .map(|o| o.is_some())
    }

    // -- History -------------------------------------------------------------

    /// Record a visit. Container-id partitioning keeps jars separate in queries.
    pub fn record_history_visit(
        &self,
        url: &str,
        title: &str,
        container_id: Option<i64>,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO history (url, title, container_id, visited_at_epoch_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![url, title, container_id, Self::now_ms()],
        )?;
        Ok(())
    }

    /// Most recent history entries, optionally scoped to one container.
    pub fn recent_history(&self, limit: u32, container_id: Option<i64>) -> SqlResult<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, container_id, visited_at_epoch_ms
             FROM history
             WHERE (?1 IS NULL OR container_id = ?1)
             ORDER BY visited_at_epoch_ms DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![container_id, limit], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                container_id: row.get(3)?,
                visited_at_epoch_ms: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Clear the entire browsing history, optionally per container only.
    pub fn clear_history(&self, container_id: Option<i64>) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM history WHERE (?1 IS NULL OR container_id = ?1)",
            params![container_id],
        )
    }

    // -- Search engines --------------------------------------------------------

    /// Register a search engine; `make_default` clears the previous default.
    pub fn add_search_engine(
        &self,
        name: &str,
        query_template: &str,
        make_default: bool,
    ) -> SqlResult<i64> {
        let position: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM search_engines", [], |r| r.get(0))?;
        if make_default {
            self.conn.execute("UPDATE search_engines SET is_default = 0", [])?;
        }
        self.conn.execute(
            "INSERT INTO search_engines (name, query_template, is_default, position)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, query_template, make_default, position],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Set an existing engine as the sole default.
    pub fn set_default_search_engine(&self, id: i64) -> SqlResult<()> {
        self.conn.execute("UPDATE search_engines SET is_default = 0", [])?;
        self.conn.execute("UPDATE search_engines SET is_default = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Return configured search engines in position order.
    pub fn list_search_engines(&self) -> SqlResult<Vec<SearchEngine>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, query_template, is_default, position FROM search_engines ORDER BY position",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SearchEngine {
                id: row.get(0)?,
                name: row.get(1)?,
                query_template: row.get(2)?,
                is_default: row.get(3)?,
                position: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    // -- Sessions and tabs ----------------------------------------------------

    /// Save a session, replacing its tabs and groups atomically.
    pub fn save_session(&self, name: &str, active_tab_id: Option<i64>, tabs: &[StoredTabSeed], created_at_epoch_ms: i64) -> SqlResult<i64> {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        let result = (|| {
            self.conn.execute(
                "INSERT INTO sessions (name, active_tab_id, created_at_epoch_ms) VALUES (?1, ?2, ?3)",
                params![name, active_tab_id, created_at_epoch_ms],
            )?;
            let session_id = self.conn.last_insert_rowid();
            for tab in tabs {
                self.conn.execute(
                    "INSERT INTO tabs (session_id, url, title, parent_tab_id, position, pinned, hibernated, group_id, container_id, last_active_epoch_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![session_id, tab.url, tab.title, tab.parent_tab_id, tab.position, tab.pinned, tab.hibernated, tab.group_id, tab.container_id, tab.last_active_epoch_ms],
                )?;
            }
            Ok(session_id)
        })();
        match result {
            Ok(id) => { self.conn.execute("COMMIT", [])?; Ok(id) }
            Err(error) => { let _ = self.conn.execute("ROLLBACK", []); Err(error) }
        }
    }

    /// Load the most recently saved session and its ordered tabs.
    pub fn load_latest_session(&self) -> SqlResult<Option<SessionRecord>> {
        let session: Option<(i64, String, Option<i64>, i64)> = self.conn
            .query_row("SELECT id, name, active_tab_id, created_at_epoch_ms FROM sessions ORDER BY created_at_epoch_ms DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
        let Some((id, name, active_tab_id, created_at_epoch_ms)) = session else { return Ok(None) };
        let mut stmt = self.conn.prepare("SELECT id, session_id, url, title, parent_tab_id, position, pinned, hibernated, group_id, container_id, last_active_epoch_ms FROM tabs WHERE session_id=?1 ORDER BY position")?;
        let tabs = stmt.query_map([id], |r| Ok(StoredTab { id:r.get(0)?, session_id:r.get(1)?, url:r.get(2)?, title:r.get(3)?, parent_tab_id:r.get(4)?, position:r.get(5)?, pinned:r.get(6)?, hibernated:r.get(7)?, group_id:r.get(8)?, container_id:r.get(9)?, last_active_epoch_ms:r.get(10)? }))?.collect::<SqlResult<Vec<_>>>()?;
        Ok(Some(SessionRecord { id, name, active_tab_id, created_at_epoch_ms, tabs }))
    }

    /// Delete a saved session.
    pub fn delete_session(&self, session_id: i64) -> SqlResult<bool> {
        Ok(self.conn.execute("DELETE FROM sessions WHERE id=?1", [session_id])? > 0)
    }
}

/// Seed record used to persist a tab before its database id has been assigned.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredTabSeed {
    pub url: String, pub title: String, pub parent_tab_id: Option<i64>, pub position: i64,
    pub pinned: bool, pub hibernated: bool, pub group_id: Option<i64>, pub container_id: Option<i64>,
    pub last_active_epoch_ms: i64,
}

/// A saved browser session and its tabs.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord { pub id: i64, pub name: String, pub active_tab_id: Option<i64>, pub created_at_epoch_ms: i64, pub tabs: Vec<StoredTab> }

/// Private-mode owner. Dropping the handle purges and closes the in-memory store.
pub struct PrivateSession { storage: Storage }
impl PrivateSession {
    pub fn start() -> SqlResult<Self> { Ok(Self { storage: Storage::open_private()? }) }
    pub fn storage(&self) -> &Storage { &self.storage }
    pub fn end(mut self) -> SqlResult<()> { self.storage.purge_all()?; Ok(()) }
}
impl Drop for PrivateSession { fn drop(&mut self) { let _ = self.storage.purge_all(); } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_container_partitioning_and_clear() {
        let s = Storage::open_in_memory().unwrap();
        s.record_history_visit("https://a", "A", Some(1)).unwrap();
        s.record_history_visit("https://b", "B", Some(2)).unwrap();
        assert_eq!(s.recent_history(10, Some(1)).unwrap().len(), 1);
        assert_eq!(s.clear_history(Some(1)).unwrap(), 1);
        assert_eq!(s.recent_history(10, None).unwrap().len(), 1);
    }
}
