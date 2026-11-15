//! Upstream filter lists: what to fetch, how to cache it, and what counts as a
//! usable list.
//!
//! This module does no HTTP. Fetching happens behind [`Fetcher`], which the
//! browser implements, so validation, conditional requests, cache layout and the
//! per-source replace-or-keep policy here are testable without a network and
//! without pulling a TLS stack into a crate whose only job is rule matching.
//!
//! Two invariants shape the design:
//!
//! - **Nothing is fetched unless the user asked.** A default network call at
//!   startup is forbidden by the privacy invariants, so lists are downloaded on
//!   an explicit action and read back from disk on every later start.
//! - **A failed or bogus update never lowers protection.** A source that errors,
//!   is refused by validation, or exceeds the size cap keeps its previous cache
//!   and its previous rules.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A filter list the browser knows how to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSource {
    /// Stable id, used as the cache file stem. Lowercase ASCII, no separators
    /// other than `-`.
    pub id: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    /// Where the list's own project lives, for attribution in the UI.
    pub homepage: &'static str,
    /// Licence terms the list is used under. Shown next to the list so the
    /// attribution travels with the rules it covers.
    pub licence: &'static str,
}

/// The lists offered by "update filter lists now". EasyList (ads), EasyPrivacy
/// (tracking beacons) and uBlock filters, all plain-text ABP syntax.
pub const DEFAULT_SOURCES: [FilterSource; 3] = [
    FilterSource {
        id: "easylist",
        title: "EasyList",
        url: "https://easylist.to/easylist/easylist.txt",
        homepage: "https://easylist.to/",
        licence: "https://easylist.to/pages/licence.html",
    },
    FilterSource {
        id: "easyprivacy",
        title: "EasyPrivacy",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        homepage: "https://easylist.to/",
        licence: "https://easylist.to/pages/licence.html",
    },
    FilterSource {
        id: "ublock-filters",
        title: "uBlock filters",
        url: "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/filters.txt",
        homepage: "https://github.com/uBlockOrigin/uAssets",
        licence: "https://github.com/uBlockOrigin/uAssets/blob/master/LICENSE",
    },
];

/// Refuse a response larger than this. The three default lists are about 4 MB
/// together; anything past 24 MB is not a filter list and must not be buffered.
pub const MAX_LIST_BYTES: u64 = 24 * 1024 * 1024;

/// A usable upstream list has at least this many rule lines. Guards against an
/// error page or a truncated response becoming the active ruleset.
pub const MIN_RULE_LINES: usize = 100;

/// Why a fetch did not produce a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Transport or TLS failure, with the client's message.
    Transport(String),
    /// Non-success HTTP status.
    Status(u16),
    /// The body exceeded [`MAX_LIST_BYTES`].
    TooLarge,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Transport(message) => write!(f, "network error: {message}"),
            FetchError::Status(status) => write!(f, "server returned HTTP {status}"),
            FetchError::TooLarge => {
                write!(f, "list is larger than the {} byte cap", MAX_LIST_BYTES)
            }
        }
    }
}

/// Result of one conditional request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// The server said our copy is current (304, or `! Expires:` still valid and
    /// the fetcher chose not to re-download).
    NotModified,
    Updated {
        text: String,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

/// One conditional GET for a list. Implementations must send `If-None-Match`
/// and `If-Modified-Since` when the cache holds them, and must return
/// [`FetchError::TooLarge`] rather than buffering an oversized body.
pub trait Fetcher {
    fn fetch(
        &self,
        source: &FilterSource,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<FetchOutcome, FetchError>;
}

/// What the cache remembers about a downloaded list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMeta {
    pub source_id: String,
    pub title: String,
    pub url: String,
    /// When this body was fetched, unix seconds.
    pub fetched_at_unix: i64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub bytes: u64,
}

/// A cached list: the body plus what we know about where it came from.
#[derive(Debug, Clone)]
pub struct CachedList {
    pub meta: ListMeta,
    pub text: String,
}

/// Why a response was not accepted as a filter list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    TooFewRules {
        found: usize,
        required: usize,
    },
    /// The body exceeded [`MAX_LIST_BYTES`].
    TooLarge {
        bytes: u64,
    },
    /// The body looks like markup, which is what a captive portal or an error
    /// page returns instead of the list.
    HtmlDocument,
    /// No `[Adblock` header and no `! Title:` line: not recognisable as a list.
    NotAFilterList,
    UnsafeId(String),
    Io(String),
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::TooFewRules { found, required } => {
                write!(
                    f,
                    "only {found} rule lines, a real list has at least {required}"
                )
            }
            Rejection::TooLarge { bytes } => write!(
                f,
                "list is {bytes} bytes, over the {} byte cap",
                MAX_LIST_BYTES
            ),
            Rejection::HtmlDocument => {
                write!(f, "response was an HTML document, not a filter list")
            }
            Rejection::NotAFilterList => write!(f, "no filter list header found"),
            Rejection::UnsafeId(id) => write!(f, "list id {id:?} cannot be used as a file name"),
            Rejection::Io(error) => write!(f, "cache error: {error}"),
        }
    }
}

