//! The engine control plane must report what the page says about itself, never
//! what a command asked for. These tests pin the difference between `TabEngine`
//! and the simulated engine it replaced: operations are not answered with
//! invented `Completed` events, and page state only reaches the chrome through
//! snapshots the backend actually provides.

use std::cell::RefCell;
use std::rc::Rc;

use craft_core::container::ContainerId;
use craft_core::tab::TabId;
use craft_engine::{
    Backend, Engine, EngineEvent, PageController, PageLoadState, PageSnapshot, TabEngine,
};

#[derive(Default)]
struct SharedPage {
    ops: RefCell<Vec<String>>,
    page: RefCell<Option<PageSnapshot>>,
}

impl SharedPage {
    /// Stand in for the backend changing what its page reports.
    fn set(&self, url: &str, state: PageLoadState, title: &str) {
        *self.page.borrow_mut() = Some(PageSnapshot {
            url: url.to_string(),
            title: title.to_string(),
            load_state: state,
            can_go_back: false,
            can_go_forward: false,
        });
    }
}

/// Stand-in for a backend page handle (a `servo::WebView`, later a CEF browser).
struct MockPage {
    shared: Rc<SharedPage>,
}

impl PageController for MockPage {
    fn load_url(&mut self, url: &str) {
        self.shared.ops.borrow_mut().push(format!("load {url}"));
        self.shared.set(url, PageLoadState::Started, "");
    }

    fn reload(&mut self, bypass_cache: bool) {
        self.shared
            .ops
            .borrow_mut()
            .push(format!("reload bypass_cache={bypass_cache}"));
    }

    fn stop(&mut self) {
        self.shared.ops.borrow_mut().push("stop".to_string());
    }

    fn go_back(&mut self) {
        self.shared.ops.borrow_mut().push("back".to_string());
    }

    fn go_forward(&mut self) {
        self.shared.ops.borrow_mut().push("forward".to_string());
    }

    fn snapshot(&self) -> Option<PageSnapshot> {
        self.shared.page.borrow().clone()
    }
}

fn tab(id: &str) -> TabId {
    TabId(id.to_string())
}

fn engine() -> (TabEngine<MockPage>, Rc<SharedPage>) {
    let shared = Rc::new(SharedPage::default());
    let controller = MockPage {
        shared: shared.clone(),
    };
    (TabEngine::new(Backend::Servo, controller), shared)
}

fn navigations(events: Vec<EngineEvent>) -> Vec<(TabId, PageLoadState, String)> {
    events
        .into_iter()
        .map(|event| match event {
            EngineEvent::Navigation(nav) => (nav.tab_id, nav.state, nav.url),
        })
        .collect()
}

#[test]
fn create_tab_records_the_tab_without_touching_any_page() {
    let (mut engine, shared) = engine();
    let id = tab("a");

    engine.create_tab(
        &id,
        &ContainerId::default_container(),
        "https://example.com",
    );

    // Creating a tab is bookkeeping: no navigation is performed and no
    // completion event is manufactured for it.
    assert!(shared.ops.borrow().is_empty());
    assert_eq!(engine.live_tab(), None);
    assert!(engine.poll_events().is_empty());

    let snapshot = engine
        .page_snapshot(&id)
        .expect("the tab must be tracked after create");
    assert_eq!(snapshot.url, "https://example.com");
    assert_eq!(snapshot.load_state, PageLoadState::Initial);
}

#[test]
fn navigate_delegates_then_reports_the_pages_own_transitions() {
    let (mut engine, shared) = engine();
    let id = tab("a");
    engine.create_tab(&id, &ContainerId::default_container(), "about:blank");

    engine.navigate(&id, "https://example.com");
    assert_eq!(
        *shared.ops.borrow(),
        vec!["load https://example.com".to_string()]
    );

    let events = navigations(engine.poll_events());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, PageLoadState::Started);

    // The page redirects and finishes: the chrome learns the real destination
    // from the backend, not from what was requested.
    shared.set(
        "https://example.com/redirected",
        PageLoadState::Completed,
        "Example",
    );
    let events = navigations(engine.poll_events());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, PageLoadState::Completed);
    assert_eq!(events[0].2, "https://example.com/redirected");

    let snapshot = engine.page_snapshot(&id).expect("tracked");
    assert_eq!(snapshot.title, "Example");

    // A page that has not moved produces no further events.
    assert!(engine.poll_events().is_empty());
}

#[test]
fn stop_only_applies_to_the_tab_that_owns_the_live_page() {
    let (mut engine, shared) = engine();
    let a = tab("a");
    let b = tab("b");
    engine.create_tab(&a, &ContainerId::default_container(), "https://a.example");
    engine.create_tab(&b, &ContainerId::default_container(), "https://b.example");
    engine.navigate(&a, "https://a.example");
    let _ = engine.poll_events();

    // b does not own the page, so stopping it must not abort a's load.
    engine.stop(&b);
    assert!(!shared.ops.borrow().contains(&"stop".to_string()));

    engine.stop(&a);
    assert!(shared.ops.borrow().contains(&"stop".to_string()));
    // stop() invents no state change; the page has to report it.
    assert!(engine.poll_events().is_empty());

    shared.set("https://a.example", PageLoadState::Stopped, "");
    let events = navigations(engine.poll_events());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, PageLoadState::Stopped);
}

#[test]
fn history_operations_delegate_and_claim_the_live_page() {
    let (mut engine, shared) = engine();
    let id = tab("a");
    engine.create_tab(&id, &ContainerId::default_container(), "https://a.example");

    engine.go_back(&id);
    engine.go_forward(&id);
    engine.reload(&id, true);
    assert_eq!(
        *shared.ops.borrow(),
        vec![
            "back".to_string(),
            "forward".to_string(),
            "reload bypass_cache=true".to_string()
        ]
    );
    assert_eq!(engine.live_tab(), Some(&id));
}

#[test]
fn closing_a_tab_stops_page_reports_for_it() {
    let (mut engine, shared) = engine();
    let id = tab("a");
    engine.create_tab(&id, &ContainerId::default_container(), "https://a.example");
    engine.navigate(&id, "https://a.example");
    let _ = engine.poll_events();

    engine.close_tab(&id);
    assert_eq!(engine.page_snapshot(&id), None);
    assert_eq!(engine.live_tab(), None);
    assert_eq!(engine.tracked_tabs(), 0);

    // The shared page may still move afterwards; nothing may be attributed to a
    // tab that no longer exists.
    shared.set("https://orphan.example", PageLoadState::Completed, "");
    assert!(engine.poll_events().is_empty());
}

#[test]
fn a_host_created_page_is_adopted_and_then_describes_itself() {
    let (mut engine, shared) = engine();
    let id = tab("initial");

    // The window creates the first page outside the control plane (it owns the
    // rendering context), so it hands ownership over explicitly.
    engine.create_tab(
        &id,
        &ContainerId::default_container(),
        "https://example.com",
    );
    shared.set(
        "https://example.com/",
        PageLoadState::Completed,
        "Example Domain",
    );
    engine.set_live_page(&id);

    let events = navigations(engine.poll_events());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, PageLoadState::Completed);
    assert_eq!(events[0].2, "https://example.com/");
    assert_eq!(engine.backend(), Backend::Servo);
}
