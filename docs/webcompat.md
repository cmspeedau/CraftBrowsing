# Web Compatibility Gaps (Servo)

Tracking note for real-world site breakage observed with Servo, on both the pinned
0.8.0 and current `main`. This is a browser-engine coverage list, not our `craft-*`
code. We do not polyfill missing Web APIs in the embedder, because that would
misreport support to every other site. It is a watchlist to re-check as Servo lands
features. See `docs/engine-decision.md` for the decision to move the primary engine to
Blink/Chromium (CEF) while keeping Servo re-evaluable.

## Method / evidence source

Compiled from on-device runs captured in the browser log, which now includes page
console output and pipeline crashes via `WebViewDelegate::show_console_message`
and `notify_crashed` (see commit `19b9ac5`). Sites exercised so far: Facebook,
ABC News (`abc.com.au` -> `www.abc.net.au`), and YouTube.

Caveat: "Unsupported property declaration" lines are what Stylo emitted while
parsing these specific pages. A single invalid selector can cause the whole rule
block to be dropped, so the property counts below are indicative of what is
missing on those pages, not a verified per-property implementation status. For a
ground-truth list, cross-check against the Servo web-platform features dashboard
before acting on any single property.

## Upstream status (verified against Servo `main`)

Method: built Servo `main` in a container (`tools/upstream-servo-check`) and ran
headless, headed-for-console checks. Results are captured in
`tools/upstream-servo-check/out/`. Date: 2026-11-14, latest release v0.8.0.

- `IntersectionObserver`: still broken on `main`. Fixture reports
  `typeof window.IntersectionObserver = undefined`, `in window = false`,
  `THREW: ReferenceError ... DISPOSITION: BROKEN`; ABC News still hits its
  client-side exception. Upstream tracking #35767 open. Not fixed by a version bump.
