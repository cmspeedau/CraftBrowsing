//! Container-aware download lifecycle and persistence helpers.
//!
//! Network clients can stream response chunks through [`DownloadStore::write_chunk`]
//! and use [`DownloadStore::resume_headers`] when issuing a resumed HTTP request.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("file I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("download {0} does not exist")]
    NotFound(String),
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    InProgress,
    Paused,
    Completed,
    Failed { error_message: String },
    Cancelled,
}
impl DownloadState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    fn from_db(s: &str, error: Option<String>) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "failed" => Self::Failed {
                error_message: error.unwrap_or_default(),
            },
            _ => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub target_path: PathBuf,
    pub temporary_path: PathBuf,
    pub filename: String,
    pub mime_type: String,
    pub total_bytes: Option<u64>,
    pub bytes_received: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub state: DownloadState,
    pub container_id: Option<String>,
    pub created_at_epoch_ms: i64,
}

/// SQLite-backed download records and the user-selected default target directory.
pub struct DownloadStore {
    conn: Connection,
}
impl DownloadStore {
    pub fn open(path: &str) -> Result<Self, DownloadError> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn open_in_memory() -> Result<Self, DownloadError> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(conn: Connection) -> Result<Self, DownloadError> {
        conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS downloads (id TEXT PRIMARY KEY, url TEXT NOT NULL, target_path TEXT NOT NULL, temporary_path TEXT NOT NULL, filename TEXT NOT NULL, mime_type TEXT NOT NULL, total_bytes INTEGER, bytes_received INTEGER NOT NULL DEFAULT 0, etag TEXT, last_modified TEXT, state TEXT NOT NULL, error_message TEXT, container_id TEXT, created_at_epoch_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_downloads_container ON downloads(container_id); CREATE TABLE IF NOT EXISTS download_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        Ok(Self { conn })
    }
    /// Resolve the OS Downloads folder; if unavailable, use `$HOME/Downloads`, then `./Downloads`.
    pub fn default_download_dir() -> PathBuf {
        resolve_download_dir(dirs::download_dir(), dirs::home_dir())
    }
    pub fn download_dir(&self) -> Result<PathBuf, DownloadError> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM download_settings WHERE key='download_dir'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_download_dir))
    }
    pub fn set_download_dir(&self, path: impl AsRef<Path>) -> Result<(), DownloadError> {
        self.conn.execute("INSERT INTO download_settings(key,value) VALUES('download_dir',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [path.as_ref().to_string_lossy().as_ref()])?;
        Ok(())
    }
    pub fn create(
        &self,
        id: &str,
        url: &str,
        target: impl AsRef<Path>,
        filename: &str,
        mime: &str,
        container_id: Option<&str>,
    ) -> Result<DownloadRecord, DownloadError> {
        let target = target.as_ref().to_path_buf();
        let temp = part_path(&target);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.conn.execute("INSERT INTO downloads(id,url,target_path,temporary_path,filename,mime_type,state,container_id,created_at_epoch_ms) VALUES(?1,?2,?3,?4,?5,?6,'in_progress',?7,?8)", params![id,url,target.to_string_lossy(),temp.to_string_lossy(),filename,mime,container_id,now])?;
        self.get(id)?
            .ok_or_else(|| DownloadError::NotFound(id.into()))
    }
    pub fn get(&self, id: &str) -> Result<Option<DownloadRecord>, DownloadError> {
        Ok(self.conn.query_row("SELECT id,url,target_path,temporary_path,filename,mime_type,total_bytes,bytes_received,etag,last_modified,state,error_message,container_id,created_at_epoch_ms FROM downloads WHERE id=?1",[id],|r|Ok(DownloadRecord{id:r.get(0)?,url:r.get(1)?,target_path:PathBuf::from(r.get::<_,String>(2)?),temporary_path:PathBuf::from(r.get::<_,String>(3)?),filename:r.get(4)?,mime_type:r.get(5)?,total_bytes:r.get::<_,Option<i64>>(6)?.map(|v|v as u64),bytes_received:r.get::<_,i64>(7)? as u64,etag:r.get(8)?,last_modified:r.get(9)?,state:DownloadState::from_db(&r.get::<_,String>(10)?,r.get(11)?),container_id:r.get(12)?,created_at_epoch_ms:r.get(13)?})).optional()?)
    }
    pub fn list(&self, container_id: Option<&str>) -> Result<Vec<DownloadRecord>, DownloadError> {
        let sql = if container_id.is_some() {
            "SELECT id FROM downloads WHERE container_id=?1 ORDER BY created_at_epoch_ms DESC"
        } else {
            "SELECT id FROM downloads ORDER BY created_at_epoch_ms DESC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let ids = if let Some(c) = container_id {
            stmt.query_map([c], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        ids.iter()
            .map(|id| self.get(id).map(|v| v.expect("listed download exists")))
            .collect()
    }
    /// Persist response metadata and transfer progress after each streamed chunk.
    pub fn update_progress(
        &self,
        id: &str,
        bytes: u64,
        total: Option<u64>,
        etag: Option<&str>,
        modified: Option<&str>,
    ) -> Result<(), DownloadError> {
        let n=self.conn.execute("UPDATE downloads SET bytes_received=?2,total_bytes=?3,etag=COALESCE(?4,etag),last_modified=COALESCE(?5,last_modified) WHERE id=?1",params![id,bytes as i64,total.map(|v|v as i64),etag,modified])?;
        if n == 0 {
            return Err(DownloadError::NotFound(id.into()));
        }
        Ok(())
    }
    /// Append one streamed body chunk to the `.part` file and persist byte count.
    pub fn write_chunk(
        &self,
        id: &str,
        chunk: &[u8],
        total: Option<u64>,
        etag: Option<&str>,
        modified: Option<&str>,
    ) -> Result<u64, DownloadError> {
        let rec = self
            .get(id)?
            .ok_or_else(|| DownloadError::NotFound(id.into()))?;
        if rec.state != DownloadState::InProgress {
            return Err(DownloadError::InvalidTransition {
                from: rec.state.as_str().into(),
                to: "write_chunk".into(),
            });
        }
        if let Some(p) = rec.temporary_path.parent() {
            fs::create_dir_all(p)?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&rec.temporary_path)?;
        f.write_all(chunk)?;
        f.flush()?;
        let n = rec.bytes_received + chunk.len() as u64;
        self.update_progress(id, n, total, etag, modified)?;
        Ok(n)
    }
    pub fn resume_headers(&self, id: &str) -> Result<Vec<(String, String)>, DownloadError> {
        let r = self
            .get(id)?
            .ok_or_else(|| DownloadError::NotFound(id.into()))?;
        let mut h = vec![("Range".into(), format!("bytes={}-", r.bytes_received))];
        if let Some(v) = r.etag {
            h.push(("If-Range".into(), v));
        } else if let Some(v) = r.last_modified {
            h.push(("If-Range".into(), v));
        }
        Ok(h)
    }
    pub fn transition(
        &self,
        id: &str,
        state: DownloadState,
    ) -> Result<DownloadRecord, DownloadError> {
        let r = self
            .get(id)?
            .ok_or_else(|| DownloadError::NotFound(id.into()))?;
        let allowed = matches!(
            (&r.state, &state),
            (
                DownloadState::InProgress,
                DownloadState::Paused
                    | DownloadState::Completed
                    | DownloadState::Cancelled
                    | DownloadState::Failed { .. }
            ) | (
                DownloadState::Paused,
                DownloadState::InProgress | DownloadState::Cancelled | DownloadState::Failed { .. }
            )
        );
        if !allowed {
            return Err(DownloadError::InvalidTransition {
                from: r.state.as_str().into(),
                to: state.as_str().into(),
            });
        }
        if state == DownloadState::Completed {
            fs::rename(&r.temporary_path, &r.target_path)?;
        }
        if state == DownloadState::Cancelled {
            let _ = fs::remove_file(&r.temporary_path);
        }
        let err = if let DownloadState::Failed { error_message } = &state {
            Some(error_message.as_str())
        } else {
            None
        };
        self.conn.execute(
            "UPDATE downloads SET state=?2,error_message=?3 WHERE id=?1",
            params![id, state.as_str(), err],
        )?;
        self.get(id)?
            .ok_or_else(|| DownloadError::NotFound(id.into()))
    }
}
fn part_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".part");
    PathBuf::from(p)
}
fn resolve_download_dir(download: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    download
        .or_else(|| home.map(|p| p.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("Downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_transitions_complete_and_cancel() {
        let s = DownloadStore::open_in_memory().unwrap();
        let root = std::env::temp_dir().join(format!(
            "craft-download-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&root);
        let target = root.join("a.bin");
        s.create(
            "a",
            "https://a",
            &target,
            "a.bin",
            "application/octet-stream",
            Some("work"),
        )
        .unwrap();
        s.write_chunk("a", b"abc", Some(3), None, None).unwrap();
        assert_eq!(
            s.transition("a", DownloadState::Paused).unwrap().state,
            DownloadState::Paused
        );
        assert_eq!(
            s.transition("a", DownloadState::InProgress).unwrap().state,
            DownloadState::InProgress
        );
        let done = s.transition("a", DownloadState::Completed).unwrap();
        assert_eq!(done.state, DownloadState::Completed);
        assert_eq!(fs::read(&target).unwrap(), b"abc");
        assert!(matches!(
            s.transition("a", DownloadState::Cancelled),
            Err(DownloadError::InvalidTransition { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn resume_metadata_persists_and_builds_range_headers() {
        let s = DownloadStore::open_in_memory().unwrap();
        let r = s.create("b", "https://b", "/tmp/b", "b", "", None).unwrap();
        s.update_progress("b", 42, Some(100), Some("\"tag\""), Some("date"))
            .unwrap();
        let saved = s.get("b").unwrap().unwrap();
        assert_eq!(saved.bytes_received, 42);
        assert_eq!(saved.etag.as_deref(), Some("\"tag\""));
        assert_eq!(
            s.resume_headers("b").unwrap(),
            vec![
                ("Range".into(), "bytes=42-".into()),
                ("If-Range".into(), "\"tag\"".into())
            ]
        );
        let _ = r;
    }
    #[test]
    fn downloads_are_filtered_by_container() {
        let s = DownloadStore::open_in_memory().unwrap();
        s.create("a", "u", "/tmp/a", "a", "", Some("work")).unwrap();
        s.create("b", "u", "/tmp/b", "b", "", Some("personal"))
            .unwrap();
        assert_eq!(
            s.list(Some("work"))
                .unwrap()
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        assert_eq!(s.list(None).unwrap().len(), 2);
    }
    #[test]
    fn default_directory_falls_back_without_home() {
        assert_eq!(resolve_download_dir(None, None), PathBuf::from("Downloads"));
        assert_eq!(
            resolve_download_dir(None, Some(PathBuf::from("/home/test"))),
            PathBuf::from("/home/test/Downloads")
        );
    }
    #[test]
    fn custom_directory_setting_round_trips() {
        let s = DownloadStore::open_in_memory().unwrap();
        s.set_download_dir("/custom/downloads").unwrap();
        assert_eq!(
            s.download_dir().unwrap(),
            PathBuf::from("/custom/downloads")
        );
    }
}
