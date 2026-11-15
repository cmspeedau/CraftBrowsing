//! Adblocking and network filter list engine powered by `adblock-rust`.
//!
//! This crate compiles filter rules and answers "should this request be
//! blocked?". It does not intercept anything: the browser's engine hook calls
//! [`AdblockManager::check_network_request`] per request and cancels the ones it
//! says yes to. That split is why this crate has no engine or network
//! dependencies and can be tested without a page.

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// The rules that ship with the browser, compiled at startup. Curated
/// endpoints, not a redistribution of a third-party list; see the file header
/// for scope and for the services that are deliberately left alone.
pub const DEFAULT_NETWORK_FILTERS: &str = include_str!("../lists/default-network-filters.txt");

/// Request resource types understood by the filter syntax. The browser maps its
/// engine's request destinations onto these names; anything unrecognised falls
/// back to `other`, which silently weakens type-restricted rules, so keep the
/// two sides in step (`craft_ui::window::adblock_request_type`).
pub const DOCUMENT: &str = "document";
pub const SUBDOCUMENT: &str = "sub_frame";
pub const SCRIPT: &str = "script";
pub const STYLESHEET: &str = "stylesheet";
pub const IMAGE: &str = "image";
pub const FONT: &str = "font";
pub const MEDIA: &str = "media";
pub const XHR: &str = "xmlhttprequest";
pub const PING: &str = "ping";
pub const WEBSOCKET: &str = "websocket";
pub const OTHER: &str = "other";

/// Every type name the matcher recognises, used by tests to reject typos.
pub const KNOWN_REQUEST_TYPES: [&str; 11] = [
    DOCUMENT,
    SUBDOCUMENT,
    SCRIPT,
    STYLESHEET,
    IMAGE,
    FONT,
    MEDIA,
    XHR,
    PING,
    WEBSOCKET,
    OTHER,
];

/// Result of loading a set of rules. Reported rather than swallowed: a list that
/// fails to parse is a functional change, not a detail.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleLoadReport {
    /// Rules compiled into the engine.
    pub accepted: usize,
    /// Rules the parser rejected, with the reason logged per rule.
    pub rejected: usize,
}

impl RuleLoadReport {
    pub fn total(&self) -> usize {
        self.accepted + self.rejected
    }
}

pub struct AdblockManager {
    /// Every rule compiled into `engine`, in insertion order. Kept so that
    /// loading a second list extends the set. The previous implementation built
    /// a fresh `FilterSet` on every call, so a second list silently discarded
    /// the first one.
    rules: RwLock<Vec<String>>,
    engine: RwLock<Engine>,
    enabled: RwLock<bool>,
    site_exceptions: RwLock<Vec<String>>,
}

impl AdblockManager {
    /// A manager with the shipped default rules compiled and filtering on.
    pub fn new() -> Self {
        let manager = Self::empty();
        manager.add_filter_rules(&default_network_rules());
        info!(
            "shields: {} network rules compiled from the bundled lists",
            manager.rule_count()
        );
        manager
    }

    /// A manager with no rules at all, for tests and for callers that want to
    /// load their own lists.
    pub fn empty() -> Self {
        Self {
            rules: RwLock::new(Vec::new()),
            engine: RwLock::new(engine_from(&[])),
            enabled: RwLock::new(true),
            site_exceptions: RwLock::new(Vec::new()),
        }
    }

    /// Compile additional rules on top of the ones already loaded.
    pub fn add_filter_rules(&self, raw_rules: &[String]) -> RuleLoadReport {
        let mut report = RuleLoadReport::default();
        let mut rules = self.rules.write().unwrap();

        for rule in raw_rules {
            let rule = rule.trim();
            if rule.is_empty() || is_comment_line(rule) {
                continue;
            }
            // Probe the rule on a scratch set so a rejected line is reported
            // with its own reason instead of vanishing into a rebuild.
            let mut probe = FilterSet::new(true);
            if let Err(error) = probe.add_filter(rule, ParseOptions::default()) {
                report.rejected += 1;
                debug!("shields: rejected filter {rule:?}: {error}");
                continue;
            }
            rules.push(rule.to_string());
            report.accepted += 1;
        }

        if report.accepted > 0 {
            // adblock-rust compiles a whole engine from a filter set; there is no
            // incremental update, so the rebuild is per load, not per request.
            let engine = engine_from(&rules);
            *self.engine.write().unwrap() = engine;
        }

        if report.rejected > 0 {
            warn!(
                "shields: {}/{} rules rejected while loading a filter list",
                report.rejected,
                report.total()
            );
        }
        report
    }

    /// Number of rules currently compiled.
    pub fn rule_count(&self) -> usize {
        self.rules.read().unwrap().len()
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().unwrap() = enabled;
    }

    /// Stop filtering requests whose page URL contains `domain_or_host`. This is
    /// the coarse per-site opt-out the shield toggle uses; it is not a rule-level
    /// exception, it suspends matching for that page entirely.
    pub fn add_site_exception(&self, domain_or_host: &str) {
        let mut exceptions = self.site_exceptions.write().unwrap();
        let needle = domain_or_host.to_string();
        if !exceptions.contains(&needle) {
            exceptions.push(needle);
        }
    }

    pub fn remove_site_exception(&self, domain_or_host: &str) {
        self.site_exceptions
            .write()
            .unwrap()
            .retain(|entry| entry != domain_or_host);
    }

    pub fn site_exceptions(&self) -> Vec<String> {
        self.site_exceptions.read().unwrap().clone()
    }

