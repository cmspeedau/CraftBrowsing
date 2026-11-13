//! # craft-transfers
//!
//! Unified transfer engine for CraftBrowsing.
//!
//! v0.5.0 introduces segmented HTTP(S) downloads and magnet/torrent
//! downloads behind one container-aware manager. The transfer algorithms are
//! vendored and adapted from `danted4/torrenter` (MIT-licensed); the
//! torrenter MCP server is intentionally not included.

pub mod paths;
pub mod validate;

pub use paths::{default_download_dir, ensure_dir, persistable_dir, resolve_user_dir};
pub use validate::{is_valid_download_url, is_valid_magnet_url};

// Additional modules will be added as the engine is wired in:
// pub mod progress;
// pub mod http;
// pub mod torrent;
// pub mod jobs;
