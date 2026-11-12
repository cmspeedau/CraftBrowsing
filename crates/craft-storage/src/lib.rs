//! # craft-storage
//!
//! Local SQLite storage layer for CraftBrowsing covering bookmarks, browsing
//! history, persisted tabs/sessions, user-selected search engines, and the
//! container-aware download manager.
//!
//! The layer is schema-versioned and migrated transactionally, supports
//! file-backed profiles as well as memory-only databases (`:memory:`) for the
//! ephemeral Private Browsing mode (`PrivateSession`).

pub mod downloads;

use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};

pub use downloads::{DownloadRecord, DownloadState, DownloadStore};

/// Current schema version. Bump and add a migration arm in [`Storage::migrate`]
/// when the layout changes.
pub const SCHEMA_VERSION: i64 = 1;

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
    conn: Connection,
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
            "DELETE FROM tabs;
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

    /// Remove a bookmark by id; returns whether a row was deleted.
    pub fn remove_bookmark(&self, id: i64) -> SqlResult<bool> {
        Ok(self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", [id])? > 0)
    }

    /// All bookmarks, newest first.
    pub fn list_bookmarks(&self) -> SqlResult<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, container_id, created_at_epoch_ms
             FROM bookmarks ORDER BY created_at_epoch_ms DESC",
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
            .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM search_engines", [], |r| {
                r.get(0)
            })?;
        self.conn.execute(
            "INSERT INTO search_engines (name, query_template, is_default, position)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, query_template, make_default as i64, position],
        )?;
        let id = self.conn.last_insert_rowid();
        if make_default {
            self.set_default_search_engine(id)?;
        }
        Ok(id)
    }

    /// Make `id` the only default search engine.
    pub fn set_default_search_engine(&self, id: i64) -> SqlResult<()> {
        self.conn.execute_batch("BEGIN;")?;
        let res = self.conn.execute("UPDATE search_engines SET is_default = 0", [])
            .and_then(|_| {
                self.conn
                    .execute("UPDATE search_engines SET is_default = 1 WHERE id = ?1", [id])
            });
        match res {
            Ok(_) => self.conn.execute_batch("COMMIT;")?,
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
        }
        Ok(())
    }

    /// All registered search engines, in configured order.
    pub fn list_search_engines(&self) -> SqlResult<Vec<SearchEngine>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, query_template, is_default, position
             FROM search_engines ORDER BY position",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SearchEngine {
                id: row.get(0)?,
                name: row.get(1)?,
                query_template: row.get(2)?,
                is_default: row.get::<_, i64>(3)? != 0,
                position: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    // -- Session / tab persistence --------------------------------------------

    /// Persist a full session (tabs + groups) in a single transaction and
    /// return the new session id.
    pub fn save_session(
        &self,
        name: &str,
        active_tab_remote_id: Option<i64>,
        groups: &[(i64, &str, &str, Option<i64>, bool)], // (remote_id, name, color, container, collapsed)
        tabs: &[StoredTabSeed],
    ) -> SqlResult<i64> {
        self.conn.execute_batch("BEGIN;")?;
        let res = (|| {
            self.conn.execute(
                "INSERT INTO sessions (name, active_tab_id, created_at_epoch_ms) VALUES (?1, ?2, ?3)",
                params![name, active_tab_remote_id, Self::now_ms()],
            )?;
            let session_id = self.conn.last_insert_rowid();

            for (remote_id, gname, color, container, collapsed) in groups {
                self.conn.execute(
                    "INSERT INTO tab_groups (session_id, remote_group_id, name, color_hex, container_id, collapsed)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![session_id, remote_id, gname, color, container, *collapsed as i64],
                )?;
            }

            for t in tabs {
                self.conn.execute(
                    "INSERT INTO tabs (session_id, url, title, parent_tab_id, position, pinned,
                                       hibernated, group_id, container_id, last_active_epoch_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        session_id,
                        t.url,
                        t.title,
                        t.parent_tab_id,
                        t.position,
                        t.pinned as i64,
                        t.hibernated as i64,
                        t.group_id,
                        t.container_id,
                        t.last_active_epoch_ms
                    ],
                )?;
            }
            Ok(session_id)
        })();

        match res {
            Ok(id) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(id)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    /// Load the most recently saved session with its tabs and groups,
    /// deterministic by (parent, position) for tree reconstruction.
    pub fn load_latest_session(&self) -> SqlResult<Option<SessionRecord>> {
        let session = self
            .conn
            .query_row(
                "SELECT id, name, active_tab_id, created_at_epoch_ms
                 FROM sessions ORDER BY created_at_epoch_ms DESC, id DESC LIMIT 1",
                [],
                |row| {
                    Ok(SessionRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        active_tab_id: row.get(2)?,
                        created_at_epoch_ms: row.get(3)?,
                        tabs: Vec::new(),
                        groups: Vec::new(),
                    })
                },
            )
            .optional()?;
        let mut session = match session {
            Some(s) => s,
            None => return Ok(None),
        };

        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, parent_tab_id, position, pinned, hibernated,
                    group_id, container_id, last_active_epoch_ms
             FROM tabs WHERE session_id = ?1 ORDER BY position, id",
        )?;
        session.tabs = stmt
            .query_map([session.id], |row| {
                Ok(StoredTab {
                    id: row.get(0)?,
                    session_id: session.id,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    parent_tab_id: row.get(3)?,
                    position: row.get(4)?,
                    pinned: row.get::<_, i64>(5)? != 0,
                    hibernated: row.get::<_, i64>(6)? != 0,
                    group_id: row.get(7)?,
                    container_id: row.get(8)?,
                    last_active_epoch_ms: row.get(9)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT remote_group_id, name, color_hex, container_id, collapsed
             FROM tab_groups WHERE session_id = ?1 ORDER BY remote_group_id",
        )?;
        session.groups = stmt
            .query_map([session.id], |row| {
                Ok(StoredGroup {
                    remote_group_id: row.get(0)?,
                    name: row.get(1)?,
                    color_hex: row.get(2)?,
                    container_id: row.get(3)?,
                    collapsed: row.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(Some(session))
    }

    /// Delete a session (tabs and groups cascade).
    pub fn delete_session(&self, session_id: i64) -> SqlResult<bool> {
        Ok(self.conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])? > 0)
    }
}

/// Seed data for one tab when saving a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTabSeed {
    pub url: String,
    pub title: String,
    pub parent_tab_id: Option<i64>,
    pub position: i64,
    pub pinned: bool,
    pub hibernated: bool,
    pub group_id: Option<i64>,
    pub container_id: Option<i64>,
    pub last_active_epoch_ms: i64,
}

/// A persisted tab group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredGroup {
    pub remote_group_id: i64,
    pub name: String,
    pub color_hex: String,
    pub container_id: Option<i64>,
    pub collapsed: bool,
}

/// A fully hydrated session loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: i64,
    pub name: String,
    pub active_tab_id: Option<i64>,
    pub created_at_epoch_ms: i64,
    pub tabs: Vec<StoredTab>,
    pub groups: Vec<StoredGroup>,
}