/// What happened to one source during an update pass.
#[derive(Debug, Clone)]
pub enum SourceAction {
    /// Fresh rules downloaded and cached; `text` is the new body.
    Updated {
        source_id: String,
        title: String,
        text: String,
        fetched_at_unix: i64,
    },
    /// Cache is still current.
    Unchanged { source_id: String, title: String },
    /// Nothing usable arrived. The previous cache and rules are untouched.
    Failed {
        source_id: String,
        title: String,
        reason: String,
    },
}

impl SourceAction {
    pub fn source_id(&self) -> &str {
        match self {
            SourceAction::Updated { source_id, .. }
            | SourceAction::Unchanged { source_id, .. }
            | SourceAction::Failed { source_id, .. } => source_id,
        }
    }
}

/// On-disk cache: one `.txt` and one `.meta.json` per list, in one directory.
///
/// Cloning is cheap (a path), so the chrome can hand one handle to an update
/// pass and keep reading the cache itself.
#[derive(Clone, Debug)]
pub struct ListStore {
    dir: PathBuf,
}

impl ListStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn body_path(&self, id: &str) -> Result<PathBuf, Rejection> {
        Ok(self.dir.join(format!("{}.txt", file_stem(id)?)))
    }

    fn meta_path(&self, id: &str) -> Result<PathBuf, Rejection> {
        Ok(self.dir.join(format!("{}.meta.json", file_stem(id)?)))
    }

    /// Read a cached list, or `None` when nothing usable is cached.
    pub fn load(&self, source: &FilterSource) -> Option<CachedList> {
        let meta: ListMeta = self.load_meta(source)?;
        let text = fs::read_to_string(self.body_path(source.id).ok()?).ok()?;
        Some(CachedList { meta, text })
    }

    /// Cached metadata only, for conditional request headers.
    pub fn load_meta(&self, source: &FilterSource) -> Option<ListMeta> {
        let raw = fs::read_to_string(self.meta_path(source.id).ok()?).ok()?;
        match serde_json::from_str::<ListMeta>(&raw) {
            Ok(meta) => Some(meta),
            Err(error) => {
                // A corrupt sidecar must not wedge updates: treat it as no cache.
                tracing::warn!(
                    "shields: ignoring unreadable cache metadata for {:?}: {error}",
                    source.id
                );
                None
            }
        }
    }

    /// Every cached list belonging to `sources`, in source order.
    pub fn load_all(&self, sources: &[FilterSource]) -> Vec<CachedList> {
        sources
            .iter()
            .filter_map(|source| self.load(source))
            .collect()
    }

    /// Write a list, replacing any previous copy.
    ///
    /// Both files are written through a temporary name and renamed, so a crash
    /// or a full disk mid-write leaves the last good list readable rather than
    /// half-overwritten.
    pub fn save(
        &self,
        source: &FilterSource,
        meta: &ListMeta,
        text: &str,
    ) -> Result<(), Rejection> {
        let body_path = self.body_path(source.id)?;
        let meta_path = self.meta_path(source.id)?;
        fs::create_dir_all(&self.dir).map_err(|e| Rejection::Io(e.to_string()))?;

        write_atomic(&body_path, text.as_bytes())?;
        let json = serde_json::to_string_pretty(meta).map_err(|e| Rejection::Io(e.to_string()))?;
        write_atomic(&meta_path, json.as_bytes())?;
        Ok(())
    }

    /// Drop a cached list (used when the user removes it).
    pub fn forget(&self, source: &FilterSource) -> Result<(), Rejection> {
        for path in [self.body_path(source.id)?, self.meta_path(source.id)?] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(Rejection::Io(error.to_string())),
            }
        }
        Ok(())
    }
}

