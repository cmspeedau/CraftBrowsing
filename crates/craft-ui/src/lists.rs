//! Upstream filter list updates: the HTTP client, the worker thread, and how the
//! chrome sees progress.
//!
//! The policy (what to fetch, validation, caching, keep-the-old-rules-on-failure)
//! lives in `craft_adblock::upstream`. This module is only the parts that belong
//! to the browser: a real client, a thread, and the channel back to the UI.
//!
//! Parsing and compiling happen on the UI thread, not the worker, because
//! `adblock::Engine` is neither `Send` nor `Sync`. That is a worthwhile trade:
//! measured on this machine, 141k rule lines parse in about 120 ms and compile in
//! about 210 ms, so one rebuild after a batch of lists costs roughly 0.3 s of
//! interface stall at the moment the user asked for an update. The network part,
//! which is seconds and unpredictable, stays off the UI thread.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use craft_adblock::upstream::{
    DEFAULT_SOURCES, FetchError, FetchOutcome, Fetcher, FilterSource, ListStore, MAX_LIST_BYTES,
    SourceAction, update_sources,
};
use tracing::{info, warn};

/// Identifies the browser to the list servers. Static on purpose: no machine,
/// account or session information, nothing to correlate.
const USER_AGENT: &str = concat!(
    "CraftBrowsing/",
    env!("CARGO_PKG_VERSION"),
    " (filter list update)"
);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// [`Fetcher`] over HTTPS using the same reqwest build the transfer engine uses.
pub struct HttpFetcher {
    client: reqwest::blocking::Client,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, FetchError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // The URLs here are compile-time constants, not user input, so
            // redirects are about CDN layout rather than SSRF. Three hops is
            // enough for the hosts that serve these lists and bounds the chase.
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|error| FetchError::Transport(error.to_string()))?;
        Ok(Self { client })
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(
        &self,
        source: &FilterSource,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<FetchOutcome, FetchError> {
        let mut request = self.client.get(source.url);
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
        }

        let response = request
            .send()
            .map_err(|error| FetchError::Transport(error.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(FetchOutcome::NotModified);
        }
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }

        // Check the advertised length before reading, then count bytes as they
        // arrive: a server that lies about `Content-Length` must not be able to
        // make the browser buffer an unbounded response.
        if let Some(length) = response.content_length()
            && length > MAX_LIST_BYTES
        {
            return Err(FetchError::TooLarge);
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let mut body: Vec<u8> = Vec::new();
        let mut reader = response;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            use std::io::Read;
            let read = reader
                .read(&mut chunk)
                .map_err(|error| FetchError::Transport(error.to_string()))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
            if body.len() as u64 > MAX_LIST_BYTES {
                return Err(FetchError::TooLarge);
            }
        }

        let text = String::from_utf8(body)
            .map_err(|error| FetchError::Transport(format!("list is not valid UTF-8: {error}")))?;
        Ok(FetchOutcome::Updated {
            text,
            etag,
            last_modified: modified,
        })
    }
}

/// One message from the update thread to the chrome.
#[derive(Debug)]
pub enum UpdateEvent {
    /// One source settled.
    Source(SourceAction),
    /// No more sources will arrive; the chrome applies what was updated.
    Finished {
        updated: usize,
        unchanged: usize,
        failed: usize,
    },
}

/// Start one update pass over [`DEFAULT_SOURCES`], returning the receiver the
/// chrome drains each frame.
///
/// Returns `None` only when the thread could not start. The caller is responsible
/// for running one pass at a time, so two clicks cannot race over the cache.
pub fn spawn_update(store: ListStore) -> Option<Receiver<UpdateEvent>> {
    let (sender, receiver) = channel();
    let started = thread::Builder::new()
        .name("filter-list-updater".to_string())
        .spawn(move || run_update(store, sender));
    match started {
        Ok(_) => Some(receiver),
        Err(error) => {
            warn!("shields: could not start the filter list updater: {error}");
            None
        }
    }
}

fn run_update(store: ListStore, sender: Sender<UpdateEvent>) {
    info!(
        "shields: updating {} filter lists from upstream",
        DEFAULT_SOURCES.len()
    );
    let fetcher = match HttpFetcher::new() {
        Ok(fetcher) => fetcher,
        Err(error) => {
            // Report every source as failed rather than leaving the chrome
            // waiting for events that will never come.
            for source in DEFAULT_SOURCES.iter() {
                let _ = sender.send(UpdateEvent::Source(SourceAction::Failed {
                    source_id: source.id.to_string(),
                    title: source.title.to_string(),
                    reason: error.to_string(),
                }));
            }
            let _ = sender.send(UpdateEvent::Finished {
                updated: 0,
                unchanged: 0,
                failed: DEFAULT_SOURCES.len(),
            });
            return;
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();

    let (mut updated, mut unchanged, mut failed) = (0usize, 0usize, 0usize);
    for action in update_sources(&DEFAULT_SOURCES, &fetcher, &store, now) {
        match &action {
            SourceAction::Updated { .. } => updated += 1,
            SourceAction::Unchanged { .. } => unchanged += 1,
            SourceAction::Failed { .. } => failed += 1,
        }
        if sender.send(UpdateEvent::Source(action)).is_err() {
            // The chrome went away mid-pass. The cache writes already happened
            // and are valid, so the next start picks them up.
            return;
        }
    }
    let _ = sender.send(UpdateEvent::Finished {
        updated,
        unchanged,
        failed,
    });
}
