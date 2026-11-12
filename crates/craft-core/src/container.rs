use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContainerId(pub String);

impl ContainerId {
    pub const DEFAULT: &'static str = "default";
    pub const PERSONAL: &'static str = "personal";
    pub const WORK: &'static str = "work";
    pub const BANKING: &'static str = "banking";
    pub const SHOPPING: &'static str = "shopping";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn default_container() -> Self {
        Self(Self::DEFAULT.to_string())
    }
}

/// A Firefox-style contextual identity partitioning cookies, cache, and storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id: ContainerId,
    pub name: String,
    pub icon: String,
    pub color_hex: String,
    pub user_context_id: u32,
    pub assigned_domains: Vec<String>,
}

impl Container {
    pub fn new(id: ContainerId, name: impl Into<String>, icon: impl Into<String>, color_hex: impl Into<String>, user_context_id: u32) -> Self {
        Self {
            id,
            name: name.into(),
            icon: icon.into(),
            color_hex: color_hex.into(),
            user_context_id,
            assigned_domains: Vec::new(),
        }
    }
}
