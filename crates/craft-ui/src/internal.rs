//! Internal `craft://` pages, served by a protocol handler registered on the engine.
//!
//! Two constraints shape this module:
//!
//! - The handler is `Send + Sync` and runs on a network thread, while
//!   `AdblockManager` is neither (its engine holds a `RefCell`). So the chrome
//!   publishes a plain-data [`ShieldStatus`] snapshot into a mutex and the handler
//!   renders from that. A page is therefore as fresh as the last publish, and
//!   reloading re-renders without the network thread touching engine state.
//! - There is no JavaScript-to-embedder bridge, so these pages are informational.
//!   Controls live in the egui chrome; a button here would be a dead control,
//!   which is what this module exists to avoid.
//!
//! Every response carries `Content-Security-Policy: default-src 'none'` with only
//! inline styles allowed, so an internal page reaching the network is impossible
//! rather than merely unintended.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use http::HeaderValue;
use http::header::{CONTENT_TYPE, HeaderMap, HeaderName};
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, ProtocolHandler, ProtocolRegistry, Request,
    ResourceFetchTiming, Response, ResponseBody,
};
use tracing::error;
use url::Url;

/// The scheme these pages answer on. Not in the engine's forbidden list, unlike
/// `http`, `https`, `chrome` and `about`.
pub const SCHEME: &str = "craft";

/// The page a new tab opens on.
pub const WELCOME_URL: &str = "craft://welcome";
/// Tracking protection status, reachable from the menu.
pub const SHIELDS_URL: &str = "craft://shields";
/// What can be controlled and what cannot yet.
pub const SETTINGS_URL: &str = "craft://settings";

/// Shared handle: the chrome writes, the handler reads.
pub type StatusHandle = Arc<Mutex<ShieldStatus>>;

/// A thread-safe snapshot of what the shields are doing, published by the chrome
/// for the status page to render. Plain data only, by necessity: nothing here may
/// reach into the engine or the matcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShieldStatus {
    /// The global switch, as the chrome last set it.
    pub enabled: bool,
    /// Whether the page in the active tab is currently exempt.
    pub page_exempt: bool,
    /// Host of the page in the active tab, empty when there is none.
    pub active_host: String,
    /// Requests refused since launch.
    pub blocked: u64,
    /// Rules loaded, per source.
    pub groups: Vec<GroupStatus>,
    /// Sites the user paused protection for.
    pub exceptions: Vec<String>,
    /// Outcome lines from the last update pass.
    pub last_pass: Vec<String>,
    /// Where downloaded lists are cached on disk.
    pub lists_dir: String,
    /// Total rules, so this page cannot disagree with the tooltip.
    pub rule_total: usize,
}

/// One rule source, as shown on the status page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupStatus {
    pub title: String,
    pub rules: usize,
    pub rejected: usize,
    /// Pre-rendered age ("ships with the browser", "2 d ago"), so this module
    /// does not need a clock and a stale string is at least a consistent one.
    pub age: String,
    /// Attribution for a downloaded list, so its licence travels with the rules.
    pub source: Option<SourceInfo>,
}

/// Homepage and licence terms of a downloaded list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub homepage: String,
    pub licence: String,
}

impl Default for ShieldStatus {
    fn default() -> Self {
        Self {
            // Filtering is on unless the chrome says otherwise, matching
            // `AdblockManager::new`.
            enabled: true,
            page_exempt: false,
            active_host: String::new(),
            blocked: 0,
            groups: Vec::new(),
            exceptions: Vec::new(),
            last_pass: Vec::new(),
            lists_dir: String::new(),
            rule_total: 0,
        }
    }
}

pub fn new_handle() -> StatusHandle {
    Arc::new(Mutex::new(ShieldStatus::default()))
}

/// Publish a snapshot for the pages to render. A poisoned lock must not take the
/// browser down, so the failure is logged and the next frame retries.
pub fn publish(handle: &StatusHandle, status: ShieldStatus) {
    match handle.lock() {
        Ok(mut guard) => *guard = status,
        Err(error) => error!("internal pages: could not publish shield status: {error}"),
    }
}

/// Serves `craft://` documents.
pub struct CraftPages {
    status: StatusHandle,
}

impl CraftPages {
    pub fn new(status: StatusHandle) -> Self {
        Self { status }
    }

