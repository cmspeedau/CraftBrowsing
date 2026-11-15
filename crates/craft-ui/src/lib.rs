//! Browser shell UI and main application entrypoint for CraftBrowsing.

pub mod lists;
pub mod omnibox;
pub mod shields;
pub mod tab_manager;
pub mod window;

use craft_adblock::AdblockManager;
use craft_adblock::upstream::{DEFAULT_SOURCES, ListStore, SourceAction};
use craft_core::container::{Container, ContainerId};
use craft_core::media::{MediaAction, MediaHub, MediaSessionEvent, MediaSessionId};
use craft_core::tab::{Tab, TabGroup, TabId};
use craft_engine::{Backend, Engine, EngineEvent, PageSlot, ServoPageController, TabEngine};
use craft_sync::protocol::SyncPreferences;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use tracing::{info, warn};

use crate::lists::UpdateEvent;

pub use omnibox::{OmniboxState, OmniboxTarget};
pub use shields::ShieldStats;
pub use tab_manager::{TabNode, TabState, VerticalTabManager};
pub use window::{BrowserWindowApp, run_window_event_loop};

pub struct BrowserApp {
    pub adblock: Arc<AdblockManager>,
    /// Session counters for refused requests: the engine's request hook writes,
    /// the chrome reads.
    pub shields: Arc<ShieldStats>,
    /// On-disk cache for upstream filter lists.
    list_store: ListStore,
    /// Receiver for the in-flight update pass, if one is running.
    list_updates: Option<Receiver<UpdateEvent>>,
    /// Sources that finished but are not compiled yet: the engine is rebuilt
    /// once at the end of a pass rather than once per list.
    pending_lists: Vec<SourceAction>,
    /// Outcome lines from the most recent update pass, newest last.
    list_log: Vec<String>,
    pub sync_preferences: SyncPreferences,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<TabId>,
    pub groups: Vec<TabGroup>,
    pub containers: Vec<Container>,
    /// The engine boundary. The chrome holds a trait object so nothing here
    /// names Servo; the backend page handle lives in `craft_engine::servo_backend`.
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
        let shields = ShieldStats::new();

        // Lists downloaded earlier are read back from disk here, with no network
        // traffic, so protection does not depend on the user pressing the button
        // again every session.
        let list_store = ListStore::new(filter_list_dir());
        install_cached_lists(&adblock, &list_store);
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
            shields,
            list_store,
            list_updates: None,
            pending_lists: Vec::new(),
            list_log: Vec::new(),
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

    /// How many rules the matcher has compiled. Shown in the menu so the shield
    /// is not a claim without a number behind it.
    pub fn filter_rule_count(&self) -> usize {
        self.adblock.rule_count()
    }

