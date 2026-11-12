//! Core types and architecture definitions for CraftBrowsing.

pub mod container;
pub mod download;
pub mod permission;
pub mod storage;
pub mod tab;

pub use container::{Container, ContainerId};
pub use download::{DownloadItem, DownloadState};
pub use permission::{PermissionKind, PermissionScope, PermissionState};
pub use tab::{Tab, TabGroup, TabId, TabState};
