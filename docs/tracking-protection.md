# Tracking Protection: What Runs Today

Status: network filtering is active, with a curated bundled ruleset and upstream
lists the user downloads on demand. Element hiding and per-list controls are not.
Verified 2026-11-15: the bundled list compiles at startup with no rejected rules,
and one deliberate run fetched EasyList, EasyPrivacy and uBlock filters and
installed 137,795 upstream rules beside it, with 3,165 lines reported as refused
by the parser.

## How a request gets refused

1. Servo routes every HTTP(S) resource of a page through
   `WebViewDelegate::load_web_resource` (`crates/craft-ui/src/window.rs`). Not
   answering the call means "do not intercept", so an allowed request costs one
   rule lookup and nothing else.
2. The request's fetch destination is mapped onto a filter-syntax resource type
   (`crates/craft-ui/src/shields.rs`): document, sub_frame, script, stylesheet,
   image, font, media, ping, other. The match is exhaustive over the engine's
   enum, so a new destination is a compile error rather than a rule that quietly
   stops firing.
3. `craft-adblock` answers yes or no (`AdblockManager::check_network_request`),
   honouring the global switch and per-site exceptions.
4. On yes, the load is cancelled, so the page sees a failed request. An empty
   `200` would let a script read our silence as a successful ad load.

Top-level documents are never refused. A blank page with no explanation reads as
a broken browser, and a blocked navigation needs its own UI, which does not
exist yet.

## Where the rules come from

- **Bundled:** `crates/craft-adblock/lists/default-network-filters.txt`, a
  curated set of network endpoints in standard Adblock Plus syntax (60 rules as of
  2026-11-15). Compiled at startup, so protection exists before any download.
- **Upstream, on request:** the overflow menu's "Update filter lists now" fetches
  EasyList, EasyPrivacy and uBlock filters over HTTPS, validates each response,
  and caches it under the platform data directory
  (`~/.local/share/craftbrowsing/filterlists` on Linux; `craft_ui::filter_list_dir`
  is the single definition). Every later start reads those files back with no
  network traffic.

  | List | Source | Licence |
  | --- | --- | --- |
  | EasyList | `https://easylist.to/easylist/easylist.txt` | `https://easylist.to/pages/licence.html` |
  | EasyPrivacy | `https://easylist.to/easylist/easyprivacy.txt` | `https://easylist.to/pages/licence.html` |
  | uBlock filters | `https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/filters.txt` | `https://github.com/uBlockOrigin/uAssets/blob/master/LICENSE` |

  The licence of each list travels with its entry in `DEFAULT_SOURCES`, so the
  attribution is in the code that uses the rules rather than in a document that
  can drift from it.

## Why nothing downloads by itself

The privacy invariants forbid default network calls, so an updater must be
user-triggered. There is no schedule, no startup fetch, and no "check for
updates" timer. A first run therefore has only the bundled rules until the
button is pressed,
which is a deliberate trade: weaker default coverage in exchange for the browser
never phoning home on its own. A future settings page can offer an opt-in
schedule; that has to be an explicit switch, not a default.

What a pass guarantees:

- One pass at a time; a second click is refused rather than racing the cache.
- A response is refused if it is HTML, has no filter list header, or has fewer
  than 100 rule lines. A refused or failed source keeps its previous cache and
  its previous rules, so one broken upstream cannot lower protection.
- Bodies are capped at 24 MB, checked against `Content-Length` first and then
  counted while reading, so a lying server cannot make the browser buffer freely.
- Conditional requests: a cached list's `ETag` and `Last-Modified` are sent as
  `If-None-Match` / `If-Modified-Since`, and a 304 is reported as "still
  current", not as a failure.
- Rules that the parser refuses are counted and logged per list, never silently
  dropped. On the real lists that is 3,165 of 140,937 lines (2.2%), which is
  normal: those lists carry syntax `adblock-rust` does not support.
- The `User-Agent` is static (`CraftBrowsing/<version> (filter list update)`) and
  no cookies or session identifiers are sent.

## Cost, measured

On this machine, in a release build: 141k rule lines parse in about 120 ms and
compile in about 210 ms, and all three lists together hold about 61 MB resident.
Compiling cannot move off the UI thread because `adblock`'s engine is neither
`Send` nor `Sync`, so downloading runs on a worker and installing happens on the
UI thread as one batched rebuild per pass, about 0.3 s of interface stall at the
moment the user asked for an update.

## What does not ship yet

- **Element hiding / cosmetic filters.** The upstream lists contain roughly
  28,000 cosmetic rules and `adblock-rust` parses them, but nothing delivers
  `get_cosmetic_styles` results to the page, so they are inert. This is the
  largest remaining gap in practice: EasyList's value is substantially in those
  rules.
