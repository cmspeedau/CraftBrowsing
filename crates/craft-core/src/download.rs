use serde::{Deserialize, Serialize};
use crate::container::ContainerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Queued,
    Downloading { bytes_received: u64, total_bytes: Option<u64> },
    Paused { bytes_received: u64, total_bytes: Option<u64> },
    Completed { target_path: String },
    Failed { reason: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub source_url: String,
    pub suggested_filename: String,
    pub container_id: ContainerId,
    pub state: DownloadState,
    pub part_filepath: String,
    pub destination_path: String,
    pub supports_range: bool,
    pub created_at: u64,
}
