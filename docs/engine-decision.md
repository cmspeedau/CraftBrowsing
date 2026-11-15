# Engine Decision: Adopt Blink/Chromium (CEF), Keep Servo Re-evaluable

Status: Accepted (direction). The `Engine` / `PageController` boundary is in
place and the chrome depends on it, but the rendering engine is still Servo. The
CEF backend, the per-tab surfaces, and packaging are not started, and each needs a
separate go.

Date: 2026-11-15
Deciders: project owner

## Context

CraftBrowsing was positioned as a pure-Rust browser on Servo. Real-world use
exposed a web-compatibility gap that a young engine closes slowly. We validated
the specific failures by building Servo `main` in a container and running headless
checks (see tools/upstream-servo-check and docs/webcompat.md).

Validated findings on Servo `main`, not just the pinned 0.8.0:
- `IntersectionObserver` is not exposed on the window global. Fixture and ABC News
  both throw `ReferenceError: IntersectionObserver is not defined`; ABC renders its
  Next.js client-side error page. Tracking issue servo/servo#35767 is open, and the
  feature is mid-development upstream.
- HTTP/2 navigation to `www.youtube.com` fails with a local stream reset
  `Reset(StreamId(1), PROTOCOL_ERROR, Library)`. Related open issues: #49396
  (YouTube intermittent), #39785 (Gemini). Not fixed on `main`.
- A long tail of CSS/selector gaps remains (`:has()` relative forms, vendor
  pseudo-elements, `backdrop-filter`, container queries, scroll-snap). Individually
  small, collectively a reason many sites render imperfectly.

Servo releases roughly monthly (0.4 through 0.8 spanned Aug to Nov 2026). The
maturity we need for a general-purpose browser is not a one-release or two-fix
away across the whole surface.

## Decision

Adopt Blink/Chromium as the primary rendering engine via CEF, chosen over per-OS
system webviews (wry) because it gives one consistent Blink on Windows, macOS, and
Linux, matching the product's cross-platform parity goal.

Retire the pure-Rust positioning. The application layer stays Rust (UI, tabs,
storage, adblock, sync, transfers, extensions); the engine becomes a dependency we
do not own. This trade is accepted deliberately, not implicitly.

Stay reversible. Introduce an `Engine` trait boundary so the chrome depends on an
interface, not on Servo or Blink directly. Servo becomes a re-addable backend. A CI
watch job tracks Servo releases and the specific gap issues, and we re-evaluate at
the next release boundary.

## Consequences

Positive:
- Sites that break on Servo today work on Blink.
- One engine across all three OSes, so QA and behavior are consistent.
- The pure-Rust maintenance burden no longer gates our compatibility.

Negative (accepted):
- Larger binaries and native dependencies on every platform (CEF is a full browser
  runtime; Linux gets bundled Chromium libs, Windows/macOS their equivalents).
- We are no longer "independent of Chromium"; that is now our engine.
- Packaging and the AppImage/`zsync` and macOS notarization flows grow to carry CEF.
- We inherit Chromium's update and telemetry behavior at the engine layer, so the
  zero-telemetry stance moves to the chrome and is enforced around the engine
  (process isolation, flag-based lockdown), not by the engine being Rust.

Neutral for now:
- Memory-safety and sandboxing claims are reworded: subsystems are Rust, rendering
  runs in the engine's sandboxed process.

## Implementation plan

1. Done (2026-11-15). Define the engine boundary in `craft-engine` and stop the
   chrome depending on a concrete engine. Shape as built: `Engine` (tab records,
   navigation, events, page snapshots) is held by `craft-ui` as `Box<dyn Engine>`;
   page operations go through a second trait, `PageController`, implemented by a
   backend's page handle (`craft_engine::servo_backend::ServoPageController`,
   behind a `servo` feature, over `servo::WebView`). The simulated `ServoEngine`
   worker thread and its fabricated `Completed`/200 events are deleted. The engine
   and GPU stack reach `craft-engine` only through that feature: the default build
   and the control-plane tests are boundary-only, and the window, surfman contexts
   and page paint stay in `craft-ui::window` until Phase 2b.
2. Phase 2a done (2026-11-15): page truth has one source. URL, title, load state
   and history availability come from `PageSnapshot`; the duplicate `PageState`
   mutex in the chrome is gone, and the address bar follows the page's real URL.
   Phase 2b, not started: move page creation and painting/compositing behind the
   boundary and give each tab its own rendering context (per-tab and per-container
   isolation depends on this). This part relocates the surfman/GL blit and must be
   verified with screenshots, not logs: a blank-but-loaded page looks identical to
   a correct one in the log.
3. Add a `CefEngine` impl behind a feature flag; port tab/page lifecycle, the
   content-rect compositing model (CEF uses native child surfaces, not a GL blit),
   input forwarding, and console/crash callbacks.
4. Move the adblock network hook to the CEF request pipeline (resource
   interception). Done on the Servo side (2026-11-15):
   `WebViewDelegate::load_web_resource` in `craft-ui::window` cancels matched
   subresource loads and the decision half lives in `craft-adblock`, which has no
   engine dependency, so the CEF port is the hook only. Rule sources are also
   engine-independent now: the bundled list plus the cached upstream lists
   (EasyList, EasyPrivacy, uBlock filters, fetched only on a user action) arrive
   through `craft_adblock::upstream`, and only the request hook has to be reborn
   per backend. Neither backend applies cosmetic rules yet. See
   `docs/tracking-protection.md`.
5. Update the three packaging targets to bundle CEF.
6. Keep the Servo `PageController` impl in-tree; it is what the re-evaluation
   flips back to.

## Re-evaluation

Trigger: a new servo/servo release where the tracked gaps are closed, or at the next
planned review window.

Schedule: monthly release boundary. Realistic next full re-evaluation window is
January to February 2027, after a couple more Servo releases land (and possibly the
`IntersectionObserver` tracking item and h2 conformance progress). The CI watch job
(`.github/workflows/servo-watch.yml`) runs on a schedule, reports the latest release
plus the state of the tracked issues, and opens or updates a tracking issue when a
gap closes so a human can run the container check and decide.

Re-evaluation is a human call backed by the container result, not an automatic flip.

## References

- docs/webcompat.md (captured gaps)
- tools/upstream-servo-check/ (container that builds Servo main and verifies)
- docs/making-submissions.md (how we report upstream, per Servo's AI-usage policy)
- servo/servo issues: #35767, #49396, #39785