- **A schedule or age-based prompt.** Nothing suggests an update when a cached
  list goes stale. The lists declare `! Expires: 4 days` in their headers, and the
  browser neither reads nor stores that, so a cache from last month is served
  with no warning. The stored `fetched_at_unix` is the only age information, and
  the menu shows it as "3 d ago" without judging it.
- **Per-list controls.** `remove_group` exists on the engine side, but there is no
  UI to disable one list, and no custom-list import.
- **Fine-grained per-site control.** There is a per-site pause (the menu, reported
  on the shield and on `craft://shields`), but it is all-or-nothing for the host:
  no allowing one tracker while blocking another, and no per-container override.
- **Parameter stripping and CNAME uncloaking.** Design text in
  `docs/security-and-privacy.md`, no implementation.

## What a paired run showed

Same page (a news homepage), same build, two runs, 35 seconds each, the only
difference being whether the cached upstream lists were present:

| Rules active | Requests refused | Third-party requests that still left the browser |
| --- | --- | --- |
| 59 (bundled only) | 25 | 156 |
| 137,862 (bundled plus upstream) | 18 | 98 |

The refusal count went **down** while leakage went down 37%. That is not a
contradiction and it is the trap in this metric: the counter counts times *we*
said no, not how much tracking happened. Upstream EasyList blocks
`www.googletagservices.com/tag/js/gpt.js`, which the bundled list missed, and once
that GPT loader never runs, the whole downstream fan-out of doubleclick,
ad-exchange and beacon requests is never attempted, so there is nothing left for
the browser to refuse. Evidence in the logs: with the bundled list alone,
`googletagservices.com/tag/js/gpt.js` returned 200 and eight doubleclick requests
followed; with the upstream lists it was refused and the log contains no
doubleclick traffic at all.

Useful corollary: the number next to the shield is a count of actions taken, not
a coverage figure. Do not present it as "tracking blocked" in a benchmark sense.

Two other things the run showed, both worth keeping in mind:

- **First-party-hosted tracking is where the extra value was.** With the upstream
  lists, Ars's own `permutive.arstechnica.com`, `fpa-events.arstechnica.com` and
  `fpa-cdn.arstechnica.com` endpoints were never requested, and the site's real
  content was unaffected: 95 asset requests to `cdn.arstechnica.net` in both runs.
- **A false positive appeared, and it is the reason per-site controls matter.**
  The upstream lists refuse AWS WAF challenge tokens
  (`*.token.awswaf.com`); this page made eight such requests and all eight were
  refused. The document still reached `Complete` and its assets still loaded, so
  on this site it cost nothing visible, but a WAF that insists on that token
  would challenge or block the visitor. The per-site pause exists for exactly
  that case, which is also why it is reported in three places (menu label, shield
  tooltip, `craft://shields`) instead of being invisible state.

## Verifying it

```sh
RUST_LOG=info cargo run -p craft-ui
# startup:  shields: 60 bundled network rules compiled (0 rejected)
# with cache: shields: loaded 3 cached filter lists, ... rules total
# per block: shields: blocked script request to https://...
# per-site:  shields: protection paused for <host>   (menu: "Pause protection for <host>")

# Internal pages, served by this build's own protocol handler:
#   craft://welcome  -> INFO page title: "CraftBrowsing"
#   craft://shields  -> INFO page title: "Tracking protection"
#   craft://settings -> INFO page title: "Settings"
#   anything else    -> INFO page title: "No such page"
# A title line means the engine fetched the document from us and parsed it;
# `load status: Complete` means it rendered. See docs/internal-pages.md.

# Real fetch, cache, compile and block, against the live upstreams. Opt in:
cargo test -p craft-ui --test upstream_end_to_end -- --ignored --nocapture
```

The paired run below is what found that `googletagservices.com` was missing from
the bundled list, so it is now covered and pinned by
`the_bundled_rules_block_the_ad_tag_loader_at_the_root`. The measurements below
were taken with 59 rules, before that line existed.

Unit tests: `cargo test -p craft-adblock` (rule groups, loading, matching,
exceptions, cache policy, validation, fail-open on unparsable requests) and
`cargo test -p craft-ui --lib` (destination mapping, startup install, one pass at
a time, counters).

## Engine independence

The hook is Servo-specific glue in `craft-ui`. The decision half is not:
`craft-adblock` has no engine dependency, and neither does the list cache or its
policy, so the Blink/CEF move replaces step 1 with CEF's request interception and
keeps everything else (`docs/engine-decision.md` step 4).
