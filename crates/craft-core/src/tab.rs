use crate::container::ContainerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabGroupId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    Active,
    Background,
    Hibernated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: TabGroupId,
    pub name: String,
    pub color_hex: String,
    pub bound_container: Option<ContainerId>,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub container_id: ContainerId,
    pub state: TabState,
    pub pinned: bool,
    pub group_id: Option<TabGroupId>,
    pub parent_tab_id: Option<TabId>,
    pub scroll_y: Option<u32>,
    pub last_active_epoch_ms: u64,
}

impl Tab {
    pub fn new(id: TabId, url: impl Into<String>, container_id: ContainerId) -> Self {
        Self {
            id,
            url: url.into(),
            title: "New Tab".to_string(),
            container_id,
            state: TabState::Active,
            pinned: false,
            group_id: None,
            parent_tab_id: None,
            scroll_y: None,
            last_active_epoch_ms: 0,
        }
    }

    pub fn hibernate(&mut self) {
        self.state = TabState::Hibernated;
    }

    pub fn wake(&mut self) {
        self.state = TabState::Active;
    }
}
