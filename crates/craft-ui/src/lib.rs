//! Browser shell UI and main application entrypoint for CraftBrowsing.

use craft_adblock::AdblockManager;
use craft_core::container::Container;
use craft_core::tab::{Tab, TabGroup, TabId};
use craft_plugins::PluginRuntime;
use craft_sync::protocol::SyncPreferences;
use std::sync::Arc;
use tracing::info;

pub struct BrowserApp {
    pub adblock: Arc<AdblockManager>,
    pub sync_preferences: SyncPreferences,
    pub tabs: Vec<Tab>,
    pub groups: Vec<TabGroup>,
    pub containers: Vec<Container>,
    pub plugin_runtime: PluginRuntime,
}

impl BrowserApp {
    pub fn new() -> Self {
        info!("Initializing CraftBrowsing browser shell...");

        let adblock = Arc::new(AdblockManager::new());
        let sync_preferences = SyncPreferences::default();
        let plugin_runtime = PluginRuntime::new();

        Self {
            adblock,
            sync_preferences,
            tabs: Vec::new(),
            groups: Vec::new(),
            containers: Vec::new(),
            plugin_runtime,
        }
    }

    pub fn open_tab(&mut self, url: &str, container_id: craft_core::container::ContainerId) -> TabId {
        let tab_id = TabId(uuid_v4_stub());
        let tab = Tab::new(tab_id.clone(), url, container_id);
        self.tabs.push(tab);
        tab_id
    }
}

fn uuid_v4_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("tab-{:x}", nanos)
}
