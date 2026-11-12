//! Browser shell UI and main application entrypoint for CraftBrowsing.

use craft_adblock::AdblockManager;
use craft_core::container::{Container, ContainerId};
use craft_core::tab::{Tab, TabGroup, TabId};
use craft_engine::{ServoConfig, ServoEngine, NavigationCommand, EngineEvent};
use craft_plugins::PluginRuntime;
use craft_sync::protocol::SyncPreferences;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

pub struct BrowserApp {
    pub adblock: Arc<AdblockManager>,
    pub sync_preferences: SyncPreferences,
    pub tabs: Vec<Tab>,
    pub groups: Vec<TabGroup>,
    pub containers: Vec<Container>,
    pub plugin_runtime: PluginRuntime,
    pub engine: ServoEngine,
    pub engine_events_rx: mpsc::UnboundedReceiver<EngineEvent>,
}

impl BrowserApp {
    pub fn new() -> Self {
        info!("Initializing CraftBrowsing browser shell with Servo engine integration...");

        let adblock = Arc::new(AdblockManager::new());
        let sync_preferences = SyncPreferences::default();
        let plugin_runtime = PluginRuntime::new();

        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let servo_config = ServoConfig::default();
        let engine = ServoEngine::new(servo_config, events_tx);

        Self {
            adblock,
            sync_preferences,
            tabs: Vec::new(),
            groups: Vec::new(),
            containers: Vec::new(),
            plugin_runtime,
            engine,
            engine_events_rx: events_rx,
        }
    }

    pub fn open_tab(&mut self, url: &str, container_id: ContainerId) -> TabId {
        let tab_id = TabId(uuid_v4_stub());
        let tab = Tab::new(tab_id.clone(), url, container_id.clone());
        self.tabs.push(tab);

        // Spawn a corresponding Servo browser context partitioned by container
        self.engine.create_browser_context(tab_id.clone(), container_id, url.to_string());
        tab_id
    }

    pub fn navigate_tab(&self, tab_id: &TabId, url: &str) {
        self.engine.dispatch_navigation(
            tab_id,
            NavigationCommand::Navigate {
                url: url.to_string(),
                user_initiated: true,
            },
        );
    }

    pub fn close_tab(&mut self, tab_id: &TabId) {
        self.engine.close_browser_context(tab_id);
        self.tabs.retain(|t| &t.id != tab_id);
    }
}

fn uuid_v4_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("tab-{:x}", nanos)
}