    /// Requests refused by a filter rule during this session.
    pub fn blocked_requests(&self) -> u64 {
        self.shields.blocked()
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

    // -- Upstream filter lists -------------------------------------------------

    /// Start an update pass over the known upstream filter lists.
    ///
    /// This is the only path that fetches rules from the network. Nothing is
    /// fetched at startup: a default network call is forbidden by the privacy
    /// invariants, so lists arrive on this explicit user action and are read back
    /// from the on-disk cache on every later start.
    pub fn update_filter_lists(&mut self) {
        if self.list_updates.is_some() {
            self.push_list_log("already updating: one pass at a time".to_string());
            return;
        }
        match lists::spawn_update(self.list_store.clone()) {
            Some(receiver) => {
                self.list_updates = Some(receiver);
                self.push_list_log(format!(
                    "updating {} filter lists from upstream",
                    DEFAULT_SOURCES.len()
                ));
            }
            None => self.push_list_log("could not start the filter list updater".to_string()),
        }
    }

    /// Collect update events, and compile the new lists into the running matcher
    /// once the pass has finished. Call once per frame from the chrome.
    ///
    /// Compilation happens here, on the UI thread, because `adblock`'s engine is
    /// neither `Send` nor `Sync`. Batching every list in the pass into one rebuild
    /// costs about 0.3 s measured on 141k rule lines, instead of once per list.
    pub fn poll_list_updates(&mut self) {
        let Some(receiver) = self.list_updates.take() else {
            return;
        };

        let mut actions = Vec::new();
        let mut finished = None;
        while let Ok(event) = receiver.try_recv() {
            match event {
                UpdateEvent::Source(action) => {
                    self.push_list_log(describe_action(&action));
                    actions.push(action);
                }
                UpdateEvent::Finished {
                    updated,
                    unchanged,
                    failed,
                } => finished = Some((updated, unchanged, failed)),
            }
        }
        self.pending_lists.extend(actions);

        match finished {
            Some((updated, unchanged, failed)) => {
                let rules = self.apply_pending_lists();
                self.push_list_log(format!(
                    "pass finished: {updated} updated, {unchanged} unchanged, {failed} failed; {rules} rules compiled"
                ));
                self.list_updates = None;
            }
            None => self.list_updates = Some(receiver),
        }
    }

    /// The loaded rule groups (bundled rules plus any cached lists), for display.
    pub fn filter_list_status(&self) -> Vec<String> {
        self.adblock
            .groups()
            .iter()
            .map(|group| {
                format!(
                    "{}: {} rules, {}",
                    group.title,
                    group.accepted,
                    age_label(group.updated_unix)
                )
            })
            .collect()
    }

    /// Outcome lines from the most recent pass, newest last.
    pub fn filter_list_log(&self) -> &[String] {
        &self.list_log
    }

    pub fn lists_updating(&self) -> bool {
        self.list_updates.is_some()
    }

    /// Install every updated list from the finished pass in one rebuild, and
    /// return the total compiled rule count.
    fn apply_pending_lists(&mut self) -> usize {
        let pending = std::mem::take(&mut self.pending_lists);
        let updates: Vec<(&str, &str, String, Option<i64>)> = pending
            .iter()
            .filter_map(|action| match action {
                SourceAction::Updated {
                    source_id,
                    title,
                    text,
                    fetched_at_unix,
                } => Some((
                    source_id.as_str(),
                    title.as_str(),
                    text.clone(),
                    Some(*fetched_at_unix),
                )),
                _ => None,
            })
            .collect();

        if updates.is_empty() {
            return self.adblock.rule_count();
        }

        let reports = self.adblock.install_lists(updates);
        let rejected: usize = reports.iter().map(|report| report.rejected).sum();
        if rejected > 0 {
            self.push_list_log(format!("{rejected} rules refused by the parser this pass"));
        }
        self.adblock.rule_count()
    }

    fn push_list_log(&mut self, line: String) {
        info!("shields: {line}");
        self.list_log.push(line);
        const KEEP: usize = 8;
        if self.list_log.len() > KEEP {
            let excess = self.list_log.len() - KEEP;
            self.list_log.drain(..excess);
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

/// Where cached filter lists live: the platform data directory, with the temp
/// directory as a last resort. These files are a cache, not user state, so
/// losing them costs nothing but the next manual update.
/// Where this build keeps its downloaded filter lists, so the location is citable
/// from docs, tests and diagnostics.
pub fn filter_list_dir() -> PathBuf {
    let root = match dirs::data_dir() {
        Some(root) => root,
        None => {
            warn!(
                "shields: no platform data directory, caching filter lists in the temp directory"
            );
            std::env::temp_dir()
        }
    };
    root.join("craftbrowsing").join("filterlists")
}

/// Load every cached list into the manager in one rebuild.
fn install_cached_lists(adblock: &AdblockManager, store: &ListStore) {
    let cached = store.load_all(&DEFAULT_SOURCES);
    if cached.is_empty() {
        info!(
            "shields: no cached filter lists in {}, using the bundled rules only",
            store.dir().display()
        );
        return;
    }
    let reports = adblock.install_lists(cached.iter().map(|list| {
        (
            list.meta.source_id.as_str(),
            list.meta.title.as_str(),
            list.text.clone(),
            Some(list.meta.fetched_at_unix),
        )
    }));
    let rules: usize = reports.iter().map(|report| report.accepted).sum();
    info!(
        "shields: loaded {} cached filter lists, {rules} rules compiled, {} rules total",
        cached.len(),
        adblock.rule_count()
    );
}

/// One line per source for the menu, phrased so a failure is visible as a
/// failure rather than silence.
fn describe_action(action: &SourceAction) -> String {
    match action {
        SourceAction::Updated {
            source_id,
            title,
            text,
            ..
        } => format!("{title} ({source_id}): downloaded {} bytes", text.len()),
        SourceAction::Unchanged { title, .. } => format!("{title}: still current"),
        SourceAction::Failed {
            title,
            reason,
            source_id,
            ..
        } => format!("{title} ({source_id}): kept the previous copy, {reason}"),
    }
}

fn age_label(updated_unix: Option<i64>) -> String {
    let Some(then) = updated_unix else {
        return "ships with the browser".to_string();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(then);
    let minutes = (now - then).max(0) / 60;
    match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        60..=1_439 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1_440),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_adblock::upstream::ListMeta;

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// These strings are what the user reads instead of a number they can check,
    /// so the wording and the units are part of the feature.
    #[test]
    fn list_status_and_log_lines_say_what_is_true() {
        assert_eq!(age_label(None), "ships with the browser");
        assert_eq!(age_label(Some(now())), "just now");
        assert_eq!(age_label(Some(now() - 5 * 60)), "5 min ago");
        assert_eq!(age_label(Some(now() - 3 * 3600)), "3 h ago");
        assert_eq!(age_label(Some(now() - 2 * 86_400)), "2 d ago");
        // A clock skew into the future must not read as a negative age.
        assert_eq!(age_label(Some(now() + 3600)), "just now");

        let updated = SourceAction::Updated {
            source_id: "easylist".to_string(),
            title: "EasyList".to_string(),
            text: "x".repeat(2048),
            fetched_at_unix: now(),
        };
        assert_eq!(
            describe_action(&updated),
            "EasyList (easylist): downloaded 2048 bytes"
        );
        assert_eq!(
            describe_action(&SourceAction::Unchanged {
                source_id: "easyprivacy".to_string(),
                title: "EasyPrivacy".to_string(),
            }),
            "EasyPrivacy: still current"
        );
        let failed = describe_action(&SourceAction::Failed {
            source_id: "ublock-filters".to_string(),
            title: "uBlock filters".to_string(),
            reason: "server returned HTTP 503".to_string(),
        });
        assert!(failed.starts_with("uBlock filters (ublock-filters): kept the previous copy"));
        assert!(failed.contains("503"), "{failed}");
    }

    #[test]
    fn cached_lists_are_installed_at_startup_and_counted() {
        let dir = std::env::temp_dir().join(format!(
            "craft-ui-startup-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = ListStore::new(&dir);
        let source = &DEFAULT_SOURCES[0];
        let body = format!("[Adblock Plus 2.0]\n! Title: {}\n", source.title)
            + "||startup-check.example.test^\n";
        let meta = ListMeta {
            source_id: source.id.to_string(),
            title: source.title.to_string(),
            url: source.url.to_string(),
            fetched_at_unix: now(),
            etag: None,
            last_modified: None,
            bytes: body.len() as u64,
        };
        store.save(source, &meta, &body).unwrap();

        let adblock = AdblockManager::new();
        let bundled = adblock.rule_count();
        install_cached_lists(&adblock, &store);

        assert_eq!(adblock.rule_count(), bundled + 1);
        assert!(adblock.check_network_request(
            "https://startup-check.example.test/pixel.js",
            "https://site.test/",
            craft_adblock::SCRIPT,
        ));
        let status = adblock.groups();
        assert_eq!(status.len(), 2, "bundled plus the cached list");
        assert_eq!(status[1].title, source.title);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_cache_leaves_the_bundled_rules_alone() {
        let dir = std::env::temp_dir().join(format!(
            "craft-ui-nocache-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = ListStore::new(&dir);
        let adblock = AdblockManager::new();
        let before = adblock.rule_count();
        install_cached_lists(&adblock, &store);
        assert_eq!(adblock.rule_count(), before);
        assert_eq!(adblock.groups().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn one_update_pass_at_a_time() {
        // No real pass here: starting one would hit the network from a unit test.
        // The guard is what matters, so the in-flight state is set directly.
        let mut app = BrowserApp::new();
        assert!(!app.lists_updating());

        let (sender, receiver) = std::sync::mpsc::channel::<UpdateEvent>();
        app.list_updates = Some(receiver);
        assert!(app.lists_updating());

        app.update_filter_lists();
        assert!(
            app.filter_list_log()
                .last()
                .unwrap()
                .contains("already updating"),
            "a second click must refuse rather than start a competing writer: {:?}",
            app.filter_list_log()
        );

        // A pass that reported one updated source installs it when it finishes.
        let body = "[Adblock Plus 2.0]\n! Title: T\n||guard-check.example.test^\n".to_string();
        let before = app.adblock.rule_count();
        assert!(
            sender
                .send(UpdateEvent::Source(SourceAction::Updated {
                    source_id: "guard-check".to_string(),
                    title: "Guard check".to_string(),
                    text: body,
                    fetched_at_unix: 1,
                }))
                .is_ok()
        );
        let _ = sender.send(UpdateEvent::Finished {
            updated: 1,
            unchanged: 0,
            failed: 0,
        });
        drop(sender);

        app.poll_list_updates();
        assert!(!app.lists_updating());
        assert_eq!(app.adblock.rule_count(), before + 1);
        assert!(app.adblock.check_network_request(
            "https://guard-check.example.test/pixel.js",
            "https://site.test/",
            craft_adblock::SCRIPT,
        ));

        // Nothing left over: a later pass has no pending actions to install.
        app.poll_list_updates();
        assert_eq!(app.adblock.rule_count(), before + 1);
    }
}
