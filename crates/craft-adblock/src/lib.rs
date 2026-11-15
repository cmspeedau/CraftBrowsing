//! Adblocking and network filter list engine powered by `adblock-rust`.
//!
//! This crate compiles filter rules and answers "should this request be
//! blocked?". It does not intercept anything: the browser's engine hook calls
//! [`AdblockManager::check_network_request`] per request and cancels the ones it
//! says yes to. That split is why this crate has no engine dependency and can be
//! tested without a page.
//!
//! Two kinds of rules exist: the curated list that ships in the binary, and
//! upstream lists downloaded on the user's request
//! ([`upstream`][crate::upstream]). Both live in the manager as named
//! [`RuleGroup`]s, because `adblock-rust` cannot add or remove rules on a live
//! engine: any change rebuilds it from every group, so the groups have to be
//! kept around.
//!
//! One constraint worth knowing before using this type: `adblock`'s engine is
//! neither `Send` nor `Sync` (a `RefCell` regex cache and an `Rc` capped
//! collection inside it), so a manager must be built, rebuilt and queried on one
//! thread. Slow work that can leave that thread, such as downloading a list,
//! belongs on a worker; compiling its rules does not. See `craft_ui::lists`.

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::{debug, info, warn};

pub mod upstream;

/// The rules that ship with the browser, compiled at startup. Curated
/// endpoints, not a redistribution of a third-party list; see the file header
/// for scope and for the services that are deliberately left alone.
pub const DEFAULT_NETWORK_FILTERS: &str = include_str!("../lists/default-network-filters.txt");

/// Group id of the rules that ship with the binary.
pub const BUNDLED_GROUP: &str = "bundled";

/// Group id of rules added at runtime (a user's own list, or tests).
pub const USER_GROUP: &str = "user";

/// Request resource types understood by the filter syntax. The browser maps its
/// engine's request destinations onto these names; anything unrecognised falls
/// back to `other`, which silently weakens type-restricted rules, so keep the
/// two sides in step (`craft_ui::shields::adblock_request_type`).
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

/// What a rule load produced. Reported rather than swallowed: a list where ten
/// thousand rules failed to parse is a functional change, not a detail.
///
/// The counts are for the *group after the load*, not the delta, because the
/// engine is rebuilt from the group's whole text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleLoadReport {
    /// Lines compiled into the engine.
    pub accepted: usize,
    /// Rule lines the parser refused, with the reason logged per rule.
    pub rejected: usize,
}

impl RuleLoadReport {
    /// Rule lines that were not comments or blank.
    pub fn total(&self) -> usize {
        self.accepted + self.rejected
    }
}

/// One named set of rules inside the manager: the bundled list, a user's own
/// rules, or a single upstream list.
///
/// The body is kept as one string rather than a vector of lines. The three
/// upstream lists together are 141k rule lines and about 4 MB of text; holding
/// them as 141k separate `String`s would cost far more than the engine itself,
/// and nothing needs random access to individual lines.
#[derive(Debug, Clone)]
pub struct RuleGroup {
    /// Stable identifier, e.g. `"easylist"`. Also the cache file name stem.
    pub id: String,
    /// Display name, from the list's own `! Title:` header when it has one.
    pub title: String,
    /// Number of rule lines that compiled.
    pub accepted: usize,
    /// Number of rule lines the parser refused.
    pub rejected: usize,
    /// When this group's content last changed, as unix seconds. `None` for the
    /// rules that ship in the binary, which never go stale.
    pub updated_unix: Option<i64>,
    body: String,
}

impl RuleGroup {
    pub fn new(id: &str, title: &str, body: String, updated_unix: Option<i64>) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            accepted: 0,
            rejected: 0,
            updated_unix,
            body,
        }
    }

    /// Rule lines in the body, excluding comments and blanks.
    pub fn rule_lines(&self) -> impl Iterator<Item = &str> {
        self.body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !is_comment_line(line))
    }
}

pub struct AdblockManager {
    /// Every group compiled into `engine`, in load order. Kept because
    /// `adblock-rust` engines are immutable once built, so any change has to
    /// rebuild from all sources. (It also fixes a real bug: the earlier
    /// `add_filter_rules` built a fresh `FilterSet` per call, so loading a
    /// second list silently discarded the first.)
    groups: RwLock<Vec<RuleGroup>>,
    engine: RwLock<Engine>,
    enabled: RwLock<bool>,
    site_exceptions: RwLock<Vec<String>>,
}

impl AdblockManager {
    /// A manager with the shipped default rules compiled and filtering on.
    pub fn new() -> Self {
        let manager = Self::empty();
        let report = manager.set_group(
            BUNDLED_GROUP,
            "Bundled rules",
            DEFAULT_NETWORK_FILTERS.to_string(),
            None,
        );
        info!(
            "shields: {} bundled network rules compiled ({} rejected)",
            report.accepted, report.rejected
        );
        manager
    }