    /// A registry carrying just the internal scheme, ready to merge into the
    /// engine's own protocols.
    pub fn registry(status: StatusHandle) -> ProtocolRegistry {
        let mut registry = ProtocolRegistry::default();
        if let Err(err) = registry.register(SCHEME, Self::new(status)) {
            // Only a duplicate or forbidden scheme can fail, and `craft` is
            // neither. Losing the internal pages is bad; killing the browser is
            // worse, so this is reported and browsing continues.
            error!("internal pages: could not register the craft:// scheme: {err:?}");
            return ProtocolRegistry::default();
        }
        registry
    }

    fn read_status(&self) -> ShieldStatus {
        match self.status.lock() {
            Ok(guard) => guard.clone(),
            Err(error) => {
                error!("internal pages: status lock poisoned, rendering defaults: {error}");
                ShieldStatus::default()
            }
        }
    }
}

impl ProtocolHandler for CraftPages {
    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let route = route_of(url.as_str());
        let status = self.read_status();
        let body = render_page(&route, &status).unwrap_or_else(|| not_found(&route));

        let mut response = Response::new(url, ResourceFetchTiming::new(request.timing_type()));
        *response.body.lock() = ResponseBody::Done(body.into_bytes());
        response.status = HttpStatus::new_raw(200, b"OK".to_vec());

        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str("text/html; charset=utf-8") {
            headers.insert(CONTENT_TYPE, value);
        }
        if let Ok(value) = HeaderValue::from_str("default-src 'none'; style-src 'unsafe-inline'") {
            headers.insert(HeaderName::from_static("content-security-policy"), value);
        }
        response.headers = headers;

        Box::pin(std::future::ready(response))
    }

    /// These pages belong to the browser; no document should `fetch()` them and
    /// read the result.
    fn is_fetchable(&self) -> bool {
        false
    }
}

/// Map a `craft://` URL to a page route.
///
/// `craft://welcome` parses as host `welcome` with an empty path, while
/// `craft://settings/shields` is host `settings` plus path `/shields`, so both
/// parts are needed.
pub fn route_of(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return String::new();
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let path = parsed.path().trim_matches('/').to_ascii_lowercase();
    let joined = match (host.as_str(), path.as_str()) {
        ("", "") => String::new(),
        ("", only_path) => only_path.to_string(),
        (only_host, "") => only_host.to_string(),
        (h, p) => format!("{h}/{p}"),
    };
    // Aliases only: anything else passes through and 404s if no page matches it.
    match joined.as_str() {
        "" | "home" | "newtab" => "welcome".to_string(),
        "shield" | "protection" | "settings/shields" => "shields".to_string(),
        other => other.to_string(),
    }
}

/// Render a known route, or `None` so the caller can produce a 404.
pub fn render_page(route: &str, status: &ShieldStatus) -> Option<String> {
    match route {
        "welcome" => Some(welcome(status)),
        "shields" => Some(shields(status)),
        "settings" => Some(settings(status)),
        _ => None,
    }
}

fn welcome(status: &ShieldStatus) -> String {
    let shield_line = if status.enabled {
        format!(
            "Tracking protection is on, with {} rules loaded and {} requests refused since launch.",
            status.rule_total, status.blocked
        )
    } else {
        "Tracking protection is paused for every site.".to_string()
    };

    page(
        "CraftBrowsing",
        &format!(
            r#"<h1>CraftBrowsing</h1>
<p>A privacy-focused browser. Everything above the rendering engine is Rust: the chrome, tabs, filtering and transfers.</p>
<p class="note">{shield_line}</p>
<h2>Pages here</h2>
<ul>
  <li><a href="craft://shields">Tracking protection status</a>: which rule lists are loaded, and what they have refused.</li>
  <li><a href="craft://settings">Settings</a>: what you can control, and what you cannot yet.</li>
</ul>
<h2>Engine</h2>
<p>This build renders with Servo. The project has accepted a decision to move to Blink/Chromium (CEF) as the primary engine for site compatibility while keeping Servo as a re-evaluable backend behind an engine boundary. That swap is a planned phase, not something this build does.</p>
<h2>About this page</h2>
<p>Internal pages are served from memory. Their response forbids scripts and subresources, so nothing here can reach the network. Interactive controls are deliberately absent: a page like this cannot talk back to the browser, so the menu carries them.</p>"#
        ),
    )
}

