//! Engine events emitted by the Servo engine bridge.

use craft_core::tab::TabId;

#[derive(Debug, Clone)]
pub enum PageLoadState {
    Initial,
    Started,
    Loading,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct NavigationEvent {
    pub tab_id: TabId,
    pub state: PageLoadState,
    pub url: String,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Navigation(NavigationEvent),
}
