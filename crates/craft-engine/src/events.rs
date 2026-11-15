//! Engine events reported to the browser chrome.
//!
//! These are observations of real page state, not predictions: the control plane
//! emits them by comparing successive [`PageSnapshot`]s.

use craft_core::tab::TabId;

/// How far a load has gotten, as the engine reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageLoadState {
    /// Nothing has been loaded in this tab yet.
    Initial,
    /// A load was requested and has not produced a document.
    Started,
    /// A document is loading (head parsed, subresources pending).
    Loading,
    /// The load finished.
    Completed,
    /// The load was aborted by the user or by `window.stop()`.
    Stopped,
    /// The load could not be completed.
    Failed,
}

impl PageLoadState {
    /// A load the engine has not finished reporting on.
    pub fn is_loading(&self) -> bool {
        matches!(self, PageLoadState::Started | PageLoadState::Loading)
    }
}

/// A change in a tab's page: where it is, and how far the load got.
///
/// There is no status code here on purpose: the engine boundary does not expose
/// HTTP status for a navigation, and inventing one (as the earlier simulated
/// engine did) would report success for loads that failed. Failures arrive as
/// [`PageLoadState::Failed`].
#[derive(Debug, Clone)]
pub struct NavigationEvent {
    pub tab_id: TabId,
    pub state: PageLoadState,
    pub url: String,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Navigation(NavigationEvent),
}