fn shields(status: &ShieldStatus) -> String {
    let rows = if status.groups.is_empty() {
        r#"<tr><td colspan="4">No rule sources are loaded.</td></tr>"#.to_string()
    } else {
        status
            .groups
            .iter()
            .map(|group| {
                let refused = if group.rejected > 0 {
                    format!("{} refused by the parser", group.rejected)
                } else {
                    "all lines compiled".to_string()
                };
                let attribution = match &group.source {
                    Some(source) => format!("<div class=\"sub\">{}</div>", esc(&source.homepage),),
                    None => String::new(),
                };
                format!(
                    "<tr><td>{}{attribution}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(&group.title),
                    group.rules,
                    refused,
                    esc(&group.age),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let page_state = if status.active_host.is_empty() {
        "No page is open, so there is nothing to pause for a site.".to_string()
    } else if status.page_exempt {
        format!(
            "Protection is <strong>paused for {}</strong>. Use the menu to resume it.",
            esc(&status.active_host)
        )
    } else {
        format!(
            "Protection is active on {}. The menu can pause it for this site alone.",
            esc(&status.active_host)
        )
    };

    let exceptions = if status.exceptions.is_empty() {
        "<p>No sites are paused.</p>".to_string()
    } else {
        let items = status
            .exceptions
            .iter()
            .map(|site| format!("<li>{}</li>", esc(site)))
            .collect::<Vec<_>>()
            .join("");
        format!("<p>Paused sites, covering each one and its subdomains:</p><ul>{items}</ul>")
    };

    let pass = if status.last_pass.is_empty() {
        String::new()
    } else {
        let items = status
            .last_pass
            .iter()
            .map(|line| format!("<li>{}</li>", esc(line)))
            .collect::<Vec<_>>()
            .join("");
        format!("<h2>Last update pass</h2><ul>{items}</ul>")
    };

    let state = if status.enabled { "on" } else { "paused" };

    page(
        "Tracking protection",
        &format!(
            r#"<h1>Tracking protection</h1>
<p>Global state: <strong>{state}</strong>. Rules loaded: <strong>{rules}</strong>. Requests refused since launch: <strong>{blocked}</strong>.</p>
<h2>Rules loaded</h2>
<table>
  <thead><tr><th>Source</th><th>Rules</th><th>Parser</th><th>Age</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
<h2>This site</h2>
<p>{page_state}</p>
{exceptions}
{pass}
<h2>Where the lists live</h2>
<p>Downloaded lists are cached at <code>{lists}</code>. Nothing fetches on its own: the menu's "Update filter lists now" is the only thing that reaches the network for rules.</p>
<h2>What this does not do yet</h2>
<ul>
  <li><strong>Element hiding.</strong> Cosmetic rules from the upstream lists are parsed but never applied to the page, so an ad can still leave its layout hole.</li>
  <li><strong>Per-list control.</strong> You cannot disable one list, import your own, or get a warning when a cached list has gone stale.</li>
  <li><strong>Blocked navigation.</strong> A top-level document is never refused, so a tracker's own homepage still loads.</li>
</ul>"#,
            blocked = status.blocked,
            rules = status.rule_total,
            lists = esc(&status.lists_dir),
        ),
    )
}

fn settings(status: &ShieldStatus) -> String {
    let available = [
        (
            "Pause or resume tracking protection everywhere",
            "the shield in the address bar",
        ),
        (
            "Pause tracking protection for one site",
            "the menu, on the site's own row",
        ),
        ("Download filter list updates", "the menu"),
        (
            "See which rules are loaded and what they refused",
            "the menu, and craft://shields",
        ),
        ("Open a tab in a named container", "the menu"),
    ];
    let not_yet = [
        (
            "Per-container cookie and storage isolation",
            "container identities show on tabs, but the browser runs one shared page and does not use the partitioned storage layer, so nothing is isolated yet",
        ),
        (
            "HTTPS-Only mode and its interstitial",
            "no request path implements it",
        ),
        (
            "DNS-over-HTTPS or DNS-over-TLS selection",
            "no resolver configuration exists",
        ),
        (
            "Anti-fingerprinting (canvas, WebGL, audio, user agent)",
            "not implemented anywhere",
        ),
        (
            "A full privacy dashboard",
            "the settings surfaces named in the design documents do not exist yet; this page and craft://shields are what is real",
        ),
    ];

    let bullet = |(what, why): (&str, &str)| {
        format!(
            "<li><strong>{}</strong> &mdash; {}</li>",
            esc(what),
            esc(why)
        )
    };
    let available_items = available
        .iter()
        .map(|item| bullet(*item))
        .collect::<Vec<_>>()
        .join("");
    let not_yet_items = not_yet
        .iter()
        .map(|item| bullet(*item))
        .collect::<Vec<_>>()
        .join("");

    page(
        "Settings",
        &format!(
            r#"<h1>Settings</h1>
<p>This page lists what the running browser actually controls, because much of the surrounding documentation describes a design rather than a feature. Rules loaded right now: {rules}.</p>
<h2>You can do this</h2>
<ul>{available_items}</ul>
<h2>Not implemented yet</h2>
<ul>{not_yet_items}</ul>
<p><a href="craft://shields">Tracking protection status</a></p>"#,
            rules = status.rule_total
        ),
    )
}

fn not_found(route: &str) -> String {
    page(
        "No such page",
        &format!(
            r#"<h1>No page at craft://{route}</h1>
<p>Internal pages are a small, fixed set. These are the ones that exist:</p>
<ul>
  <li><a href="craft://welcome">craft://welcome</a> &mdash; start page</li>
  <li><a href="craft://shields">craft://shields</a> &mdash; tracking protection status</li>
  <li><a href="craft://settings">craft://settings</a> &mdash; what you can control, and what you cannot yet</li>
</ul>"#
        ),
    )
}

fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: light }}
  body {{ font: 15px/1.55 system-ui, sans-serif; margin: 0 auto; padding: 28px 20px 56px; max-width: 46rem; color: #16161a; background: #fff }}
  h1 {{ font-size: 1.5rem; margin: 0 0 .6rem }}
  h2 {{ font-size: 1.05rem; margin: 1.8rem 0 .5rem; color: #4a4a55 }}
  p, li {{ color: #2c2c33 }}
  a {{ color: #0053e6 }}
  code {{ font: .9em ui-monospace, monospace; background: #f2f2f5; padding: .1em .35em; border-radius: 3px; word-break: break-all }}
  .note {{ background: #f2f6ff; border-left: 3px solid #0053e6; padding: .6rem .8rem; border-radius: 0 4px 4px 0 }}
  table {{ border-collapse: collapse; width: 100%; font-size: .95rem }}
  th, td {{ text-align: left; padding: .45rem .5rem; border-bottom: 1px solid #e6e6ea; vertical-align: top }}
  th {{ color: #4a4a55; font-weight: 600 }}
  .sub {{ color: #6c6c77; font-size: .8rem }}
</style>
</head>
<body>
{body}
</body>
</html>"#,
        title = esc(title)
    )
}

/// Escape text destined for HTML.
///
/// Not decoration: list titles arrive from remote `! Title:` headers, and hosts
/// and pass outcome lines come from the network too, all rendered into a
/// privileged `craft://` document.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ShieldStatus {
        ShieldStatus {
            enabled: true,
            page_exempt: false,
            active_host: "news.example.org".to_string(),
            blocked: 17,
            groups: vec![
                GroupStatus {
                    title: "Bundled rules".to_string(),
                    rules: 60,
                    rejected: 0,
                    age: "ships with the browser".to_string(),
                    source: None,
                },
                GroupStatus {
                    title: "EasyList".to_string(),
                    rules: 68_000,
                    rejected: 2_143,
                    age: "2 d ago".to_string(),
                    source: Some(SourceInfo {
                        homepage: "https://easylist.to/".to_string(),
                        licence: "https://easylist.to/pages/licence.html".to_string(),
                    }),
                },
            ],
            exceptions: vec!["bank.example".to_string()],
            last_pass: vec!["EasyList: still current".to_string()],
            lists_dir: "/home/test/.local/share/craftbrowsing/filterlists".to_string(),
            rule_total: 68_060,
        }
    }

    /// The URL constants and the registered scheme must not drift apart, or a
    /// menu item would load a scheme nothing answers on.
    #[test]
    fn the_page_urls_use_the_registered_scheme() {
        for url in [WELCOME_URL, SHIELDS_URL, SETTINGS_URL] {
            assert!(
                url.starts_with(&format!("{SCHEME}://")),
                "{url} is not on the {SCHEME}:// scheme"
            );
            let route = route_of(url);
            assert!(
                render_page(&route, &ShieldStatus::default()).is_some(),
                "{url} routes to {route:?}, which has no page"
            );
        }
    }

    #[test]
    fn urls_normalise_to_routes() {
        assert_eq!(route_of("craft://welcome"), "welcome");
        assert_eq!(route_of("craft://"), "welcome");
        assert_eq!(route_of("craft://home"), "welcome");
        assert_eq!(route_of("craft://shields"), "shields");
        assert_eq!(route_of("craft://protection"), "shields");
        assert_eq!(route_of("craft://settings/shields"), "shields");
        assert_eq!(route_of("craft://settings"), "settings");
        assert_eq!(route_of("craft://WELCOME"), "welcome");
        assert_eq!(route_of("craft://nope"), "nope");
        // Something that is not a URL at all must not panic.
        assert_eq!(route_of("nonsense"), "");
    }

    #[test]
    fn the_known_routes_render_and_others_do_not() {
        let status = sample();
        for route in ["welcome", "shields", "settings"] {
            let html = render_page(route, &status).unwrap_or_else(|| panic!("{route} must render"));
            assert!(
                html.starts_with("<!doctype html>"),
                "{route} is missing a doctype"
            );
            assert!(
                html.contains("<title>") && html.contains("</title>"),
                "{route} has no title"
            );
        }
        assert!(render_page("nope", &status).is_none());
    }

    #[test]
    fn the_status_page_shows_the_real_numbers() {
        let html = render_page("shields", &sample()).expect("renders");
        assert!(html.contains("68000"));
        assert!(html.contains("68060"), "total rules should be shown");
        assert!(html.contains("17"), "refused requests should be shown");
        assert!(html.contains("2143"), "refusals per list");
        assert!(html.contains("ships with the browser"));
        assert!(html.contains("2 d ago"));
        assert!(html.contains("news.example.org"));
        assert!(
            html.contains("bank.example"),
            "paused sites should be listed"
        );
        assert!(
            html.contains("still current"),
            "last pass lines should appear"
        );
        assert!(
            html.contains("craftbrowsing/filterlists"),
            "cache location should be named"
        );
        assert!(
            html.contains("Element hiding"),
            "the gaps should be stated on the page"
        );
    }

    /// A remote `! Title:` header is attacker-reachable text rendered into a
    /// privileged page, so it must not be able to introduce markup.
    #[test]
    fn markup_from_a_list_title_cannot_escape_its_cell() {
        let mut status = sample();
        status.groups[1].title = "<script>alert(1)</script>".to_string();
        status
            .exceptions
            .push("<img src=x onerror=alert(2)>".to_string());
        status.active_host = "<b>host</b>".to_string();
        status
            .last_pass
            .push("</table><script>x</script>".to_string());

        let html = render_page("shields", &status).expect("renders");
        assert!(
            !html.contains("<script>alert"),
            "raw script must not reach the output"
        );
        assert!(
            !html.contains("<img src=x"),
            "raw image tag must not reach the output"
        );
        assert!(
            html.contains("&lt;script&gt;"),
            "the value should still be readable"
        );
        assert!(html.contains("&lt;b&gt;host&lt;/b&gt;"));
    }

    #[test]
    fn the_settings_page_separates_real_from_missing() {
        let html = render_page("settings", &sample()).expect("renders");
        assert!(html.contains("You can do this"));
        assert!(html.contains("Not implemented yet"));
        assert!(html.contains("Per-container cookie and storage isolation"));
        assert!(html.contains("68060"));
    }

    #[test]
    fn internal_pages_load_no_external_resources() {
        for route in ["welcome", "shields", "settings"] {
            let html = render_page(route, &sample()).expect("renders");
            let lowered = html.to_ascii_lowercase();
            for forbidden in [
                "<script", "<img", "<link", "<iframe", "url(http", "src=http", "srcset", "@import",
            ] {
                assert!(
                    !lowered.contains(forbidden),
                    "{route} contains {forbidden:?}, so an internal page could fetch something"
                );
            }
        }
    }

    #[test]
    fn an_unknown_route_still_tells_you_what_exists() {
        let html = not_found("whatever");
        assert!(html.contains("craft://whatever"));
        for known in ["craft://welcome", "craft://shields", "craft://settings"] {
            assert!(html.contains(known), "{known} should be listed");
        }
    }

    #[test]
    fn escaping_covers_the_characters_that_matter() {
        assert_eq!(
            esc("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(esc("plain"), "plain");
        assert_eq!(esc(""), "");
    }

    #[test]
    fn the_default_status_matches_a_fresh_manager() {
        let default = ShieldStatus::default();
        assert!(
            default.enabled,
            "filtering is on unless the chrome says otherwise"
        );
        assert_eq!(default.blocked, 0);
        assert!(default.groups.is_empty());
        assert!(default.active_host.is_empty());

        let html = render_page("shields", &default).expect("renders with no data");
        assert!(html.contains("No rule sources are loaded."));
        assert!(html.contains("No sites are paused."));
        assert!(html.contains("No page is open"));
        // No last-pass section when nothing has happened.
        assert!(!html.contains("Last update pass"));
    }

    #[test]
    fn publishing_updates_what_the_pages_would_say() {
        let handle = new_handle();
        publish(&handle, sample());
        let guard = handle.lock().unwrap();
        assert_eq!(guard.rule_total, 68_060);
        assert_eq!(*guard, sample());
    }
}
