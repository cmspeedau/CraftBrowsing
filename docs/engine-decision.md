# Engine Decision: Adopt Blink/Chromium (CEF), Keep Servo Re-evaluable

Status: Accepted (direction). Not yet implemented. `craft-engine` still embeds
Servo today. The swap and the `Engine` trait boundary are a planned phase that
needs a separate go before code changes.

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

## Implementation plan (not started; needs a separate go)

1. Define the `Engine` trait in `craft-engine` (session, tab, navigation, events)
   and move the current Servo embed behind a `ServoEngine` impl, with the egui
   chrome depending only on the trait. This is the reversible foundation and keeps
   today's working build.
2. Add a `CefEngine` impl behind a feature flag; port tab/page lifecycle, the
   content-rect compositing model (CEF uses native child surfaces, not a GL blit),
   input forwarding, and console/crash callbacks.
3. Move the adblock network hook to the CEF request pipeline (resource interception).
4. Update the three packaging targets to bundle CEF.
5. Keep the `ServoEngine` impl in-tree; it is what the re-evaluation flips back to.

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
