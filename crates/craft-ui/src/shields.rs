//! Tracking-protection state the chrome shows and the engine delegate updates.
//!
//! Deciding *whether* a request is blocked belongs to `craft-adblock`. This
//! module is the browser-side half: the counters shown in the chrome, and the
//! mapping from an engine request destination to the resource-type names the
//! filter syntax understands. The mapping lives here, next to the hook that uses
//! it, so a new engine destination is a compile error rather than a rule that
//! quietly stops matching.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use url::Url;

use content_security_policy::Destination;
use craft_adblock::{DOCUMENT, FONT, IMAGE, MEDIA, OTHER, PING, SCRIPT, STYLESHEET, SUBDOCUMENT};

/// Host of a page URL, for asking the matcher about a site rather than a request.
///
/// Empty or unparsable URLs (including `about:blank`, `craft://` pages and data
/// URLs) have no host worth pausing on, so they yield `None` rather than a string
/// that would match nothing.
pub fn host_of(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Session counters shared between the engine delegate (which sees every
/// request) and the chrome (which reports them). `Arc` because the delegate is
/// held by an `Rc` inside Servo and must not own the browser state.
#[derive(Debug, Default)]
pub struct ShieldStats {
    blocked: AtomicU64,
}

impl ShieldStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// One more request refused by a filter rule.
    pub fn record_block(&self) {
        self.blocked.fetch_add(1, Ordering::Relaxed);
    }

    pub fn blocked(&self) -> u64 {
        self.blocked.load(Ordering::Relaxed)
    }
}

/// Map an engine request destination onto a filter-syntax resource type.
///
/// Exhaustive on purpose: when the engine gains a destination, this must be
/// updated deliberately. A wildcard arm would file new request kinds under
/// `other`, and type-restricted rules (`$script`, `$ping`) would then silently
/// stop matching them.
pub fn adblock_request_type(destination: &Destination) -> &'static str {
    match destination {
        Destination::Document => DOCUMENT,
        // Nested browsing contexts: frames and iframes carry their own document.
        Destination::Frame | Destination::IFrame => SUBDOCUMENT,
        Destination::Script
        | Destination::Worker
        | Destination::ServiceWorker
        | Destination::SharedWorker
        | Destination::AudioWorklet
        | Destination::PaintWorklet => SCRIPT,
        Destination::Style => STYLESHEET,
        Destination::Image => IMAGE,
        Destination::Font => FONT,
        Destination::Audio | Destination::Video | Destination::Track => MEDIA,
        // Reporting endpoints: beacons and CSP reports are how a lot of
        // measurement traffic arrives.
        Destination::Report => PING,
        // Plugin and embedded objects are fetched rather than played, and the
        // shipped lists have no `$object` rules, so `other` is the honest label.
        Destination::Embed | Destination::Object | Destination::None => OTHER,
        Destination::Json
        | Destination::Text
        | Destination::Xslt
        | Destination::Manifest
        | Destination::WebIdentity => OTHER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_adblock::KNOWN_REQUEST_TYPES;

    const ALL: [Destination; 24] = [
        Destination::None,
        Destination::Audio,
        Destination::AudioWorklet,
        Destination::Document,
        Destination::Embed,
        Destination::Font,
        Destination::Frame,
        Destination::IFrame,
        Destination::Image,
        Destination::Json,
        Destination::Manifest,
        Destination::Object,
        Destination::PaintWorklet,
        Destination::Report,
        Destination::Script,
        Destination::ServiceWorker,
        Destination::SharedWorker,
        Destination::Style,
        Destination::Text,
        Destination::Track,
        Destination::Video,
        Destination::WebIdentity,
        Destination::Worker,
        Destination::Xslt,
    ];

    /// A typo in a mapped name is invisible at runtime: the matcher falls back to
    /// `other` and type-restricted rules stop firing. This pins every mapping to
    /// a name `craft-adblock` actually recognises.
    #[test]
    fn every_destination_maps_to_a_recognised_filter_type() {
        for destination in ALL {
            let name = adblock_request_type(&destination);
            assert!(
                KNOWN_REQUEST_TYPES.contains(&name),
                "{destination:?} mapped to unrecognised filter type {name:?}"
            );
        }
    }

    #[test]
    fn the_types_that_carry_most_tracking_traffic_stay_distinct() {
        assert_eq!(adblock_request_type(&Destination::Script), SCRIPT);
        assert_eq!(adblock_request_type(&Destination::Worker), SCRIPT);
        assert_eq!(adblock_request_type(&Destination::Report), PING);
        assert_eq!(adblock_request_type(&Destination::IFrame), SUBDOCUMENT);
        assert_eq!(adblock_request_type(&Destination::Image), IMAGE);
        assert_eq!(adblock_request_type(&Destination::Document), DOCUMENT);
    }

    #[test]
    fn only_real_pages_have_a_host_to_pause_on() {
        assert_eq!(
            host_of("https://News.Example.ORG/article?x=1").as_deref(),
            Some("news.example.org")
        );
        assert_eq!(
            host_of("http://localhost:8080/").as_deref(),
            Some("localhost")
        );
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("not a url"), None);
        assert_eq!(host_of("about:blank"), None);
        assert_eq!(host_of("data:text/html,hi"), None);
    }

    #[test]
    fn stats_count_blocks_without_losing_any() {
        let stats = ShieldStats::new();
        assert_eq!(stats.blocked(), 0);
        for _ in 0..250 {
            stats.record_block();
        }
        assert_eq!(stats.blocked(), 250);
    }
}