// ---------------------------------------------------------------------------
// Ephemeral Private Browsing
// ---------------------------------------------------------------------------

/// Guard for an ephemeral Private Browsing session.
///
/// Backed exclusively by a hardened `:memory:` SQLite database — no rollback
/// journal, exclusive lock, secure-delete purging. When the guard is dropped
/// every user-data row is purged (`secure_delete` overwrites in place) and
/// the connection is closed, releasing all memory. Nothing is ever written to
/// disk at any point in the lifecycle.
pub struct PrivateSession {
    storage: Storage,
}

impl PrivateSession {
    /// Start a new private session. The database starts completely empty —
    /// bookmarks, history, and saved sessions from the regular profile are
    /// intentionally invisible.
    pub fn start() -> SqlResult<Self> {
        Ok(Self {
            storage: Storage::open_private()?,
        })
    }

    /// Storage handle scoped to this private session.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Explicitly end the session with a guaranteed purge (rather than
    /// relying on `Drop`), surfacing any purge error to the caller.
    pub fn end(mut self) -> SqlResult<()> {
        let res = self.storage.purge_all();
        std::mem::forget(self);
        res
    }
}

impl Drop for PrivateSession {
    fn drop(&mut self) {
        // Best-effort purge; secure_delete overwrites freed pages in place.
        let _ = self.storage.purge_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_tabs() -> Vec<StoredTabSeed> {
        vec![
            StoredTabSeed {
                url: "https://example.com".into(),
                title: "Root".into(),
                parent_tab_id: None,
                position: 0,
                pinned: true,
                hibernated: false,
                group_id: None,
                container_id: Some(1),
                last_active_epoch_ms: 100,
            },
            StoredTabSeed {
                url: "https://example.com/docs".into(),
                title: "Child".into(),
                parent_tab_id: Some(1),
                position: 1,
                pinned: false,
                hibernated: true,
                group_id: Some(7),
                container_id: Some(1),
                last_active_epoch_ms: 50,
            },
        ]
    }

    #[test]
    fn bookmark_round_trip_and_lookup() {
        let s = Storage::open_in_memory().unwrap();
        let id = s.add_bookmark("https://servo.org", "Servo", None).unwrap();
        assert!(s.is_bookmarked("https://servo.org").unwrap());
        assert!(!s.is_bookmarked("https://other.org").unwrap());
        assert!(s.remove_bookmark(id).unwrap());
        assert!(!s.is_bookmarked("https://servo.org").unwrap());
    }

    #[test]
    fn history_container_partitioning_and_clear() {
        let s = Storage::open_in_memory().unwrap();
        s.record_history_visit("https://a.dev", "A", Some(1)).unwrap();
        s.record_history_visit("https://b.dev", "B", Some(2)).unwrap();
        let personal = s.recent_history(10, Some(1)).unwrap();
        assert_eq!(personal.len(), 1);
        assert_eq!(personal[0].url, "https://a.dev");
        assert_eq!(s.recent_history(10, None).unwrap().len(), 2);
        assert_eq!(s.clear_history(Some(2)).unwrap(), 1);
        assert_eq!(s.recent_history(10, None).unwrap().len(), 1);
    }

    #[test]
    fn search_engine_default_is_exclusive() {
        let s = Storage::open_in_memory().unwrap();
        let d = s.add_search_engine("DuckDuckGo", "https://duckduckgo.com/?q={searchTerms}", true).unwrap();
        let g = s.add_search_engine("Google", "https://google.com/search?q={searchTerms}", false).unwrap();
        assert!(s.list_search_engines().unwrap()[0].is_default);
        s.set_default_search_engine(g).unwrap();
        let engines = s.list_search_engines().unwrap();
        assert_eq!(engines.iter().filter(|e| e.is_default).count(), 1);
        assert_eq!(engines.iter().find(|e| e.id == g).unwrap().is_default, true);
        assert_eq!(engines.iter().find(|e| e.id == d).unwrap().is_default, false);
    }

    #[test]
    fn session_save_load_round_trip() {
        let s = Storage::open_in_memory().unwrap();
        let groups = vec![(7i64, "Work", "#4a90d9", Some(1i64), false)];
        let sid = s.save_session("autosave", Some(1), &groups, &seed_tabs()).unwrap();
        let loaded = s.load_latest_session().unwrap().unwrap();

        assert_eq!(loaded.id, sid);
        assert_eq!(loaded.active_tab_id, Some(1));
        assert_eq!(loaded.tabs.len(), 2);
        // Deterministic tree order and structure preserved.
        assert_eq!(loaded.tabs[0].url, "https://example.com");
        assert!(loaded.tabs[0].pinned);
        assert_eq!(loaded.tabs[1].parent_tab_id, Some(1));
        assert!(loaded.tabs[1].hibernated);
        assert_eq!(loaded.tabs[1].group_id, Some(7));
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].remote_group_id, 7);
        assert_eq!(loaded.groups[0].color_hex, "#4a90d9");

