# Internal Pages (`craft://`)

Short reference for the pages this browser serves itself, and the constraints that
make adding one non-trivial.

## What exists

| URL | Purpose |
| --- | --- |
| `craft://welcome` | Start page for new tabs. States what the browser is, which engine it renders with, and what the shields are doing right now. |
| `craft://shields` | Tracking protection status: rule sources with counts and age, requests refused this session, paused sites, where downloaded lists are cached, licence attribution, and what protection does not do yet. |
| `craft://settings` | An inventory split in two: what you can control today, and what is designed but not implemented. |
| `craft://<anything else>` | A 404 page that lists the pages above. |

Code: `crates/craft-ui/src/internal.rs`. The scheme is registered on the engine in
`craft-ui::window::resumed` through `servo::protocol_handler::ProtocolRegistry`.

## What does not exist yet

Feature documents name settings pages that have never been built:
`craft://settings/privacy`, `/security`, `/hardware`, `/sync`, `/updates`,
`/backup`, `/media`, plus `craft://downloads` and `craft://feeds`. Typing one now
returns our own 404 page rather than a failed load, but it is still a dead link.
`craft://settings` is the honest replacement until each area is implemented.

## Constraints on adding a page

1. **The handler runs off the UI thread and must be `Send + Sync`.** The browser
   state it needs is therefore a plain-data snapshot
   (`ShieldStatus`) published into an `Arc<Mutex<..>>` by the chrome. It cannot
   hold an `AdblockManager` or a `WebView`: `adblock`'s engine and Servo's page
   handles are neither `Send` nor `Sync`.
2. **Nothing on the page can act back on the browser.** There is no
   JavaScript-to-embedder bridge in this engine integration, so a button here
   would be a dead control. Controls belong in the egui chrome; these pages
   report.
3. **Scripts and subresources are impossible by construction.** Every response
   carries `Content-Security-Policy: default-src 'none'; style-src
   'unsafe-inline'`. Inline styles are allowed because the pages ship their own;
   nothing else is.
4. **Everything rendered here must be escaped.** List titles come from remote
   `! Title:` headers, hosts and pass outcome lines from the network, and they are
   all rendered into a privileged `craft://` document. `esc()` is applied to every
   such value and a test feeds the renderer markup masquerading as a title.
5. **The page must state what is not true yet.** These pages are where the
   project's honesty about itself is user-visible, so a claim on them has to match
   the code. `render_page` and the copy are covered by tests in
   `crates/craft-ui/src/internal.rs`, including a check that no page contains a
   `<script`, `<img`, `<link` or remote `url(...)`.

## Adding a page

1. Add the route in `route_of` (aliases go there, not in the matcher arms).
2. Write a renderer taking `&ShieldStatus`; extend that struct with plain data if
   the page needs more, and publish it from `BrowserApp::publish_shield_status`.
3. Add the page to `not_found`'s list.
4. Tests: route normalisation, the rendered content, escaping of any new
   attacker-reachable field, and the no-external-resources check.

## Verifying

```sh
RUST_LOG=info cargo run -p craft-ui
# craft://welcome  -> INFO page title: "CraftBrowsing"
# craft://shields  -> INFO page title: "Tracking protection"
# craft://settings -> INFO page title: "Settings"
# craft://nonsense -> INFO page title: "No such page"
# each followed by  INFO load status: Complete
```

A title line means the engine fetched the document from our handler and parsed it;
`Complete` means it rendered. `cargo test -p craft-ui --lib internal` covers the
rendering side without a window.
