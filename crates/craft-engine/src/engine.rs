//! The engine boundary. `craft-ui` programs against [`Engine`], never a concrete
//! engine, so Servo (today) and Blink/Chromium-CEF (planned) are interchangeable
//! backends. See docs/engine-decision.md.
//!
//! Scope of this module: the engine *control plane*. Two traits, deliberately
//! split by who implements them:
//!
//! - [`Engine`] is implemented by [`crate::TabEngine`], which owns the
//!   backend-agnostic tab records and the event stream. The chrome holds it as
//!   `Box<dyn Engine>`.
//! - [`PageController`] is implemented by a backend's page handle. For Servo that
//!   is `craft_ui::page::ServoPageController` over a `servo::WebView`; a Blink
//!   backend implements the same trait over a CEF browser.
//!
//! This split exists because page painting and compositing are still owned by
//! `craft-ui::window`: they are coupled to the winit window, the egui/surfman GL
//! context and the content-rect blit. Lifting them behind this boundary is
//! Phase 2b and must be verified with real screenshots (a blank-but-loaded page
//! is indistinguishable from a correct one in logs), so it is deferred rather
//! than done blind. Until then [`PageController`] addresses the one live page,
//! and per-tab rendering contexts are created by the host, not by this crate.

use craft_core::container::ContainerId;
use craft_core::tab::TabId;

use crate::events::{EngineEvent, PageLoadState};

/// Which rendering engine backs an [`Engine`] implementation. Fixed at runtime
/// for now; the point of the enum is that the chrome never branches on it, it
/// only reads it for display/diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Servo,
    /// Planned. Not implemented; see docs/engine-decision.md.
    Blink,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Servo => "servo",
            Backend::Blink => "blink",
        }
    }
}

/// A backend-agnostic snapshot of a page, so the chrome can render URL, title,
/// load state and history availability without knowing the engine. Every field
/// must come from the engine's own view of the page; nothing here is inferred.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub load_state: PageLoadState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl PageSnapshot {
    /// A tab whose page has not reported anything yet.
    pub fn initial(url: &str) -> Self {
        Self {
            url: url.to_string(),
            title: String::new(),
            load_state: PageLoadState::Initial,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

/// The page operations a backend must perform. Implemented by the host, on the
/// UI thread: engine page handles wrap non-`Send` state (`Rc`/context), so this
/// trait is neither `Send` nor `Sync` by design.
///
/// The receiver drives the page it currently shows, which today means one shared
/// page for the whole window (per-tab rendering contexts are Phase 2b). [`Engine`]
/// is what decides *which tab* a reported state belongs to, so this trait
/// deliberately has no `TabId` to misattribute against.
pub trait PageController {
    /// Navigate the live page. A URL the backend cannot parse or load must be
    /// reported through [`Engine::poll_events`] as [`PageLoadState::Failed`],
    /// not swallowed silently.
    fn load_url(&mut self, url: &str);

    /// Reload the live page. `bypass_cache` is best effort: Servo 0.8 exposes no
    /// cache-ignoring reload, so an implementation that cannot honor it reloads
    /// normally rather than pretending otherwise.
    fn reload(&mut self, bypass_cache: bool);

    /// Abort the in-flight load of the live page.
    fn stop(&mut self);

    fn go_back(&mut self);
    fn go_forward(&mut self);

    /// The live page's own view of itself, or `None` when there is no page.
    fn snapshot(&self) -> Option<PageSnapshot>;
}

/// The engine control plane the browser chrome depends on.
///
/// Object-safe (used as `&mut dyn Engine`) and intentionally not `Send`.
pub trait Engine {
    /// Which engine this is, for display only.
    fn backend(&self) -> Backend;

    /// Register a tab in the control plane. This does not load anything: page
    /// creation is the host's job until per-tab surfaces exist (Phase 2b).
    fn create_tab(&mut self, tab_id: &TabId, container_id: &ContainerId, url: &str);

    /// Forget a tab's page record. The tab's page content is released by the
    /// host when it destroys the surface.
    fn close_tab(&mut self, tab_id: &TabId);

    /// Adopt the controller's live page as this tab's page. The host calls this
    /// for the page it created itself at startup; afterwards page operations on
    /// a tab move the live page to that tab.
    fn set_live_page(&mut self, tab_id: &TabId);

    fn navigate(&mut self, tab_id: &TabId, url: &str);
    fn reload(&mut self, tab_id: &TabId, bypass_cache: bool);
    fn stop(&mut self, tab_id: &TabId);
    fn go_back(&mut self, tab_id: &TabId);
    fn go_forward(&mut self, tab_id: &TabId);

    /// Drain pending engine events. Events are produced by comparing the live
    /// page's [`PageSnapshot`] against what was last reported, so a page that
    /// changes underneath us (a redirect, a title, a finished load) surfaces
    /// here instead of being assumed.
    fn poll_events(&mut self) -> Vec<EngineEvent>;

    /// Current page record for a tab, if it is tracked.
    fn page_snapshot(&self, tab_id: &TabId) -> Option<PageSnapshot>;
}
