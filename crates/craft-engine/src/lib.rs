//! Engine control plane for CraftBrowsing: the [`Engine`] boundary plus
//! [`TabEngine`], the backend-agnostic tab records the browser chrome drives.
//!
//! There is no simulation in this crate. Every page operation is delegated to a
//! host-implemented [`PageController`], and every reported state comes from
//! comparing what the controller says about its page against what was reported
//! last time. The earlier `ServoEngine` here was a worker thread that answered
//! its own commands with fabricated `Completed`/200 navigation events; it has
//! been removed, because a chrome driven by invented engine events cannot tell a
//! real load from a failed one.
//!
//! Painting and compositing are not here either. They live in
//! `craft-ui::window` until Phase 2b; see `engine.rs` and docs/engine-decision.md.

pub mod compositor;
pub mod engine;
pub mod events;
pub mod pipeline;
#[cfg(feature = "servo")]
pub mod servo_backend;

use std::collections::{HashMap, VecDeque};

use craft_core::container::ContainerId;
use craft_core::tab::TabId;
use tracing::info;

pub use engine::{Backend, Engine, PageController, PageSnapshot};
pub use events::{EngineEvent, NavigationEvent, PageLoadState};
#[cfg(feature = "servo")]
pub use servo_backend::{PageSlot, ServoPageController};

/// A tab as the control plane sees it: its container partition and the last page
/// state the backend reported for it.
#[derive(Debug, Clone)]
pub struct TabRecord {
    pub tab_id: TabId,
    pub container_id: ContainerId,
    pub snapshot: PageSnapshot,
}

/// Backend-agnostic engine handle. It owns the tab records and the event queue,
/// delegates all page operations to `C`, and turns real page-state changes into
/// [`EngineEvent`]s.
///
/// `live` is which tab the controller's single current page belongs to. Page
/// operations move it, because with one shared surface (the Servo situation
/// until Phase 2b) navigating a tab *is* changing what the window shows.
pub struct TabEngine<C: PageController> {
    backend: Backend,
    controller: C,
    tabs: HashMap<TabId, TabRecord>,
    live: Option<TabId>,
    pending: VecDeque<EngineEvent>,
}

impl<C: PageController> TabEngine<C> {
    pub fn new(backend: Backend, controller: C) -> Self {
        info!(
            "Engine control plane initialised (backend: {})",
            backend.as_str()
        );
        Self {
            backend,
            controller,
            tabs: HashMap::new(),
            live: None,
            pending: VecDeque::new(),
        }
    }

    /// Number of tabs the control plane is tracking.
    pub fn tracked_tabs(&self) -> usize {
        self.tabs.len()
    }

    /// Which tab owns the live page, if any.
    pub fn live_tab(&self) -> Option<&TabId> {
        self.live.as_ref()
    }

    /// Mark `tab_id` as owning the live page and note that a load began. The URL
    /// in the event is what we currently know about the page, never a guess about
    /// where the engine will end up.
    fn begin_load(&mut self, tab_id: &TabId, url: Option<&str>) {
        self.live = Some(tab_id.clone());
        let url = match url {
            Some(url) => {
                if let Some(record) = self.tabs.get_mut(tab_id) {
                    record.snapshot.url = url.to_string();
                }
                url.to_string()
            }
            None => self
                .tabs
                .get(tab_id)
                .map(|record| record.snapshot.url.clone())
                .unwrap_or_default(),
        };
        if let Some(record) = self.tabs.get_mut(tab_id) {
            record.snapshot.load_state = PageLoadState::Started;
        }
        self.pending
            .push_back(EngineEvent::Navigation(NavigationEvent {
                tab_id: tab_id.clone(),
                state: PageLoadState::Started,
                url,
            }));
    }

    /// Pull the live page's own state in and emit an event when it moved, so a
    /// redirect or a finished load reaches the chrome instead of being assumed.
    fn refresh_from_page(&mut self) {
        let Some(live) = self.live.clone() else {
            return;
        };
        let Some(snapshot) = self.controller.snapshot() else {
            return;
        };
        let Some(record) = self.tabs.get_mut(&live) else {
            return;
        };
        let changed = record.snapshot.url != snapshot.url
            || record.snapshot.load_state != snapshot.load_state;
        if changed {
            self.pending
                .push_back(EngineEvent::Navigation(NavigationEvent {
                    tab_id: live,
                    state: snapshot.load_state.clone(),
                    url: snapshot.url.clone(),
                }));
        }
        // Title and history availability are kept fresh either way: the chrome
        // reads them from `page_snapshot` and must not see a stale value just
        // because the URL and load state did not move.
        record.snapshot = snapshot;
    }
}

impl<C: PageController> Engine for TabEngine<C> {
    fn backend(&self) -> Backend {
        self.backend
    }

    fn create_tab(&mut self, tab_id: &TabId, container_id: &ContainerId, url: &str) {
        self.tabs.insert(
            tab_id.clone(),
            TabRecord {
                tab_id: tab_id.clone(),
                container_id: container_id.clone(),
                snapshot: PageSnapshot::initial(url),
            },
        );
    }

    fn close_tab(&mut self, tab_id: &TabId) {
        self.tabs.remove(tab_id);
        if self.live.as_ref() == Some(tab_id) {
            // The host still shows the last page until something else loads;
            // there is no page left to report for this tab.
            self.live = None;
        }
    }

    fn set_live_page(&mut self, tab_id: &TabId) {
        if self.tabs.contains_key(tab_id) {
            self.live = Some(tab_id.clone());
        }
    }

    fn navigate(&mut self, tab_id: &TabId, url: &str) {
        self.controller.load_url(url);
        self.begin_load(tab_id, Some(url));
    }

    fn reload(&mut self, tab_id: &TabId, bypass_cache: bool) {
        self.controller.reload(bypass_cache);
        self.begin_load(tab_id, None);
    }

    fn stop(&mut self, tab_id: &TabId) {
        // Only the tab that owns the live page can be stopped, and no event is
        // emitted here: the page reports its own stopped state on the next poll.
        // That is the difference between this and the fake engine it replaced.
        if self.live.as_ref() == Some(tab_id) {
            self.controller.stop();
        }
    }

    fn go_back(&mut self, tab_id: &TabId) {
        self.controller.go_back();
        self.begin_load(tab_id, None);
    }

    fn go_forward(&mut self, tab_id: &TabId) {
        self.controller.go_forward();
        self.begin_load(tab_id, None);
    }

    fn poll_events(&mut self) -> Vec<EngineEvent> {
        self.refresh_from_page();
        self.pending.drain(..).collect()
    }

    fn page_snapshot(&self, tab_id: &TabId) -> Option<PageSnapshot> {
        self.tabs.get(tab_id).map(|record| record.snapshot.clone())
    }
}