    /// A manager with no rules at all, for tests and for callers that load
    /// everything themselves.
    pub fn empty() -> Self {
        Self {
            groups: RwLock::new(Vec::new()),
            engine: RwLock::new(build_engine([])),
            enabled: RwLock::new(true),
            site_exceptions: RwLock::new(Vec::new()),
        }
    }

    /// Replace (or create) one group's rules and rebuild the engine once.
    ///
    /// Prefer [`AdblockManager::install_lists`] when several lists changed at
    /// once: each rebuild reparses every group, so applying three upstream lists
    /// one at a time costs three full parses.
    pub fn set_group(
        &self,
        id: &str,
        title: &str,
        body: String,
        updated_unix: Option<i64>,
    ) -> RuleLoadReport {
        let mut reports = self.install_lists([(id, title, body, updated_unix)]);
        reports.pop().unwrap_or_default()
    }

    /// Replace (or create) several groups, then rebuild the engine once. Returns
    /// one report per input, in the same order.
    pub fn install_lists<'a, I>(&self, lists: I) -> Vec<RuleLoadReport>
    where
        I: IntoIterator<Item = (&'a str, &'a str, String, Option<i64>)>,
    {
        let mut groups = self.groups.write().unwrap();
        let mut ids: Vec<String> = Vec::new();

        for (id, title, body, updated_unix) in lists {
            ids.push(id.to_string());
            match groups.iter_mut().position(|g| g.id == id) {
                Some(index) => groups[index] = RuleGroup::new(id, title, body, updated_unix),
                None => groups.push(RuleGroup::new(id, title, body, updated_unix)),
            }
        }

        let counts = rebuild_locked(&mut groups, &self.engine);
        for id in &ids {
            let report = report_for(&counts, id);
            info!(
                "shields: list {id:?} active with {} rules ({} refused by the parser); {} groups, {} rules total",
                report.accepted,
                report.rejected,
                groups.len(),
                groups.iter().map(|g| g.accepted).sum::<usize>(),
            );
        }

        ids.iter().map(|id| report_for(&counts, id)).collect()
    }

    /// Append rules to the manager's own [`USER_GROUP`].
    ///
    /// The returned report is the group's totals after the append, not the delta:
    /// the engine is rebuilt from the whole group.
    pub fn add_filter_rules(&self, raw_rules: &[String]) -> RuleLoadReport {
        let addition = raw_rules
            .iter()
            .map(|rule| rule.trim())
            .filter(|rule| !rule.is_empty() && !is_comment_line(rule))
            .collect::<Vec<_>>()
            .join("\n");

        let mut groups = self.groups.write().unwrap();
        match groups.iter_mut().find(|g| g.id == USER_GROUP) {
            Some(group) => {
                if group.body.is_empty() {
                    group.body = addition;
                } else {
                    group.body.push('\n');
                    group.body.push_str(&addition);
                }
                group.updated_unix = Some(now_unix());
            }
            None => groups.push(RuleGroup::new(
                USER_GROUP,
                "Custom rules",
                addition,
                Some(now_unix()),
            )),
        }
        report_for(&rebuild_locked(&mut groups, &self.engine), USER_GROUP)
    }

    /// Drop one group, e.g. a list the user removed.
    pub fn remove_group(&self, id: &str) -> bool {
        let mut groups = self.groups.write().unwrap();
        let before = groups.len();
        groups.retain(|group| group.id != id);
        if groups.len() != before {
            rebuild_locked(&mut groups, &self.engine);
            true
        } else {
            false
        }
    }

    /// The loaded groups, in load order, for display.
    pub fn groups(&self) -> Vec<RuleGroup> {
        self.groups.read().unwrap().clone()
    }

