# Tracking Protection: What Runs Today

Status: network filtering is active in the browser. Element hiding and remote
filter-list subscriptions are not. Verified 2026-11-15 against a live news page:
59 bundled rules compiled, 25 requests refused, page still loaded.

## How a request gets refused

1. Servo routes every HTTP(S) resource of a page through
   `WebViewDelegate::load_web_resource` (`crates/craft-ui/src/window.rs`). Not
   answering the call means "do not intercept", so an allowed request costs one
   rule lookup and nothing else.
2. The request's fetch destination is mapped onto a filter-syntax resource type
   (`crates/craft-ui/src/shields.rs`): script, stylesheet, image, font, media,
   sub_frame, ping, other. The match is exhaustive over the engine's enum, so a
   new destination is a compile error rather than a rule that quietly stops
   firing.
3. `craft-adblock` answers yes or no (`AdblockManager::check_network_request`),
   honouring the global switch and per-site exceptions.
4. On yes, the load is cancelled, so the page sees a failed request. An empty
   `200` would let a script treat our silence as a successful ad load.

Top-level documents are never refused. A blank page with no explanation reads as
a broken browser, and a blocked navigation needs its own UI, which does not
exist yet.

## What ships

- Compiled rules at startup from `crates/craft-adblock/lists/default-network-filters.txt`:
  curated ad, tracking, analytics and session-recording endpoints in standard
  Adblock Plus network syntax.
- The address-bar shield, which pauses and resumes filtering with one click and
  reports the rule count and the number of requests refused this session.
- Loading additional lists without losing the ones already in memory
  (`add_filter_rules` appends; it used to replace, so a second list erased the
  first). Rejected rules are counted and logged, not swallowed.

## What does not ship yet

- **Element hiding / cosmetic filters.** `adblock-rust` parses them, but nothing
  applies them: no `get_cosmetic_styles` call reaches the page. Any `$elemhide`
  or `###selector` rule in a list is inert today.
- **Remote subscriptions.** No EasyList, EasyPrivacy or uBlock download. The
  shipped rules are ours, so the coverage is a few dozen endpoints rather than a
  full list. An updater must be user-triggered: fetching lists at startup would
  be a default network call, which the privacy invariants forbid.
- **Per-site controls UI.** `add_site_exception` and `remove_site_exception`
  exist and are tested; the shield only toggles globally, because internal pages
  (`craft://settings/shields`) are not registered with the engine.
- **Parameter stripping and CNAME uncloaking.** Described in
  `docs/security-and-privacy.md` as design; not implemented.

## Engine independence

The hook is Servo-specific glue in `craft-ui`. The decision half is not:
`craft-adblock` has no engine dependency, so the Blink/CEF move replaces step 1
with CEF's request interception and keeps steps 2 to 4 (`docs/engine-decision.md`
step 4).

## Verifying it

```sh
RUST_LOG=info cargo run -p craft-ui
# startup line:  shields: 59 network rules compiled from the bundled lists
# per block:     shields: blocked script request to https://...
```

Unit tests: `cargo test -p craft-adblock` (rule loading, matching, exceptions,
fail-open on unparsable requests) and `cargo test -p craft-ui shields`
(destination mapping, counters).