    /// Should this request be blocked?
    ///
    /// `request_type` must be one of [`KNOWN_REQUEST_TYPES`]. A load the matcher
    /// cannot parse is allowed through: failing open on a malformed URL is the
    /// correct trade for a browser, because a false block breaks a page while a
    /// missed block only leaks a request.
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
            Err(error) => {
                debug!("shields: could not parse request {url:?}: {error}");
                return false;
            }
        };
        engine.check_network_request(&request).matched
    }
}

impl Default for AdblockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The bundled rules as individually owned strings, comments stripped.
pub fn default_network_rules() -> Vec<String> {
    DEFAULT_NETWORK_FILTERS
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_comment_line(line))
        .map(str::to_string)
        .collect()
}

fn is_comment_line(line: &str) -> bool {
    line.starts_with('!') || line.starts_with('[')
}

fn engine_from(rules: &[String]) -> Engine {
    let mut filter_set = FilterSet::new(true);
    for rule in rules {
        filter_set.add_filter(rule, ParseOptions::default()).ok();
    }
    // The second argument enables cosmetic-filter parsing. The browser does not
    // apply cosmetic rules yet, so they cost memory and change nothing; element
    // hiding is its own roadmap item.
    Engine::from_filter_set(filter_set, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker_script() -> bool {
        AdblockManager::new().check_network_request(
            "https://www.google-analytics.com/analytics.js",
            "https://news.example.org/article",
            SCRIPT,
        )
    }

    #[test]
    fn bundled_rules_compile_without_rejections() {
        let manager = AdblockManager::empty();
        let report = manager.add_filter_rules(&default_network_rules());
        assert_eq!(report.rejected, 0, "a bundled rule must not be rejected");
        assert!(
            report.accepted > 20,
            "the bundled lists should carry a real ruleset, got {}",
            report.accepted
        );
        assert_eq!(manager.rule_count(), report.accepted);
    }

    #[test]
    fn new_manager_blocks_a_known_tracker_endpoint() {
        assert!(tracker_script());
    }

    #[test]
    fn first_party_and_unlisted_requests_pass() {
        let manager = AdblockManager::new();
        assert!(!manager.check_network_request(
            "https://news.example.org/article",
            "https://news.example.org/",
            DOCUMENT,
        ));
        assert!(!manager.check_network_request(
            "https://news.example.org/app.js",
            "https://news.example.org/article",
            SCRIPT,
        ));
        assert!(!manager.check_network_request(
            "https://images.example.org/photo.jpg",
            "https://news.example.org/article",
            IMAGE,
        ));
    }

    #[test]
    fn disabling_protection_blocks_nothing() {
        let manager = AdblockManager::new();
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        assert!(!manager.check_network_request(
            "https://www.google-analytics.com/analytics.js",
            "https://news.example.org/article",
            SCRIPT,
        ));
    }

    #[test]
    fn site_exception_suspends_filtering_for_that_page() {
        let manager = AdblockManager::new();
        manager.add_site_exception("news.example.org");
        assert!(!manager.check_network_request(
            "https://www.google-analytics.com/analytics.js",
            "https://news.example.org/article",
            SCRIPT,
        ));
        // Other pages are still protected.
        assert!(manager.check_network_request(
            "https://www.google-analytics.com/analytics.js",
            "https://other.example.org/article",
            SCRIPT,
        ));
        manager.remove_site_exception("news.example.org");
        assert!(manager.check_network_request(
            "https://www.google-analytics.com/analytics.js",
            "https://news.example.org/article",
            SCRIPT,
        ));
    }

    #[test]
    fn loading_a_second_list_keeps_the_first() {
        // Regression: `add_filter_rules` used to rebuild the engine from only
        // the rules passed to that call, so the second list erased the first.
        let manager = AdblockManager::empty();
        let first = vec!["||ads.example-one.test^".to_string()];
        let second = vec!["||stats.example-two.test^".to_string()];
        assert_eq!(manager.add_filter_rules(&first).accepted, 1);
        assert_eq!(manager.add_filter_rules(&second).accepted, 1);
        assert_eq!(manager.rule_count(), 2);

        assert!(manager.check_network_request(
            "https://ads.example-one.test/banner.js",
            "https://site.test/",
            SCRIPT,
        ));
        assert!(manager.check_network_request(
            "https://stats.example-two.test/beacon",
            "https://site.test/",
            PING,
        ));
    }

    #[test]
    fn type_restricted_rules_only_match_their_type() {
        // Guards the browser's destination mapping: a wrong type name would
        // look fine and quietly stop matching.
        let manager = AdblockManager::empty();
        manager.add_filter_rules(&["||tracker.example.test/collect.js$script".to_string()]);
        assert!(manager.check_network_request(
            "https://tracker.example.test/collect.js",
            "https://site.test/",
            SCRIPT,
        ));
        assert!(!manager.check_network_request(
            "https://tracker.example.test/collect.js",
            "https://site.test/",
            IMAGE,
        ));
    }

    #[test]
    fn unparsable_requests_are_allowed_through() {
        let manager = AdblockManager::new();
        assert!(!manager.check_network_request("not a url at all", "https://site.test/", SCRIPT));
        for name in KNOWN_REQUEST_TYPES {
            assert!(!manager.check_network_request("https://site.test/page", "", name));
        }
    }

    #[test]
    fn comment_and_blank_lines_are_not_compiled() {
        let manager = AdblockManager::empty();
        let report = manager.add_filter_rules(&[
            "! a comment".to_string(),
            "   ".to_string(),
            "[Adblock Plus 2.0]".to_string(),
            "||only-this.example.test^".to_string(),
        ]);
        assert_eq!(report.accepted, 1);
        assert_eq!(report.rejected, 0);
    }
}
