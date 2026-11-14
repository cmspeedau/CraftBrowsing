//! Adblocking and network filter list engine powered by `adblock-rust`.

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use std::sync::RwLock;

pub struct AdblockManager {
    engine: RwLock<Engine>,
    enabled: RwLock<bool>,
    site_exceptions: RwLock<Vec<String>>,
}

impl AdblockManager {
    pub fn new() -> Self {
        let filter_set = FilterSet::new(true);
        let engine = Engine::from_filter_set(filter_set, true);
        Self {
            engine: RwLock::new(engine),
            enabled: RwLock::new(true),
            site_exceptions: RwLock::new(Vec::new()),
        }
    }
}

impl Default for AdblockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AdblockManager {
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().unwrap() = enabled;
    }

    pub fn add_filter_rules(&self, raw_rules: &[String]) {
        let mut filter_set = FilterSet::new(true);
        for rule in raw_rules {
            filter_set.add_filter(rule, ParseOptions::default()).ok();
        }
        let new_engine = Engine::from_filter_set(filter_set, true);
        *self.engine.write().unwrap() = new_engine;
    }

    pub fn check_network_request(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        if !self.is_enabled() {
            return false;
        }

        let exceptions = self.site_exceptions.read().unwrap();
        if exceptions.iter().any(|domain| source_url.contains(domain)) {
            return false;
        }

        let engine = self.engine.read().unwrap();
        let request = match Request::new(url, source_url, request_type) {
            Ok(request) => request,
            Err(_) => return false,
        };
        let blocker_result = engine.check_network_request(&request);
        blocker_result.matched
    }
}