/// Fetch every source, cache what validates, and report per source.
///
/// Existing cache entries are never removed by a failed update, so one broken
/// upstream leaves the other lists (and the previous copy of that list) intact.
pub fn update_sources(
    sources: &[FilterSource],
    fetcher: &dyn Fetcher,
    store: &ListStore,
    now_unix: i64,
) -> Vec<SourceAction> {
    let mut actions = Vec::with_capacity(sources.len());

    for source in sources {
        let cached = store.load_meta(source);
        let outcome = fetcher.fetch(
            source,
            cached.as_ref().and_then(|m| m.etag.as_deref()),
            cached.as_ref().and_then(|m| m.last_modified.as_deref()),
        );

        let action = match outcome {
            Err(error) => SourceAction::Failed {
                source_id: source.id.to_string(),
                title: source.title.to_string(),
                reason: error.to_string(),
            },
            Ok(FetchOutcome::NotModified) => SourceAction::Unchanged {
                source_id: source.id.to_string(),
                title: source.title.to_string(),
            },
            Ok(FetchOutcome::Updated {
                text,
                etag,
                last_modified,
            }) => match validate_list(source, &text) {
                Err(rejection) => SourceAction::Failed {
                    source_id: source.id.to_string(),
                    title: source.title.to_string(),
                    reason: rejection.to_string(),
                },
                Ok(()) => {
                    let meta = ListMeta {
                        source_id: source.id.to_string(),
                        title: source.title.to_string(),
                        url: source.url.to_string(),
                        fetched_at_unix: now_unix,
                        etag,
                        last_modified,
                        bytes: text.len() as u64,
                    };
                    if let Err(rejection) = store.save(source, &meta, &text) {
                        SourceAction::Failed {
                            source_id: source.id.to_string(),
                            title: source.title.to_string(),
                            reason: rejection.to_string(),
                        }
                    } else {
                        SourceAction::Updated {
                            source_id: source.id.to_string(),
                            title: source.title.to_string(),
                            text,
                            fetched_at_unix: now_unix,
                        }
                    }
                }
            },
        };

        match &action {
            SourceAction::Updated { source_id, .. } => {
                tracing::info!("shields: updated list {source_id:?}")
            }
            SourceAction::Unchanged { source_id, .. } => {
                tracing::debug!("shields: {source_id:?} unchanged")
            }
            SourceAction::Failed {
                source_id, reason, ..
            } => tracing::warn!("shields: could not update {source_id:?}: {reason}"),
        }
        actions.push(action);
    }

    actions
}

/// Is this body usable as `source`'s list?
pub fn validate_list(source: &FilterSource, text: &str) -> Result<(), Rejection> {
    file_stem(source.id)?;

    if text.len() as u64 > MAX_LIST_BYTES {
        return Err(Rejection::TooLarge {
            bytes: text.len() as u64,
        });
    }

    let lowered = text.to_ascii_lowercase();
    if lowered.contains("<html") || lowered.contains("<!doctype html") {
        return Err(Rejection::HtmlDocument);
    }

    let header =
        text.starts_with("[Adblock") || text.lines().take(40).any(|l| l.starts_with("! Title:"));
    if !header {
        return Err(Rejection::NotAFilterList);
    }

    let rules = rule_lines(text).count();
    if rules < MIN_RULE_LINES {
        return Err(Rejection::TooFewRules {
            found: rules,
            required: MIN_RULE_LINES,
        });
    }
    Ok(())
}

/// Rule lines in a list body: not blank, not a comment, not a header.
pub fn rule_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('!') && !line.starts_with('['))
}

