//! End-to-end check of the upstream filter list path: real fetch, real cache,
//! real compile, real blocking.
//!
//! Marked `#[ignore]` because it reaches the network. CI must not depend on
//! easylist.to being up, so run it deliberately:
//!
//! ```sh
//! cargo test -p craft-ui --test upstream_end_to_end -- --ignored --nocapture
//! ```

use std::fs;

use craft_adblock::AdblockManager;
use craft_adblock::upstream::{DEFAULT_SOURCES, ListStore, SourceAction, update_sources};
use craft_ui::lists::HttpFetcher;

#[test]
#[ignore = "fetches real filter lists from the internet"]
fn upstream_lists_download_cache_and_block() {
    let unique = format!(
        "craft-upstream-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    let store = ListStore::new(&dir);

    let fetcher = HttpFetcher::new().expect("an HTTP client can be built");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let actions = update_sources(&DEFAULT_SOURCES, &fetcher, &store, now);
    let updated: Vec<&SourceAction> = actions
        .iter()
        .filter(|action| matches!(action, SourceAction::Updated { .. }))
        .collect();
    for action in &actions {
        match action {
            SourceAction::Updated {
                source_id, text, ..
            } => println!("{source_id}: downloaded {} bytes", text.len()),
            SourceAction::Unchanged { source_id, .. } => println!("{source_id}: unchanged"),
            SourceAction::Failed {
                source_id, reason, ..
            } => println!("{source_id}: FAILED {reason}"),
        }
    }
    assert!(
        !updated.is_empty(),
        "at least one upstream list must download; every source failed"
    );

    // Cached to disk and readable back without the network.
    let cached = store.load_all(&DEFAULT_SOURCES);
    assert_eq!(cached.len(), updated.len());

    let manager = AdblockManager::new();
    let before = manager.rule_count();
    let reports = manager.install_lists(cached.iter().map(|list| {
        (
            list.meta.source_id.as_str(),
            list.meta.title.as_str(),
            list.text.clone(),
            Some(list.meta.fetched_at_unix),
        )
    }));
    let installed: usize = reports.iter().map(|report| report.accepted).sum();
    let refused: usize = reports.iter().map(|report| report.rejected).sum();
    println!(
        "installed {installed} upstream rules alongside {before} bundled rules ({} total); parser refused {refused}",
        manager.rule_count()
    );
    assert!(
        manager.rule_count() > before + 10_000,
        "upstream lists should add far more than the bundled rules"
    );
    // Real lists contain syntax adblock-rust cannot parse; that is expected and
    // must be reported rather than hidden, but a majority must compile.
    assert!(
        installed > refused,
        "more rules were refused than accepted ({refused} of {}), the parser or the lists changed",
        installed + refused
    );

    for (url, kind) in [
        (
            "https://securepubads.g.doubleclick.net/pagead/js/adsbygoogle.js",
            "script",
        ),
        (
            "https://www.google-analytics.com/collect?v=1&t=pageview",
            "xhr",
        ),
        ("https://sb.scorecardresearch.com/b?c1=2", "image"),
    ] {
        assert!(
            manager.check_network_request(url, "https://www.example-news-site.test/article", kind),
            "{url} should be blocked by the upstream lists"
        );
    }
    assert!(
        !manager.check_network_request(
            "https://www.example-news-site.test/article/photo.jpg",
            "https://www.example-news-site.test/article",
            "image",
        ),
        "a first-party image must not be blocked"
    );

    // Second pass: conditional requests must not re-download everything.
    let again = update_sources(&DEFAULT_SOURCES, &fetcher, &store, now + 1);
    for action in &again {
        if let SourceAction::Failed {
            source_id, reason, ..
        } = action
        {
            match reason.as_str() {
                r if r.contains("304") => panic!("{source_id} reported a 304 as a failure"),
                _ => println!("{source_id}: second pass did not update ({reason})"),
            }
        }
    }
    assert_eq!(store.load_all(&DEFAULT_SOURCES).len(), cached.len());

    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "fetches real filter lists into the browser's own cache directory"]
fn populate_the_browser_cache_for_a_real_launch() {
    // Not an assertion about behaviour so much as a fixture: this is the same
    // call the menu button makes, so running it and then launching the browser
    // exercises startup-from-cache for real. Run with:
    //   cargo test -p craft-ui --test upstream_end_to_end -- --ignored
    let store = ListStore::new(craft_ui::filter_list_dir());
    let fetcher = HttpFetcher::new().expect("client");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let actions = update_sources(&DEFAULT_SOURCES, &fetcher, &store, now);
    let updated = actions
        .iter()
        .filter(|action| matches!(action, SourceAction::Updated { .. }))
        .count();
    println!(
        "cached {} lists in {}, {} on disk",
        updated,
        store.dir().display(),
        store.load_all(&DEFAULT_SOURCES).len()
    );
    assert_eq!(
        store.load_all(&DEFAULT_SOURCES).len(),
        updated,
        "everything reported as updated must be readable back from disk"
    );
}

#[test]
#[ignore = "fetches real filter lists from the internet"]
fn a_server_error_is_reported_not_silently_swallowed() {
    let dir = std::env::temp_dir().join(format!("craft-upstream-404-{}", std::process::id()));
    let store = ListStore::new(&dir);
    let fetcher = HttpFetcher::new().expect("client");
    let source = craft_adblock::upstream::FilterSource {
        id: "does-not-exist",
        title: "Missing list",
        url: "https://easylist.to/easylist/this-list-does-not-exist.txt",
        homepage: "https://easylist.to/",
        licence: "https://easylist.to/pages/licence.html",
    };

    let actions = update_sources(std::slice::from_ref(&source), &fetcher, &store, 0);
    match &actions[0] {
        SourceAction::Failed { reason, .. } => {
            println!("got the expected failure: {reason}");
            assert!(
                !reason.is_empty(),
                "a failed source must carry a reason the chrome can show",
            );
        }
        other => panic!("a 404 must be reported as a failure, got {other:?}"),
    }
    assert!(store.load(&source).is_none(), "nothing should be cached");
    fs::remove_dir_all(&dir).ok();
}