- HTTP/2 on Google: still fails on `main`. `www.youtube.com` navigation reproduces
  `Reset(StreamId(1), PROTOCOL_ERROR, Library)` (client-initiated). Open upstream:
  #49396 (YouTube intermittent), #39785 (Gemini, Android). No conformance fix merged;
  recent h2 changes are only a dependency bump (#48917) and a timer (#48657), and our
  lock file already has the same h2 0.4.20. So a newer baseline is unlikely to help.
- Cadence: Servo releases are roughly monthly (0.4 to 0.8 spanned Aug to Nov 2026).
  The maturity needed for a general browser is not one or two fixes away, which is the
  input to the engine decision (see `docs/engine-decision.md` and the `servo-watch` CI job).

## Load-breaking (high confidence)

- `IntersectionObserver` present in the code but not reachable from web JS.
  - Symptom: `ReferenceError: IntersectionObserver is not defined` in the page
    console, then Next.js renders "Application error: a client-side exception has
    occurred" for `www.abc.net.au`. The document itself loaded fine (HTTP 200);
    the app threw during hydration.
  - Code state: `components/script/dom/intersectionobserver/` exists with a real
    `Constructor`, but is not installed on the window global.
  - Verified on `main` (container build, 2026-11-14): still `undefined`, fixture and
    ABC both still throw. So this is not fixed by "waiting for a baseline"; see the
    Upstream status section.
  - Impact: hard. Any site that constructs an `IntersectionObserver` unconditionally
    breaks. Common for lazy-loading and infinite-scroll UIs.
  - Action: track upstream (IO tracking issue #35767). Re-test each release via
    `tools/upstream-servo-check`.

## Selector engine failures

- `:has()` in relative-selector forms. Basic `:has(.cls)` parses; forms with a
  combinator and/or certain pseudo-classes inside throw
  `SelectorError(UnexpectedIdent)`, e.g.:
  - `:has(+ .MenuDrawerBody:hover)` (sibling combinator inside `:has`)
  - `:has(.navCategory:focus-visible)` (`:focus-visible` argument)
  - `:has([data-panel-open])`
  - Impact: dropped layout rules for menus/accordions/mega-menus -> visual breakage,
    not usually fatal.
- Vendor pseudo-elements are rejected outright (dropped):
  - `::-webkit-scrollbar` (89 occurrences on these pages)
  - `::-moz-range-thumb` / `::-moz-range-track`
  - `::-webkit-search-cancel-button` / `-decoration` / `-results-button`
  - `::-webkit-datetime-edit-*`
  - `::-webkit-media-text-track-container`
  - `::-ms-clear`
  - Impact: cosmetic. Custom scrollbars, range-slider thumbs, native-input styling
    do not apply. Expected: these are non-standard and Servo intentionally does not
    implement `-webkit-`/`-ms-` pseudo-elements.

## CSS properties reported unsupported (indicative counts from these pages)

Higher-severity layout/visual properties to watch:

- `backdrop-filter` (49) - blur/glass effects absent
- `scroll-snap-type` / `scroll-snap-align` / `scroll-snap-stop` (~48) - snap scrolling
- `scroll-behavior` (6) - smooth scrolling
- `overscroll-behavior` / `overscroll-behavior-y` (~18) - pull-to-refresh chaining
- `container-type` (10) - container queries (with `@container`)
- `contain` (14)
- `text-overflow` (43) - ellipsis truncation
- `scrollbar-width` / `scrollbar-color` (~50) - standard scrollbar theming
- `word-break: break-word` (32), `hyphens` (12)
- `column-count` / `-moz-column-gap` / `break-inside` / `page-break-inside`
- `mask` (6), `background-clip: text` (6)
- `font-variation-settings` (30) - variable-font axes
- `appearance` / `-moz-appearance` / `-moz-osx-font-smoothing` /
  `forced-color-adjust` - form and accessibility theming
- `resize` (8), `text-underline-offset` (7)
- `@font-face` descriptor `font-weight: lighter` (Unsupported)

Note: some of these (e.g. `word-break`, `column-count`, `contain`, `hyphens`) may
already be partly supported and were dropped here only because the enclosing rule
failed to parse. Treat the list as "seen missing on these pages", and verify
before prioritizing.

## Network stack (load-breaking, confirmed)

- HTTP/2 request stream reset on Google properties.
  - Symptom: top-level navigation to `www.youtube.com` fails with
    `Could not load the requested page: client error (SendRequest)`. Log:
    `hyper Error(Http2, Error { kind: Reset(StreamId(1), PROTOCOL_ERROR, Library) })`
    on the first request stream, immediately after the `youtube.com` ->
    `www.youtube.com` 301. The same signature also killed an ABC request. So it
    is a repeating failure on HTTP/2 servers that enforce strict framing, not a
    one-off.
  - Where: `servo-net` builds the client with `.enable_http1().enable_http2()`
    (`connector.rs::create_http_client`), and ALPN selects h2. The only code that
    pins ALPN to `http/1.1` is the WebSocket handshake path
    (`http_loader.rs:2229`), so normal navigation has no h2->h1 fallback.
  - Embedder options: none exposed via `Opts`/preferences. There is no supported
    way to force HTTP/1.1 from `craft-*`. A workaround would require vendoring or
    patching `servo-net` to disable HTTP/2 (drop `.enable_http2()`), which is not
    something to do lightly.
  - Action: upstream. Reproduced on `main` (2026-11-14, see Upstream status above),
    so it is not a pinned-version-only bug. This is a Servo/rustls/h2 conformance
    issue against Google's h2 servers. For the report, the verified-good comparator is
    curl/libcurl against the same URL: if curl negotiates h2 and 200s where our client
    self-resets, that contrast is the evidence. Repro: load `youtube.com` (and
    `www.google.com`) with `RUST_LOG=info` and grep `PROTOCOL_ERROR`.

## Per-site summary

- Facebook (Login): loads and renders. `:has()` and vendor pseudo-elements dropped
  -> minor visual gaps. Not load-breaking.
- ABC News (Next.js): loads the document, then `IntersectionObserver` is
  undefined -> client-side exception -> site error page. Load-breaking. Root cause
  is a missing DOM API, not CSS.
- YouTube: does not load at all. HTTP/2 stream reset on the main document
  (`PROTOCOL_ERROR`). Load-breaking. Root cause is the engine's HTTP/2 path, not
  our embedder and not CSS/JS.

## Re-check procedure

1. Re-run against each site and capture the log (`RUST_LOG=info cargo run -p
   craft-ui`).
2. Grep the new log the same way this file was built:
   `SelectorError`, `Unsupported property`, `page console] Error`, `PROTOCOL_ERROR`.
3. Diff against this list; remove items Servo now supports, add new gaps.
4. Link findings to Servo issues; do not implement polyfills or forced-protocol
   hacks in `craft-*`.
