# CraftBrowsing Project Roadmap

A privacy-first browser built on Servo, designed to put user control first.

## Vision

CraftBrowsing is a next-generation, highly customizable web browser built on Servo. It prioritizes performance, privacy, and user sovereignty, offering a modern browsing experience with advanced features like Multi-Account Containers, integrated adblocking, and local-first data management.

## 3. Release Milestones & Phase Tracking

### v0.1.0 — Phase 1: Core Foundation, Tabs & Essential Shell
- [x] Workspace architecture setup with latest stable Rust.
- [x] Embed Servo runtime with WebRender surface compositor in `craft-engine`.
- [x] Implement declarative browser chrome with horizontal and vertical tabs layout engine in `craft-app` (`docs/tabs-and-groups.md`).
- [x] Tab Grouping, color-coded clusters, and direct Multi-Account Container binding.
- [x] Session restore engine and memory hibernation / tab discarding for inactive tabs.
- [x] Local SQLite storage layer for bookmarks, history, tabs, and user-selected search engines (`craft-storage`).
- [x] Ephemeral Private Browsing mode with memory-only SQLite (`:memory:`) and zero disk touches.
- [x] Integrate native adblocking (`adblock-rust`) directly into network request pipeline with per-site quick-toggle.
- [x] Firefox-style Multi-Account Containers (`UserContextId` cookie jar, cache, and storage partitioning).
- [x] Containerized Download Manager with HTTP Range pause/resume and streaming `.part` writes (`docs/downloads.md`).
- [ ] Unified Site Permissions prompt manager (camera, mic, notifications, clipboard).
- [x] Privacy & Security Settings dashboard (`craft://settings/privacy`) with granular enable/disable controls.
- [ ] CI/CD release matrix for Windows (`.exe`/`.msix`), macOS (`.dmg`), and Linux AppImage with `zsync` (`docs/releases-and-packaging.md`).

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
