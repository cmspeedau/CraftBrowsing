# CraftBrowsing Project Roadmap

A privacy-first browser with a Rust application layer, rendering on Servo today, designed to put user control first. The engine decision (Blink/Chromium as the target primary engine, Servo kept re-evaluable) is recorded in [docs/engine-decision.md](engine-decision.md).

## Vision

CraftBrowsing is a next-generation, highly customizable web browser with a Rust application layer and a swappable rendering engine (Servo today, Blink/Chromium planned; see `docs/engine-decision.md`). It prioritizes performance, privacy, and user sovereignty, offering a modern browsing experience with advanced features like Multi-Account Containers, integrated adblocking, and local-first data management.

> Roadmap checkboxes below describe the whole design goal. Each item is ticked only
> when the behavior is reachable in the running browser, not when a crate
> implements it. Where a subsystem exists but is not yet wired into the app, the
> entry says so.
>
> **Integration status (verified 2026-11-15).** The workspace builds one binary,
> `craft-ui`, which depends on `craft-core`, `craft-engine`, `craft-adblock`
> (network matcher, consulted per request by the engine's resource hook) and
> `craft-sync` (preferences only). `craft-storage` and
> `craft-transfers` are complete, tested crates that no binary links yet, so
> history/bookmarks/permission persistence, private-browsing profiles, the
> partitioned cookie jars, and the download manager are reachable through their
> own tests, not through the browser. `craft-plugins` is the same. Items below
> that describe crate-level work already done say so in place.
>
> Six Phase 1 items are ticked for work that exists in a crate or in the chrome
> model but is **not reachable in the running browser yet**, checked by call
> sites on 2026-11-15: the download manager (no download path in `craft-ui`), the
> site permissions prompt manager (none in `craft-ui`), the `craft://settings/*`
> privacy dashboard (no internal pages are registered, so those URLs fail to
> load), session restore and tab hibernation/discarding (`hibernate_tab`,
> `discard_idle_tabs`, `serialize_session`/`restore_session` have no caller), tab
> grouping (`nest_tab`/`toggle_collapse` have no caller), and ephemeral private
> browsing (no profile selection in the shell). Each needs its wiring item before
> the tick describes the product. The CI/release matrix item is real: three OS
> targets with AppImage and `zsync` metadata.
>
> Also described in `README.md` and `docs/security-and-privacy.md` but with no
> implementation anywhere in the workspace, checked by search on 2026-11-15:
> anti-fingerprinting (canvas readback noise, WebGL blunting, AudioContext
> jitter, user-agent normalization), HTTPS-Only upgrading and its interstitial,
> DNS-over-HTTPS / DNS-over-TLS, Encrypted Client Hello, and per-container
> protection overrides. Network tracking protection is the one control that now
> genuinely runs (`docs/tracking-protection.md`).

## 3. Release Milestones & Phase Tracking

### v0.1.0 — Phase 1: Core Foundation, Tabs & Essential Shell
- [x] Workspace architecture setup with latest stable Rust.
- [x] Embed Servo runtime with WebRender surface compositor. (A real `servo` 0.8.0 embed: a single `WebView` renders through WebRender into an offscreen context that the egui chrome composites; omnibox, back/forward/reload, HiDPI, resize, and URL/title callbacks are wired. The window creates and paints the page in `craft-ui::window`; page operations go through the `Engine` / `PageController` boundary in `craft-engine`, whose simulated command bridge has been deleted. Pending: per-tab rendering contexts, and moving paint/compositing behind the boundary (Phase 2b, see `docs/engine-decision.md`). Containers and multiprocess content processes are still pending; see `docs/architecture.md` and the `servo-engine-integration` skill.)
- [x] `Engine` trait boundary in `craft-engine`: the control plane (tab records, navigation, page snapshots, event diffing) is backend-agnostic, the chrome holds `Box<dyn Engine>`, and the Servo backend implements `PageController` over its `WebView`. Page truth (URL, title, load state, history) now has exactly one source, which made a Servo error page visible in the chrome instead of a fabricated success. Phase 2b and the Blink/CEF backend remain; see `docs/engine-decision.md`.
- [x] Implement declarative browser chrome with horizontal and vertical tabs layout engine in `craft-app` (`docs/tabs-and-groups.md`).
- [x] Tab Grouping, color-coded clusters, and direct Multi-Account Container binding.
- [x] Session restore engine and memory hibernation / tab discarding for inactive tabs.
- [x] Local SQLite storage layer for bookmarks, history, tabs, and user-selected search engines (`craft-storage`).
- [x] Ephemeral Private Browsing mode with memory-only SQLite (`:memory:`) and zero disk touches.
- [x] Route every page request through the native `adblock-rust` matcher, with the address-bar quick toggle. (Done 2026-11-15: Servo's `WebViewDelegate::load_web_resource` hook consults the matcher per subresource and cancels matched loads, top-level documents excepted. Verified on a live news page: 59 bundled rules compiled, 25 requests refused, page still rendered. See `docs/tracking-protection.md`.)
- [ ] Load real filter lists from upstream (EasyList, EasyPrivacy, uBlock filters) with user-triggered updates and a local cache. (The shipped set is 59 curated endpoint rules in `crates/craft-adblock/lists/default-network-filters.txt`. An updater must stay explicit: fetching lists at startup would be a default network call, which the privacy invariants forbid.)
- [ ] Apply cosmetic and element-hiding rules. (`adblock-rust` parses them into the engine; nothing delivers `get_cosmetic_styles` results to the page, so a `$elemhide` or `###selector` rule is inert.)
- [ ] Per-site shield controls in the settings surface. (`add_site_exception` and `remove_site_exception` exist and are tested, and the matcher honors them, but the chrome only toggles protection globally because internal pages such as `craft://settings/shields` are not registered with the engine.)
- [ ] Firefox-style Multi-Account Containers (`UserContextId` cookie jar, cache, and storage partitioning). (Partly built as of 2026-11-15: container identities with colors/icons exist in the chrome, `craft-storage` implements partitioned cookie jars, origin storage and cache keyed by `container_id` and purges with the container, but the browser does not use `craft-storage` and Servo runs with `temporary_storage: true` on a single shared `WebView`, so nothing is partitioned per container yet. Needs per-container page handles, which arrive with the per-tab surfaces of Phase 2b.)
- [x] Container-aware tab model: each tab and engine record carries its `ContainerId`, and container-scoped new-tab actions in the chrome pass the real id (`craft-engine::TabRecord.container_id`).
- [x] Containerized Download Manager with HTTP Range pause/resume and streaming `.part` writes (`docs/downloads.md`).
- [x] Unified Site Permissions prompt manager (camera, mic, notifications, clipboard).
- [x] Privacy & Security Settings dashboard (`craft://settings/privacy`) with granular enable/disable controls.
- [x] CI/CD release matrix for Windows (`.exe`/`.msix`), macOS (`.dmg`), and Linux AppImage with `zsync` (`docs/releases-and-packaging.md`).

### v0.2.0 — Phase 2: Developer Tools, Media Hub, PWAs & Hardware Flashing
- [ ] **Global Media Hub & PiP Scrubber (`docs/media-hub.md`):**
  - [ ] Unified toolbar media controller for background audio/video playback.
  - [ ] Picture-in-Picture (PiP) window with bidirectional seek bar, volume scrubber, and container badge.
  - [ ] Global keyboard shortcuts and OS system media keys integration (MPRIS on Linux, SMTC on Windows, Now Playing on macOS).
- [ ] **In-Process Web Developer Tools (`docs/devtools.md`):**
  - [ ] Live DOM tree & CSS inspector with box-model visualizer.
  - [ ] Network waterfall monitor with header inspection and adblock filter hit badges.
  - [ ] Interactive JavaScript Console (REPL).
  - [ ] Containerized Storage Inspector (`localStorage`, `sessionStorage`, Cookies, IndexedDB).
- [ ] **Chrome DevTools Protocol (CDP) Bridge:** WebSocket endpoint for remote debugging and headless test automation.
- [ ] **Progressive Web Apps (`docs/pwa.md`):** W3C manifest parser, standalone chromeless windows, native OS shortcuts, and container context inheritance.
- [ ] **Hardware Flashing & Device Access (`docs/hardware-and-devices.md`):**
  - [ ] `craft-hardware` engine bridge for WebSerial, WebUSB, and WebHID.
  - [ ] Native device picker dialogs with VID/PID inspection (ESP32, Arduino, WebDFU, QMK).
  - [ ] Zero-trust permission model with instant address-bar killswitch (disabled by default).
- [ ] **Media & Document Reader:**
  - [ ] Media playback pipeline (AV1, VP9, Opus via OS media backends).
  - [ ] Built-in PDF viewer and native print pipeline.
  - [ ] Distraction-free Reader View text extractor.

### v0.3.0 — Phase 3: Zero-Knowledge Local Sync & Built-in RSS Reader
- [ ] **Local Feed Reader (`craft://feeds`) (`docs/rss-reader.md`):**
  - [ ] Built-in discovery of RSS 2.0, Atom, and JSON feeds via address bar icon.
  - [ ] Pure-Rust feed parser and offline SQLite feed article cache.
  - [ ] Reader View integration for clutter-free offline article reading.
  - [ ] Feed subscription export/import via OPML (XML).
- [ ] **Zero-Knowledge Local Sync (Opt-In) (`docs/sync.md`):**
  - [ ] Cryptographic engine using `x25519-dalek` and `chacha20poly1305`.
  - [ ] Zero-broadcast architecture: network listeners completely dormant until explicitly turned on by user.
  - [ ] Local mDNS peer-to-peer device discovery and QR-code/PIN mutual authentication.
  - [ ] Fine-grained sync selectors: independent toggles for bookmarks, history, tabs, containers, blocklists, feeds, and PWAs.
  - [ ] Optional lightweight, zero-knowledge Docker sync relay for cross-network synchronization.

### v0.4.0 — Phase 4: Portability, Extensions & Ecosystem Hardening
- [ ] **Backup, Profile Export & Portability (`docs/backup-and-portability.md`):**
  - [ ] One-click encrypted `.craftvault` export/import using ChaCha20-Poly1305 and Argon2id.
  - [ ] CLI/Flag portable mode (`craft-app --portable`) storing configuration and databases alongside binary.
  - [ ] Selective import/export wizard for air-gapped machine transfers.
- [ ] **WebExtensions Sandbox & Extension Runtime (`docs/plugin-system.md`):**
  - [ ] Manifest v2/v3 parser and sandboxed JavaScript runtime (Boa Engine / Deno Core).
  - [ ] Firefox-compatible extension APIs (`tabs`, `storage`, `contextualIdentities`, `webRequest`).
  - [ ] External password manager Native Messaging IPC bridge (Bitwarden, 1Password, KeePassXC).
- [ ] **Advanced Security Hardening:**
  - [ ] Anti-fingerprinting defenses (canvas blunting, audio context jitter, font enumeration shielding).
  - [ ] Built-in DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT) resolver selector with zero-log presets (Quad9, Mullvad).
- [ ] In-browser auto-update background worker integration for Windows, macOS, and Linux AppImage zsync.

### v0.5.0 — Phase 5: Built-in Transfer Engine (Magnet + Segmented HTTP)
- [ ] **Vendor torrenter core modules into `craft-transfers`:**
  - [ ] Add `craft-transfers` crate to the workspace.
  - [ ] Vendor and adapt `validate`, `paths`, `progress`, and job-state helpers.
  - [ ] Vendor segmented HTTP downloader (4-part byte-range resume).
  - [ ] Vendor magnet/torrent engine (librqbit, DHT in memory only).
- [ ] **Unified transfer lifecycle:**
  - [ ] Start, pause, resume, stop, restart, clear.
  - [ ] Concurrency cap (default 4) with queued jobs.
  - [ ] Restore jobs across browser restarts with missing-file detection.
- [ ] **Storage & schema:**
  - [ ] Bump schema to v5 with unified transfer records.
  - [ ] Persist magnet info hashes, seed mode, container ID, and resume metadata.
  - [ ] Transfer settings (default dir, concurrency, seed default, DHT, port range).
- [ ] **Container awareness:**
  - [ ] Tag every transfer with its originating container.
  - [ ] Filter `craft://downloads` by container.
  - [ ] Preserve container cookie/proxy heritage for HTTP transfers.
- [ ] **Privacy & security controls:**
  - [ ] Leech mode by default; Share mode explicit opt-in.
  - [ ] DHT in memory only; no peer persistence.
  - [ ] User-gesture + confirmation for magnet links.
  - [ ] Clear-on-exit and private-browsing handling for transfer history.
- [ ] **UI:**
  - [ ] Extend `craft://downloads` with HTTP/MAGNET badges, peers, speed, ETA.
  - [ ] Toolbar download indicator for all transfer kinds.
  - [ ] Magnet link interception prompt.
- [ ] **Documentation:**
  - [ ] `docs/transfers.md`
  - [ ] Update `docs/downloads.md`, `docs/architecture.md`, and security docs.
- [ ] Explicitly out of scope for this milestone: torrenter MCP server integration.
