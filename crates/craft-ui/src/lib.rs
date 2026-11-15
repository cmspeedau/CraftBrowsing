//! Browser shell UI and main application entrypoint for CraftBrowsing.

pub mod omnibox;
pub mod page;
pub mod tab_manager;
pub mod window;

use craft_adblock::AdblockManager;
use craft_core::container::{Container, ContainerId};
use craft_core::media::{MediaAction, MediaHub, MediaSessionEvent, MediaSessionId};
use craft_core::tab::{Tab, TabGroup, TabId};
use craft_engine::{Backend, Engine, EngineEvent, TabEngine};
use craft_sync::protocol::SyncPreferences;
use std::sync::Arc;
use tracing::info;

pub use omnibox::{OmniboxState, OmniboxTarget};
pub use page::{PageSlot, ServoPageController};
pub use tab_manager::{TabNode, TabState, VerticalTabManager};
pub use window::{BrowserWindowApp, run_window_event_loop};

pub struct BrowserApp {
    pub adblock: Arc<AdblockManager>,
    pub sync_preferences: SyncPreferences,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<TabId>,
    pub groups: Vec<TabGroup>,
    pub containers: Vec<Container>,
    /// The engine boundary. The chrome holds a trait object so nothing here
    /// names Servo; the backend-specific page handle lives in `craft_ui::page`.
    pub engine: Box<dyn Engine>,
    /// Published to by `window.rs` once it has built the real page.
    page_slot: PageSlot,
    pub omnibox: OmniboxState,
    pub tab_manager: VerticalTabManager,
    pub media_hub: MediaHub,
}

impl BrowserApp {
    pub fn new() -> Self {
        info!("Initializing CraftBrowsing browser shell with Servo engine integration...");

        // AdblockManager is not proven Send+Sync by clippy, but it is only
        // shared behind an Arc within a single process and never crosses a
        // thread boundary in the current shell.
        #[allow(clippy::arc_with_non_send_sync)]
        let adblock = Arc::new(AdblockManager::new());
        let sync_preferences = SyncPreferences::default();
        let omnibox = OmniboxState::new("", ContainerId::default_container());
        let tab_manager = VerticalTabManager::new();
        let media_hub = MediaHub::new();

        // Seed Firefox-style Multi-Account Container identities so the tab
        // accent bars and container menu have real backing data (color comes
        // from Container.color_hex, not decoration).
        let containers = vec![
            Container::new(
                ContainerId::default_container(),
                "Default",
                "🏠",
                "#8f8f9d",
                0,
            ),
            Container::new(
                ContainerId::new(ContainerId::PERSONAL),
                "Personal",
                "👤",
                "#0053e6",
                1,
            ),
            Container::new(
                ContainerId::new(ContainerId::WORK),
                "Work",
                "💼",
                "#2ac3a2",
                2,
            ),
            Container::new(
                ContainerId::new(ContainerId::BANKING),
                "Banking",
                "🏦",
                "#8a2be2",
                3,
            ),
            Container::new(
                ContainerId::new(ContainerId::SHOPPING),
                "Shopping",
                "🛒",
                "#ff5f00",
                4,
            ),
        ];

        // The chrome owns the control plane; the window hands it a page to drive
        // once the winit/GL surface exists (see `PageSlot::attach`).
        let page_slot = PageSlot::default();
        let engine: Box<dyn Engine> = Box::new(TabEngine::new(
            Backend::Servo,
            ServoPageController::new(page_slot.clone()),
        ));

        Self {
            adblock,
            sync_preferences,
            tabs: Vec::new(),
            active_tab_id: None,
            groups: Vec::new(),
            containers,
            engine,
            page_slot,
            omnibox,
            tab_manager,
            media_hub,
        }
    }

    /// The handle the window publishes its live page into.
    pub fn page_slot(&self) -> &PageSlot {
        &self.page_slot
    }

    /// Look up a container's display color (hex) and icon by id. Falls back to
    /// the default identity so a tab always has real data to render.
    pub fn container_identity(&self, id: &ContainerId) -> (String, String, String) {
        let c = self
            .containers
            .iter()
            .find(|c| &c.id == id)
            .or_else(|| self.containers.first());
        match c {
            Some(c) => (c.name.clone(), c.color_hex.clone(), c.icon.clone()),
            None => (
                "Default".to_string(),
                "#8f8f9d".to_string(),
                "🏠".to_string(),
            ),
        }
    }

    // -- Container and privacy identity helpers ---------------------------

    pub fn adblock_enabled(&self) -> bool {
        self.adblock.is_enabled()
    }

    pub fn set_adblock_enabled(&mut self, enabled: bool) {
        self.adblock.set_enabled(enabled);
    }