fn file_stem(id: &str) -> Result<String, Rejection> {
    let ok = !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if ok {
        Ok(id.to_string())
    } else {
        Err(Rejection::UnsafeId(id.to_string()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Rejection> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("part")
    ));
    fs::write(&tmp, bytes).map_err(|e| Rejection::Io(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| Rejection::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// What the stub was asked, per call: which list, and which validators.
    type Seen = Vec<(String, Option<String>, Option<String>)>;
    type Responses = Vec<(String, Result<FetchOutcome, FetchError>)>;

    struct Stub {
        responses: RefCell<Responses>,
        seen: RefCell<Seen>,
    }

    impl Stub {
        fn new(responses: Vec<(&str, Result<FetchOutcome, FetchError>)>) -> Self {
            Self {
                responses: RefCell::new(
                    responses
                        .into_iter()
                        .map(|(id, r)| (id.to_string(), r))
                        .collect(),
                ),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Fetcher for Stub {
        fn fetch(
            &self,
            source: &FilterSource,
            etag: Option<&str>,
            last_modified: Option<&str>,
        ) -> Result<FetchOutcome, FetchError> {
            self.seen.borrow_mut().push((
                source.id.to_string(),
                etag.map(str::to_string),
                last_modified.map(str::to_string),
            ));
            let responses = self.responses.borrow();
            let index = responses
                .iter()
                .position(|(id, _)| *id == source.id)
                .ok_or_else(|| FetchError::Transport(format!("no stub for {}", source.id)))?;
            Ok(match &responses[index].1 {
                Ok(FetchOutcome::Updated {
                    text,
                    etag,
                    last_modified,
                }) => FetchOutcome::Updated {
                    text: text.clone(),
                    etag: etag.clone(),
                    last_modified: last_modified.clone(),
                },
                Ok(FetchOutcome::NotModified) => FetchOutcome::NotModified,
                Err(error) => return Err(error.clone()),
            })
        }
    }

    fn list(title: &str, rules: usize) -> String {
        let mut text = format!("[Adblock Plus 2.0]\n! Title: {title}\n! Expires: 4 days\n");
        for i in 0..rules {
            text.push_str(&format!("||track{i}.example.test^\n"));
        }
        text
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = format!(
            "craft-adblock-upstream-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_update_writes_the_body_and_a_readable_sidecar() {
        let store = ListStore::new(temp_dir("save"));
        let source = &DEFAULT_SOURCES[0];
        let body = list(source.title, 120);
        let fetcher = Stub::new(vec![
            (
                source.id,
                Ok(FetchOutcome::Updated {
                    text: body.clone(),
                    etag: Some("\"abc\"".into()),
                    last_modified: Some("Mon, 01 Jan 2026 00:00:00 GMT".into()),
                }),
            ),
            (DEFAULT_SOURCES[1].id, Err(FetchError::Status(404))),
            (DEFAULT_SOURCES[2].id, Err(FetchError::Status(404))),
        ]);

        let actions = update_sources(&DEFAULT_SOURCES[..1], &fetcher, &store, 1_700_000_000);
        assert!(matches!(actions[0], SourceAction::Updated { .. }));

        let cached = store.load(source).expect("the list must be cached");
        assert_eq!(cached.text, body);
        assert_eq!(cached.meta.fetched_at_unix, 1_700_000_000);
        assert_eq!(cached.meta.etag.as_deref(), Some("\"abc\""));
        assert_eq!(cached.meta.bytes, body.len() as u64);
        assert!(store.dir().join("easylist.txt").is_file());

        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn cached_validators_are_sent_on_the_next_request() {
        let dir = temp_dir("conditional");
        let store = ListStore::new(&dir);
        let source = &DEFAULT_SOURCES[0];
        let body = list(source.title, 120);

        update_sources(
            std::slice::from_ref(source),
            &Stub::new(vec![(
                source.id,
                Ok(FetchOutcome::Updated {
                    text: body.clone(),
                    etag: Some("\"v1\"".into()),
                    last_modified: Some("v1-time".into()),
                }),
            )]),
            &store,
            1,
        );

        let fetcher = Stub::new(vec![(source.id, Ok(FetchOutcome::NotModified))]);
        let actions = update_sources(std::slice::from_ref(source), &fetcher, &store, 2);
        assert!(matches!(actions[0], SourceAction::Unchanged { .. }));
        assert_eq!(
            *fetcher.seen.borrow(),
            vec![(
                source.id.to_string(),
                Some("\"v1\"".to_string()),
                Some("v1-time".to_string())
            )],
            "the second request must be conditional"
        );

        // An unchanged report must not have cleared the cache.
        assert_eq!(store.load(source).expect("still cached").text, body);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_or_bogus_response_keeps_the_previous_rules() {
        let dir = temp_dir("keep");
        let store = ListStore::new(&dir);
        let source = &DEFAULT_SOURCES[0];
        let good = list(source.title, 120);
        update_sources(
            std::slice::from_ref(source),
            &Stub::new(vec![(
                source.id,
                Ok(FetchOutcome::Updated {
                    text: good.clone(),
                    etag: None,
                    last_modified: None,
                }),
            )]),
            &store,
            1,
        );

        // A captive portal answering 200 with HTML.
        let actions = update_sources(
            std::slice::from_ref(source),
            &Stub::new(vec![(
                source.id,
                Ok(FetchOutcome::Updated {
                    text: "<html><body>Sign in to the wifi</body></html>".to_string(),
                    etag: None,
                    last_modified: None,
                }),
            )]),
            &store,
            2,
        );
        assert!(matches!(actions[0], SourceAction::Failed { .. }));
        assert_eq!(store.load(source).expect("cache intact").text, good);

        // A truncated list (few rules) is refused for the same reason.
        let actions = update_sources(
            std::slice::from_ref(source),
            &Stub::new(vec![(
                source.id,
                Ok(FetchOutcome::Updated {
                    text: list(source.title, 3),
                    etag: None,
                    last_modified: None,
                }),
            )]),
            &store,
            3,
        );
        assert!(matches!(actions[0], SourceAction::Failed { .. }));
        assert_eq!(store.load(source).expect("cache intact").text, good);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_broken_source_does_not_stop_the_others() {
        let dir = temp_dir("partial");
        let store = ListStore::new(&dir);
        let fetcher = Stub::new(vec![
            (
                "easylist",
                Err(FetchError::Transport("connection reset".into())),
            ),
            (
                "easyprivacy",
                Ok(FetchOutcome::Updated {
                    text: list("EasyPrivacy", 200),
                    etag: None,
                    last_modified: None,
                }),
            ),
            (
                "ublock-filters",
                Ok(FetchOutcome::Updated {
                    text: list("uBlock filters", 150),
                    etag: None,
                    last_modified: None,
                }),
            ),
        ]);

        let actions = update_sources(&DEFAULT_SOURCES, &fetcher, &store, 10);
        assert!(matches!(actions[0], SourceAction::Failed { .. }));
        assert!(matches!(actions[1], SourceAction::Updated { .. }));
        assert!(matches!(actions[2], SourceAction::Updated { .. }));
        let cached = store.load_all(&DEFAULT_SOURCES);
        assert_eq!(cached.len(), 2, "only the two good lists are on disk");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_rejects_the_things_a_server_mistake_looks_like() {
        let source = &DEFAULT_SOURCES[0];
        assert_eq!(
            validate_list(source, "welcome to nginx\n"),
            Err(Rejection::NotAFilterList)
        );
        assert_eq!(
            validate_list(source, "[Adblock Plus 2.0]\n||only.example^\n"),
            Err(Rejection::TooFewRules {
                found: 1,
                required: MIN_RULE_LINES
            })
        );
        assert_eq!(
            validate_list(source, "<!DOCTYPE html><html><body>x</body></html>"),
            Err(Rejection::HtmlDocument)
        );
        assert!(validate_list(source, &list("ok", MIN_RULE_LINES)).is_ok());
    }

    #[test]
    fn a_list_id_that_is_not_a_safe_file_name_is_refused() {
        // The id becomes a path component, so it must never carry separators.
        let dir = temp_dir("safety");
        let store = ListStore::new(&dir);
        let source = FilterSource {
            id: "../../etc/passwd",
            title: "evil",
            url: "https://example.test/evil.txt",
            homepage: "https://example.test",
            licence: "https://example.test",
        };
        let meta = ListMeta {
            source_id: source.id.into(),
            title: source.title.into(),
            url: source.url.into(),
            fetched_at_unix: 0,
            etag: None,
            last_modified: None,
            bytes: 0,
        };
        assert!(matches!(
            store.save(&source, &meta, "x"),
            Err(Rejection::UnsafeId(_))
        ));
        assert_eq!(
            validate_list(&source, &list("x", 200)),
            Err(Rejection::UnsafeId(source.id.to_string()))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_sidecar_is_ignored_rather_than_fatal() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("easylist.meta.json"), "{ not json").unwrap();
        let store = ListStore::new(&dir);
        assert!(store.load_meta(&DEFAULT_SOURCES[0]).is_none());

        let actions = update_sources(
            std::slice::from_ref(&DEFAULT_SOURCES[0]),
            &Stub::new(vec![(
                "easylist",
                Ok(FetchOutcome::Updated {
                    text: list("EasyList", 120),
                    etag: None,
                    last_modified: None,
                }),
            )]),
            &store,
            5,
        );
        assert!(matches!(actions[0], SourceAction::Updated { .. }));
        assert!(store.load(&DEFAULT_SOURCES[0]).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn forget_removes_both_files_and_tolerates_absence() {
        let dir = temp_dir("forget");
        let store = ListStore::new(&dir);
        let source = &DEFAULT_SOURCES[0];
        let meta = ListMeta {
            source_id: source.id.into(),
            title: source.title.into(),
            url: source.url.into(),
            fetched_at_unix: 0,
            etag: None,
            last_modified: None,
            bytes: 3,
        };
        store.save(source, &meta, "||x.example^\n").unwrap();
        assert!(store.load(source).is_some());
        store.forget(source).unwrap();
        assert!(store.load(source).is_none());
        store.forget(source).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_sources_are_real_and_distinguishable() {
        let ids: Vec<&str> = DEFAULT_SOURCES.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["easylist", "easyprivacy", "ublock-filters"]);
        for source in DEFAULT_SOURCES.iter() {
            assert!(
                source.url.starts_with("https://"),
                "{} must be fetched over TLS",
                source.id
            );
            assert!(!source.title.is_empty());
            assert!(!source.licence.is_empty());
            assert!(validate_list(source, &list(source.title, MIN_RULE_LINES)).is_ok());
        }
    }
}
