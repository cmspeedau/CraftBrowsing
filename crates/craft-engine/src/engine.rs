//! The engine boundary. `craft-ui` programs against [`Engine`], never a concrete
//! engine, so Servo (today) and Blink/Chromium-CEF (planned) are interchangeable
//! backends. See docs/engine-decision.md.
//!
//! Scope of this trait: the engine *control plane* (tab lifecycle, navigation,
//! per-tab state, and an event stream). Page painting and compositing are
//! intentionally NOT here yet. The real paint path currently lives in
//! `craft-ui::window` because it is tightly coupled to the winit window, the
//! egui/surfman GL context, and the content-rect blit. Lifting it behind this
//! trait is Phase 2 and must be verified visually (a blank-but-loaded page looks
//! identical to a good page in logs), so it is deliberately deferred rather than
//! done blind here.

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

/// A backend-agnostic snapshot of a tab's page, so the chrome can render URL,
/// title, load state, and history availability without knowing the engine.
#[derive(Debug, Clone)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub load_state: PageLoadState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

/// The engine control plane the browser chrome depends on.
///
/// Object-safe (used as `&mut dyn Engine`) and intentionally not `Send`: engine
/// handles wrap non-`Send` engine state (`Rc`/context) and live on the UI thread,
/// per the Servo embedder contract.
pub trait Engine {
    /// Which engine this is, for display only.
    fn backend(&self) -> Backend;

    /// Create a browsing context for a tab at the given URL.
    fn create_tab(&mut self, tab_id: &TabId, container_id: &ContainerId, url: &str);

    /// Destroy the browsing context for a tab.
    fn close_tab(&mut self, tab_id: &TabId);

    fn navigate(&mut self, tab_id: &TabId, url: &str);
    fn reload(&mut self, tab_id: &TabId, bypass_cache: bool);
    fn stop(&mut self, tab_id: &TabId);
    fn go_back(&mut self, tab_id: &TabId);
    fn go_forward(&mut self, tab_id: &TabId);

    /// Resize a tab's viewport in device pixels.
    fn resize(&mut self, tab_id: &TabId, width: u32, height: u32);

    /// Update a tab's HiDPI scale factor (device pixels per CSS pixel).
    fn set_hidpi_scale_factor(&mut self, tab_id: &TabId, scale: f32);

    /// Mark which tab is focused (input routing).
    fn set_active_tab(&mut self, tab_id: &TabId);

    /// Drain pending engine events (navigation/title/load/crash/frame-ready).
    fn poll_events(&mut self) -> Vec<EngineEvent>;

    /// Current page snapshot for a tab, if it exists.
    fn page_snapshot(&self, tab_id: &TabId) -> Option<PageSnapshot>;
}