    pub fn open_tab(&mut self, url: &str, container_id: ContainerId) -> TabId {
        let tab_id = TabId(uuid_v4_stub());
        let tab = Tab::new(tab_id.clone(), url, container_id.clone());
        self.tabs.push(tab);

        // Register in tree-style VerticalTabManager
        let parent_id = self.active_tab_id.clone();
        self.tab_manager
            .insert_tab(tab_id.clone(), parent_id, container_id.clone(), url);

        // Register the tab in the engine control plane. No page is created for
        // it yet: per-tab rendering surfaces are Phase 2b, and the window owns
        // surface creation until then.
        self.engine.create_tab(&tab_id, &container_id, url);
        self.active_tab_id = Some(tab_id.clone());
        self.tab_manager.active_tab_id = Some(tab_id.clone());
        self.omnibox.set_text(url);
        tab_id
    }

    pub fn navigate_tab(&mut self, tab_id: &TabId, url: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| &t.id == tab_id) {
            tab.navigate(url);
        }
        self.tab_manager.update_tab_url(tab_id, url);
        self.engine.navigate(tab_id, url);
        if self.active_tab_id.as_ref() == Some(tab_id) {
            self.omnibox.set_text(url);
        }
    }

    /// Submit omnibox input: parse URL or search query and navigate active tab.
    pub fn submit_omnibox(&mut self, input: &str) {
        let action = OmniboxState::resolve_input(input);
        let target_url = match action {
            OmniboxTarget::Url(url) => url,
            OmniboxTarget::Search { resolved_url, .. } => resolved_url,
            OmniboxTarget::Internal(page) => page,
        };

        if let Some(active_id) = self.active_tab_id.clone() {
            self.navigate_tab(&active_id, &target_url);
        } else {
            self.open_tab(&target_url, ContainerId::default_container());
        }
    }

    pub fn select_tab(&mut self, tab_id: &TabId) {
        if let Some(tab) = self.tabs.iter().find(|t| &t.id == tab_id) {
            self.omnibox.set_text(&tab.url);
            self.active_tab_id = Some(tab_id.clone());
            self.tab_manager.active_tab_id = Some(tab_id.clone());

            // If tab was hibernated, wake it up in the rendering engine
            let wake_info = self.tab_manager.get_node(tab_id).and_then(|node| {
                if node.state == TabState::Hibernated {
                    Some((node.container_id.clone(), node.url.clone()))
                } else {
                    None
                }
            });

            // Waking a tab re-registers it with the control plane: its page
            // record starts clean again, because a hibernated tab's content is
            // gone and per-tab surfaces do not exist yet.
            if let Some((container_id, url)) = wake_info {
                self.tab_manager.wake_tab(tab_id);
                self.engine.create_tab(tab_id, &container_id, &url);
            }
        }
    }

    /// Discard a background tab's page. This forgets the tab's engine record;
    /// the shared page itself is only released if this tab owned it, because
    /// per-tab surfaces arrive with Phase 2b.
    pub fn hibernate_tab(&mut self, tab_id: &TabId) {
        if self.active_tab_id.as_ref() == Some(tab_id) {
            return; // Never hibernate active tab
        }
        self.tab_manager.hibernate_tab(tab_id);
        self.engine.close_tab(tab_id);
    }

    pub fn close_tab(&mut self, tab_id: &TabId) {
        self.engine.close_tab(tab_id);
        self.tabs.retain(|t| &t.id != tab_id);
        self.tab_manager.remove_tab(tab_id);

        if self.active_tab_id.as_ref() == Some(tab_id) {
            self.active_tab_id = self.tabs.last().map(|t| t.id.clone());
            self.tab_manager.active_tab_id = self.active_tab_id.clone();
            if let Some(active_id) = &self.active_tab_id {
                if let Some(tab) = self.tabs.iter().find(|t| &t.id == active_id) {
                    self.omnibox.set_text(&tab.url);
                }
            } else {
                self.omnibox.set_text("");
            }
        }
    }

    /// Drain engine events and apply them to the chrome.
    ///
    /// These arrive from the engine's own view of its page (a redirect, a
    /// finished load, a new title), so page truth flows one way: engine to
    /// chrome. Nothing here assumes a load succeeded.
    pub fn poll_engine_events(&mut self) -> Vec<EngineEvent> {
        let events = self.engine.poll_events();
        for event in &events {
            let EngineEvent::Navigation(nav) = event;
            self.tab_manager.update_tab_url(&nav.tab_id, &nav.url);
            if let Some(snapshot) = self.engine.page_snapshot(&nav.tab_id) {
                if !snapshot.title.is_empty() {
                    self.tab_manager
                        .update_tab_title(&nav.tab_id, &snapshot.title);
                }
                if self.active_tab_id.as_ref() == Some(&nav.tab_id) {
                    self.omnibox.set_text(&snapshot.url);
                }
            }
        }
        events
    }

    // -- Global Media Hub -------------------------------------------------

    /// Ingest a media session event reported by the engine.
    pub fn ingest_media_event(&mut self, event: MediaSessionEvent) {
        self.media_hub.apply_event(event);
    }

    /// Dispatch a media action. In the current (simulated) engine this records
    /// the intent and updates local state; once Servo is wired, this is where
    /// the action is forwarded to the owning webview.
    pub fn dispatch_media_action(
        &mut self,
        session_id: Option<MediaSessionId>,
        action: MediaAction,
    ) {
        let target = session_id.or_else(|| self.media_hub.active_session_id());
        let Some(target) = target else {
            tracing::debug!("media action {:?} with no active session", action);
            return;
        };

        match action {
            MediaAction::Play => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.state = craft_core::media::MediaPlaybackState::Playing;
                }
                tracing::info!("media: play {}", target.0);
            }
            MediaAction::Pause => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.state = craft_core::media::MediaPlaybackState::Paused;
                }
                tracing::info!("media: pause {}", target.0);
            }
            MediaAction::TogglePlayPause => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.state = match s.state {
                        craft_core::media::MediaPlaybackState::Playing => {
                            craft_core::media::MediaPlaybackState::Paused
                        }
                        _ => craft_core::media::MediaPlaybackState::Playing,
                    };
                }
                tracing::info!("media: toggle {}", target.0);
            }
            MediaAction::Stop => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.state = craft_core::media::MediaPlaybackState::Ended;
                    s.position_ms = 0;
                }
                tracing::info!("media: stop {}", target.0);
            }
            MediaAction::SeekRelative(delta_ms) => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    let new_pos = if delta_ms >= 0 {
                        s.position_ms.saturating_add(delta_ms as u64)
                    } else {
                        s.position_ms.saturating_sub((-delta_ms) as u64)
                    };
                    s.position_ms = match s.duration_ms {
                        Some(d) => new_pos.min(d),
                        None => new_pos,
                    };
                }
                tracing::info!("media: seek {:+}ms {}", delta_ms, target.0);
            }
            MediaAction::SeekTo(pos_ms) => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.position_ms = match s.duration_ms {
                        Some(d) => pos_ms.min(d),
                        None => pos_ms,
                    };
                }
                tracing::info!("media: seek to {}ms {}", pos_ms, target.0);
            }
            MediaAction::SkipForward(ms) => {
                self.dispatch_media_action(
                    Some(target.clone()),
                    MediaAction::SeekRelative(ms as i64),
                );
            }
            MediaAction::SkipBackward(ms) => {
                self.dispatch_media_action(
                    Some(target.clone()),
                    MediaAction::SeekRelative(-(ms as i64)),
                );
            }
            MediaAction::SetVolume(v) => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.volume = v.clamp(0.0, 1.0);
                }
                tracing::info!("media: volume {} {}", v, target.0);
            }
            MediaAction::SetMuted(muted) => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.muted = muted;
                }
                tracing::info!("media: mute {} {}", muted, target.0);
            }
            MediaAction::ToggleMute => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.muted = !s.muted;
                }
                tracing::info!("media: toggle mute {}", target.0);
            }
            MediaAction::SetRate(rate) => {
                if let Some(s) = self.media_hub.get_mut(&target) {
                    s.playback_rate = rate;
                }
                tracing::info!("media: rate {} {}", rate, target.0);
            }
            MediaAction::EnterPip => {
                self.media_hub.apply_event(MediaSessionEvent::PipChanged {
                    id: target.clone(),
                    active: true,
                });
                tracing::info!("media: enter pip {}", target.0);
            }
            MediaAction::ExitPip => {
                self.media_hub.apply_event(MediaSessionEvent::PipChanged {
                    id: target.clone(),
                    active: false,
                });
                tracing::info!("media: exit pip {}", target.0);
            }
            MediaAction::NextTrack | MediaAction::PreviousTrack => {
                tracing::info!("media: track navigation not yet wired ({:?})", action);
            }
        }
    }

    /// Title string for the window reflecting the active media session.
    pub fn media_window_title(&self) -> Option<String> {
        let session = self.media_hub.active_session()?;
        if session.is_playing() {
            Some(format!(
                "CraftBrowsing - {} (playing)",
                session.display_title()
            ))
        } else {
            Some(format!("CraftBrowsing - {}", session.display_title()))
        }
    }
}

impl Default for BrowserApp {
    fn default() -> Self {
        Self::new()
    }
}

fn uuid_v4_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tab-{:x}", nanos)
}
