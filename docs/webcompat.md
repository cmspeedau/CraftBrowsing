# Web Compatibility Gaps (Servo 0.8.0)

Tracking note for real-world site breakage observed with the Servo 0.8.0 engine.
This is a browser-engine coverage list, not our `craft-*` code. We do not
polyfill missing Web APIs in the embedder, because that would misreport support
to every other site. This file is a watchlist to re-check as Servo lands
features.

## Method / evidence source

Compiled from on-device runs captured in the browser log, which now includes page
console output and pipeline crashes via `WebViewDelegate::show_console_message`
and `notify_crashed` (see commit `19b9ac5`). Sites exercised so far: Facebook and
ABC News (`abc.com.au` -> `www.abc.net.au`).

Caveat: "Unsupported property declaration" lines are what Stylo emitted while
parsing these specific pages. A single invalid selector can cause the whole rule
block to be dropped, so the property counts below are indicative of what is
missing on those pages, not a verified per-property implementation status. For a
ground-truth list, cross-check against the Servo web-platform features dashboard
before acting on any single property.

## Load-breaking (high confidence)

- `IntersectionObserver` not implemented.
  - Symptom: `ReferenceError: IntersectionObserver is not defined` in the page
    console, then Next.js renders "Application error: a client-side exception has
    occurred" for `www.abc.net.au`. The document itself loaded fine (HTTP 200);
    the app threw during hydration.
  - Impact: hard. Any site that constructs an `IntersectionObserver` unconditionally
    breaks. Common for lazy-loading and infinite-scroll UIs.
  - Action: upstream. Re-test when Servo ships `IntersectionObserver`.

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

## Network stack (transient, watch)

- One HTTP/2 stream reset observed: `hyper Error(Http2, PROTOCOL_ERROR, Reset
  StreamId(1))` during an ABC session, surfacing as `script_thread: Network error:
  HTTP failure: client error (SendRequest)`. Single occurrence, not reproduced yet.
  Action: watch for HTTP/2 reset patterns across sites; file upstream if it becomes
  reproducible rather than a one-off server reset.

## Per-site summary (so far)

- Facebook (Login): loads and renders. `:has()` and vendor pseudo-elements dropped
  -> minor visual gaps. Not load-breaking.
- ABC News (Next.js): loads the document, then `IntersectionObserver` is
  undefined -> client-side exception -> site error page. Load-breaking. Root cause
  is a missing DOM API, not CSS.

## Re-check procedure

1. Re-run against each site and capture the log (`RUST_LOG=info cargo run -p
   craft-ui`).
2. Grep the new log the same way this file was built:
   `SelectorError`, `Unsupported property`, `page console] Error`, `PROTOCOL_ERROR`.
3. Diff against this list; remove items Servo now supports, add new gaps.
4. Link findings to Servo issues; do not implement polyfills in `craft-*`.
