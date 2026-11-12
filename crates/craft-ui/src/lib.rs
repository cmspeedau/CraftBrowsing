//! Browser shell UI and main application entrypoint for CraftBrowsing.

pub mod window;
pub mod omnibox;
pub mod tab_manager;

use craft_adblock::AdblockManager;
use craft_core::container::{Container, ContainerId};
use craft_core::tab::{Tab, TabGroup, TabId};
use craft_engine::{EngineCommand, EngineEvent, ServoEngine};
use craft_plugins::PluginRuntime;
use craft_sync::protocol::SyncPreferences;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

pub use window::{BrowserWindowApp, run_window_event_loop};
pub use omnibox::{Omnibox, OmniboxAction, OmniboxState, SearchEngine};
pub use tab_manager::{VerticalTabManager, TabNode, TabState};

pub struct BrowserApp {
    pub adblock: Arc<AdblockManager>,
    pub sync_preferences: SyncPreferences,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<TabId>,
    pub groups: Vec<TabGroup>,
    pub containers: Vec<Container>,
    pub plugin_runtime: PluginRuntime,
    pub engine: ServoEngine,
    pub cmd_sender: mpsc::UnboundedSender<EngineCommand>,
    pub omnibox: Omnibox,
    pub tab_manager: VerticalTabManager,
}

impl BrowserApp {
    pub fn new() -> Self {
        info!("Initializing CraftBrowsing browser shell with Servo engine integration...");

        let adblock = Arc::new(AdblockManager::new());
        let sync_preferences = SyncPreferences::default();
        let plugin_runtime = PluginRuntime::new();
        let omnibox = Omnibox::default();
        let tab_manager = VerticalTabManager::new();

        let (engine, cmd_sender) = ServoEngine::new(adblock.clone());

        Self {
            adblock,
            sync_preferences,
            tabs: Vec::new(),
            active_tab_id: None,
            groups: Vec::new(),
            containers: Vec::new(),
            plugin_runtime,
            engine,
            cmd_sender,
            omnibox,
            tab_manager,
        }
    }

    pub fn open_tab(&mut self, url: &str, container_id: ContainerId) -> TabId {
        let tab_id = TabId(uuid_v4_stub());
        let tab = Tab::new(tab_id.clone(), url, container_id.clone());
        self.tabs.push(tab);

        // Register in tree-style VerticalTabManager
        let parent_id = self.active_tab_id.clone();
        self.tab_manager.insert_tab(tab_id.clone(), parent_id, container_id.clone(), url);

        // Spawn a corresponding Servo browser context partitioned by container
        self.engine.create_context(tab_id.clone(), container_id, url);
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
        let action = self.omnibox.resolve(input);
        let target_url = match action {
            OmniboxAction::Navigate(url) => url,
            OmniboxAction::Search { query, engine } => engine.search_url(&query),
            OmniboxAction::InternalPage(page) => page,
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
            if let Some(node) = self.tab_manager.get_node_mut(tab_id) {
                if node.state == TabState::Hibernated {
                    self.tab_manager.wake_tab(tab_id);
                    self.engine.create_context(tab_id.clone(), node.container_id.clone(), &node.url);
                }
            }
        }
    }

    pub fn hibernate_tab(&mut self, tab_id: &TabId) {
        if self.active_tab_id.as_ref() == Some(tab_id) {
            return; // Never hibernate active tab
        }
        self.tab_manager.hibernate_tab(tab_id);
        let _ = self.cmd_sender.send(EngineCommand::DestroyContext {
            tab_id: tab_id.clone(),
        });
    }

    pub fn close_tab(&mut self, tab_id: &TabId) {
        let _ = self.cmd_sender.send(EngineCommand::DestroyContext {
            tab_id: tab_id.clone(),
        });
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

    pub fn poll_engine_events(&mut self) -> Vec<EngineEvent> {
        let events = self.engine.poll_events();
        for evt in &events {
            if let EngineEvent::Navigation(nav) = evt {
                if self.active_tab_id.as_ref() == Some(&nav.tab_id) {
                    self.omnibox.set_text(&nav.url);
                }
                self.tab_manager.update_tab_url(&nav.tab_id, &nav.url);
            }
        }
        events
    }
}

impl Default for BrowserApp {
    fn default() -> Self {
        Self::new()
    }
}

fn uuid_v4_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("tab-{:x}", nanos)
}