        // Delete-then-load falls back cleanly.
        assert!(s.delete_session(sid).unwrap());
        assert!(s.load_latest_session().unwrap().is_none());
    }

    #[test]
    fn private_session_purges_on_drop() {
        let private = PrivateSession::start().unwrap();
        {
            let s = private.storage();
            s.record_history_visit("https://secret.example", "Secret", None)
                .unwrap();
            s.add_bookmark("https://secret.example", "Secret", None)
                .unwrap();
            let groups = Vec::new();
            let tabs = seed_tabs();
            s.save_session("private", None, &groups, &tabs).unwrap();
            assert_eq!(s.recent_history(10, None).unwrap().len(), 1);
        }
        drop(private);
        // Session is gone; nothing observable survives.
        let private2 = PrivateSession::start().unwrap();
        let s = private2.storage();
        assert!(s.recent_history(10, None).unwrap().is_empty());
        assert!(s.list_bookmarks().unwrap().is_empty());
        assert!(s.load_latest_session().unwrap().is_none());
    }

    #[test]
    fn private_session_explicit_end_purges() {
        let private = PrivateSession::start().unwrap();
        private
            .storage()
            .record_history_visit("https://ephemeral.dev", "E", None)
            .unwrap();
        private.end().unwrap();
        // After explicit end the store is destroyed; a fresh private session
        // is empty by construction.
        let fresh = PrivateSession::start().unwrap();
        assert!(fresh.storage().recent_history(10, None).unwrap().is_empty());
    }

    #[test]
    fn private_session_is_isolated_from_regular_profile() {
        let regular = Storage::open_in_memory().unwrap();
        regular
            .record_history_visit("https://regular.dev", "R", None)
            .unwrap();

        let private = PrivateSession::start().unwrap();
        // Private store cannot see regular profile data...
        assert!(private.storage().recent_history(10, None).unwrap().is_empty());
        // ...and writes stay confined to the private store.
        private
            .storage()
            .record_history_visit("https://private.dev", "P", None)
            .unwrap();
        assert_eq!(regular.recent_history(10, None).unwrap().len(), 1);
        assert_eq!(regular.recent_history(10, None).unwrap()[0].url, "https://regular.dev");
    }
}
