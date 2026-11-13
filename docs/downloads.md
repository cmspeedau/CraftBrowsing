# Basic Download Manager Specification

> Note: Direct HTTP(S) downloads are being superseded by the unified transfer
> engine in `docs/transfers.md` (v0.5.0), which adds 4-part segmented HTTP
> downloads and magnet/torrent support. This document remains the baseline for
> container awareness, safety checks, and the `craft://downloads` UI.
CraftBrowsing includes a built-in, lightweight, memory-safe download manager designed for transparency, security, and container awareness.

---

## Architecture Overview

Downloads in CraftBrowsing are managed by the `craft-downloads` crate, decoupling transfer operations from browser tabs and rendering pipelines so active downloads continue uninterrupted if a tab is navigated or closed.

```text
HTTP(S) Response (with Content-Disposition / binary stream)
       │
       ▼
craft-engine (Network interceptor)
       │
       ▼
craft-downloads (Transfer Manager)
 ├── Container Context (routes through container's proxy/cookies)
 ├── Temporary File Buffer (`.part` streaming via tokio::io)
 ├── Pause / Resume Engine (HTTP Range headers)
 ├── Safety & Quarantine (executable warnings, container tagging)
 └── Metadata persistence (craft-storage SQLite: history & state)
       │
       ▼
craft-app UI (`craft://downloads` & chrome toolbar panel)
```

---

## Core Capabilities

### 1. Robust Transfer Lifecycle
- **Streaming to Disk:** Files stream directly to disk using `tokio::fs` asynchronous buffers without loading large files into memory.
- **`.part` Incomplete File Pattern:** In-progress transfers write to `<filename>.part`. Upon successful SHA verification / EOF, the file is atomically renamed to its final target name.
- **Pause & Resume Support:** Leverages standard HTTP `Range` requests (`bytes=X-`) and `ETag`/`Last-Modified` validation headers to safely resume interrupted transfers.
- **Cancellation & Cleanup:** Users can cancel transfers at any point; partially written temporary files are cleaned up immediately unless explicitly retained.

### 2. Container Identity Awareness
Downloads maintain context from the tab that initiated them:
- **Cookie & Proxy Heritage:** Uses the session cookies, HTTP authentication, and proxy routing configured for the originating container tab.
- **Origin Metadata:** Download history records which container downloaded the file (e.g. *Personal*, *Work*, *Banking*).

### 3. Safety & User Protections
- **Executable Warnings:** Downloads of potentially dangerous file extensions (`.exe`, `.msi`, `.sh`, `.bat`, `.cmd`, `.app`, `.dmg`) prompt a clear user confirmation dialog before opening or saving.
- **Disk Space Checks:** Checks available storage on the target volume before commencing large downloads when `Content-Length` is present.
- **No Hidden Downloads:** Automatic drive-by downloads without direct user gesture trigger an explicit confirmation bar.

### 4. UI & Management Interface (`craft://downloads`)
- **Toolbar Popup Indicator:** A download button in the browser chrome showing overall progress, transfer speed, and remaining time.
- **Dedicated Page (`craft://downloads`):**
  - Searchable list of past and active downloads.
  - Controls: Open File, Show in Folder, Pause, Resume, Cancel, Copy Download Link, Clear History.
  - Per-item container badge indicator.

---

## Data Model

```rust
use std::path::PathBuf;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    Queued,
    Downloading { bytes_received: u64, total_bytes: Option<u64> },
    Paused { bytes_received: u64, total_bytes: Option<u64> },
    Completed,
    Failed { error_message: String },
    Cancelled,
}

pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub target_path: PathBuf,
    pub temporary_path: PathBuf,
    pub filename: String,
    pub mime_type: String,
    pub total_bytes: Option<u64>,
    pub bytes_downloaded: u64,
    pub state: DownloadState,
    pub container_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

---

## Persistence & Storage

Downloads are persisted to the local SQLite database (`craft-storage`):
- Table `downloads` stores metadata, download status, container ID, and file paths.
- Active incomplete downloads can be resumed across browser restarts if the remote server supports `Range` requests.