    /// Total compiled rules across all groups.
    pub fn rule_count(&self) -> usize {
        self.groups.read().unwrap().iter().map(|g| g.accepted).sum()
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

/// Recompile the engine from every group and return per-group counts.
///
/// The caller holds the groups write lock, so this cannot race a reader: readers
/// take the engine lock and always see a complete engine.
fn rebuild_locked(
    groups: &mut [RuleGroup],
    engine_slot: &RwLock<Engine>,
) -> HashMap<String, (usize, usize)> {
    let mut filter_set = FilterSet::new(false);
    let opts = ParseOptions::default();
    let mut counts: Vec<(String, usize, usize)> = Vec::with_capacity(groups.len());

    for group in groups.iter() {
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for rule in group.rule_lines() {
            if let Err(error) = filter_set.add_filter(rule, opts) {
                rejected += 1;
                // Only the first few per group: a bad upstream list would
                // otherwise flood the log with 141k lines.
                if rejected <= 3 {
                    debug!("shields: rejected a rule in {:?}: {error}", group.id);
                }
            } else {
                accepted += 1;
            }
        }
        counts.push((group.id.clone(), accepted, rejected));
    }

    for (id, accepted, rejected) in &counts {
        if *rejected > 0 {
            warn!(
                "shields: list {id:?} compiled {accepted} rules and dropped {rejected} the parser refused"
            );
        }
    }

    // Second argument is `optimize`, not cosmetic support: it lets adblock-rust
    // merge similar rules. Cosmetic rules are parsed (and counted above) but
    // nothing applies them yet; see docs/tracking-protection.md.
    let engine = Engine::from_filter_set(filter_set, true);
    *engine_slot.write().unwrap() = engine;

    let mut results = HashMap::with_capacity(counts.len());
    for (id, accepted, rejected) in counts {
        results.insert(id.clone(), (accepted, rejected));
    }
    for group in groups.iter_mut() {
        if let Some((accepted, rejected)) = results.get(&group.id) {
            group.accepted = *accepted;
            group.rejected = *rejected;
        }
    }
    results
}

fn build_engine<'a, I: IntoIterator<Item = &'a str>>(rules: I) -> Engine {
    let mut filter_set = FilterSet::new(false);
    for rule in rules {
        filter_set.add_filter(rule, ParseOptions::default()).ok();
    }
    Engine::from_filter_set(filter_set, true)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn report_for(counts: &HashMap<String, (usize, usize)>, id: &str) -> RuleLoadReport {
    counts
        .get(id)
        .map(|(accepted, rejected)| RuleLoadReport {
            accepted: *accepted,
            rejected: *rejected,
        })
        .unwrap_or_default()
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
        let manager = AdblockManager::empty();
        let first = vec!["||ads.example-one.test^".to_string()];
        let second = vec!["||stats.example-two.test^".to_string()];
        assert_eq!(manager.add_filter_rules(&first).accepted, 1);
        assert_eq!(manager.add_filter_rules(&second).accepted, 2);
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
    fn install_lists_applies_several_lists_in_one_pass() {
        let manager = AdblockManager::empty();
        let reports = manager.install_lists([
            (
                "easylist",
                "EasyList",
                "||ad1.example.test^".to_string(),
                Some(10),
            ),
            (
                "easyprivacy",
                "EasyPrivacy",
                "||track1.example.test^".to_string(),
                Some(11),
            ),
        ]);
        assert_eq!(
            reports,
            vec![
                RuleLoadReport {
                    accepted: 1,
                    rejected: 0
                },
                RuleLoadReport {
                    accepted: 1,
                    rejected: 0
                }
            ]
        );
        assert_eq!(manager.rule_count(), 2);
        let groups = manager.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "EasyList");
        assert_eq!(groups[1].updated_unix, Some(11));
        for host in ["ad1", "track1"] {
            assert!(
                manager.check_network_request(
                    &format!("https://{host}.example.test/pixel.js"),
                    "https://site.test/",
                    SCRIPT,
                ),
                "{host} must be blocked after a batch install"
            );
        }
    }

    #[test]
    fn a_group_can_be_replaced_without_disturbing_the_others() {
        // The point of groups: an upstream list updates its own rules only.
        let manager = AdblockManager::empty();
        manager.set_group("one", "List one", "||one.example.test^".to_string(), None);
        manager.set_group("two", "List two", "||two.example.test^".to_string(), None);
        assert_eq!(manager.rule_count(), 2);

        let report = manager.set_group(
            "two",
            "List two",
            "||two.example.test^\n||three.example.test^".to_string(),
            None,
        );
        assert_eq!(
            report.accepted, 2,
            "the replaced group reports its own totals"
        );
        assert_eq!(manager.rule_count(), 3);
        for host in ["one", "two", "three"] {
            assert!(
                manager.check_network_request(
                    &format!("https://{host}.example.test/pixel.js"),
                    "https://site.test/",
                    SCRIPT,
                ),
                "{host} should still be blocked"
            );
        }

        assert!(manager.remove_group("two"));
        assert!(!manager.remove_group("two"));
        assert_eq!(manager.rule_count(), 1);
        assert!(manager.check_network_request(
            "https://one.example.test/pixel.js",
            "https://site.test/",
            SCRIPT,
        ));
        assert!(!manager.check_network_request(
            "https://two.example.test/pixel.js",
            "https://site.test/",
            SCRIPT,
        ));
    }

    #[test]
    fn unparseable_rules_are_counted_not_swallowed() {
        let manager = AdblockManager::empty();
        // `$unknownoption` is not valid filter syntax.
        let report = manager.set_group(
            "mixed",
            "Mixed",
            "||good.example.test^\n||bad.example.test^$unknownoption=true".to_string(),
            None,
        );
        assert_eq!(
            report,
            RuleLoadReport {
                accepted: 1,
                rejected: 1
            }
        );
        assert!(manager.check_network_request(
            "https://good.example.test/x.js",
            "https://site.test/",
            SCRIPT,
        ));
    }

    #[test]
    fn group_rule_lines_skip_comments_and_headers() {
        let group = RuleGroup::new(
            "g",
            "G",
            "[Adblock Plus 2.0]\n! a comment\n\n||x.example^\n".to_string(),
            None,
        );
        assert_eq!(group.rule_lines().collect::<Vec<_>>(), vec!["||x.example^"]);
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
