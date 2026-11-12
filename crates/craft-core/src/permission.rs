use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionKind {
    WebSerial,
    WebUsb,
    WebHid,
    Camera,
    Microphone,
    Geolocation,
    Notifications,
    ClipboardRead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionState {
    Prompt,
    Granted,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PermissionScope {
    pub origin: String,
    pub container_id: String,
    pub permission: PermissionKind,
}
