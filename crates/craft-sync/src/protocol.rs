use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPreferences {
    pub sync_enabled: bool,
    pub sync_bookmarks: bool,
    pub sync_history: bool,
    pub sync_open_tabs: bool,
    pub sync_containers: bool,
    pub sync_block_lists: bool,
    pub sync_pwa_apps: bool,
}

impl Default for SyncPreferences {
    fn default() -> Self {
        Self {
            sync_enabled: false,
            sync_bookmarks: false,
            sync_history: false,
            sync_open_tabs: false,
            sync_containers: false,
            sync_block_lists: false,
            sync_pwa_apps: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedTab {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub container_id: Option<String>,
    pub pinned: bool,
    pub group_id: Option<String>,
    pub parent_tab_id: Option<String>,
    pub scroll_y: Option<u32>,
    pub last_accessed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedWindow {
    pub window_id: String,
    pub tabs: Vec<SyncedTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTabSession {
    pub device_id: String,
    pub device_name: String,
    pub updated_at: u64,
    pub windows: Vec<SyncedWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTabPayload {
    pub target_device_id: String,
    pub url: String,
    pub title: String,
    pub container_id: Option<String>,
    pub auto_open: bool,
    pub dispatched_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockListSyncPayload {
    pub subscribed_lists: Vec<String>,
    pub custom_rules: Vec<String>,
    pub site_exceptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwaAppSyncPayload {
    pub app_id: String,
    pub manifest_url: String,
    pub start_url: String,
    pub name: String,
    pub short_name: Option<String>,
    pub scope: String,
    pub display_mode: String,
    pub theme_color: Option<String>,
    pub container_id: Option<String>,
    pub updated_at: u64,
}
