//! Servo backend for the engine boundary: a [`PageController`] implementation
//! over a live `servo::WebView`. Built only with the `servo` feature, so the
//! default `craft-engine` stays free of engine and GPU dependencies.
//!
//! This module deliberately holds no windowing or GL code. The page object is
//! created by the host (`craft-ui::window`), because building one needs the winit
//! window, the surfman context and the offscreen surface the page paints into.
//! That half of the boundary moves with Phase 2b (see docs/engine-decision.md)
//! and needs screenshot verification. Everything here is a control-plane
//! operation on a page that already exists: navigation, history, load state and
//! page identity, read back out of the engine's own view of itself.

use std::cell::RefCell;
use std::rc::Rc;

use servo::{LoadStatus, WebView};
use tracing::{info, warn};
use url::Url;

use crate::{PageController, PageLoadState, PageSnapshot};

/// Handle the host uses to publish its live page to the controller.
///
/// UI-thread only by design: `servo::WebView` is an `Rc` and is not `Send`, so
/// this must not become a cross-thread registry. The host clears it before
/// Servo/GL teardown so the controller can never reach a freed page.
#[derive(Clone, Default)]
pub struct PageSlot {
    webview: Rc<RefCell<Option<WebView>>>,
}

impl PageSlot {
    pub fn attach(&self, webview: &WebView) {
        *self.webview.borrow_mut() = Some(webview.clone());
    }

    pub fn detach(&self) {
        *self.webview.borrow_mut() = None;
    }

    pub fn page(&self) -> Option<WebView> {
        self.webview.borrow().clone()
    }
}

/// The Servo implementation of [`PageController`]: every operation is applied to
/// the engine's own page object, and every reported value is read back out of it.
pub struct ServoPageController {
    slot: PageSlot,
}

impl ServoPageController {
    pub fn new(slot: PageSlot) -> Self {
        Self { slot }
    }

    fn page(&self, op: &str) -> Option<WebView> {
        let page = self.slot.page();
        if page.is_none() {
            warn!("engine: no live page yet, ignoring {op}");
        }
        page
    }
}

impl PageController for ServoPageController {
    fn load_url(&mut self, url: &str) {
        let Some(webview) = self.page("load_url") else {
            return;
        };
        match Url::parse(url) {
            Ok(parsed) => webview.load(parsed),
            Err(err) => warn!("engine: refused to navigate to {url:?}: {err}"),
        }
    }

    fn reload(&mut self, bypass_cache: bool) {
        let Some(webview) = self.page("reload") else {
            return;
        };
        if bypass_cache {
            // Servo 0.8 exposes no cache-ignoring reload. Do the ordinary
            // reload and say so, rather than claiming a hard reload happened.
            info!("engine: Servo has no cache-ignoring reload; reloading normally");
        }
        webview.reload();
    }

    fn stop(&mut self) {
        let Some(webview) = self.page("stop") else {
            return;
        };
        // There is no stop() on WebView in Servo 0.8; the page's own
        // window.stop() is the supported way to abort a load. The load status
        // then reports Stopped back through snapshot().
        webview.evaluate_javascript("window.stop()", |_| {});
    }

    fn go_back(&mut self) {
        if let Some(webview) = self.page("go_back") {
            let _ = webview.go_back(1);
        }
    }

    fn go_forward(&mut self) {
        if let Some(webview) = self.page("go_forward") {
            let _ = webview.go_forward(1);
        }
    }

    fn snapshot(&self) -> Option<PageSnapshot> {
        let webview = self.slot.page()?;
        Some(PageSnapshot {
            url: webview.url().map(|url| url.to_string()).unwrap_or_default(),
            title: webview.page_title().unwrap_or_default(),
            load_state: map_load_status(webview.load_status()),
            can_go_back: webview.can_go_back(),
            can_go_forward: webview.can_go_forward(),
        })
    }
}

fn map_load_status(status: LoadStatus) -> PageLoadState {
    match status {
        LoadStatus::Started => PageLoadState::Started,
        LoadStatus::HeadParsed => PageLoadState::Loading,
        LoadStatus::Complete => PageLoadState::Completed,
        LoadStatus::Stopped => PageLoadState::Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::map_load_status;
    use crate::PageLoadState;
    use servo::LoadStatus;

    /// The chrome's stop/reload button and its load progress depend entirely on
    /// this mapping, so each engine state must land on its own control-plane
    /// state. Two states collapsing together is how a UI ends up saying
    /// "loading" forever.
    #[test]
    fn every_load_status_maps_to_a_distinct_state() {
        let mapped = [
            map_load_status(LoadStatus::Started),
            map_load_status(LoadStatus::HeadParsed),
            map_load_status(LoadStatus::Complete),
            map_load_status(LoadStatus::Stopped),
        ];
        let expected = [
            PageLoadState::Started,
            PageLoadState::Loading,
            PageLoadState::Completed,
            PageLoadState::Stopped,
        ];
        assert_eq!(mapped, expected);

        for (i, a) in mapped.iter().enumerate() {
            for (j, b) in mapped.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "engine states {i} and {j} collapsed");
                }
            }
        }
    }
}
