# CraftBrowsing Project Roadmap & Release Schedule

This document serves as the **authoritative single source of truth** for CraftBrowsing's engineering milestones, feature specifications, version control schema, and release track lifecycle.

---

## 1. Version Control & Release Strategy

CraftBrowsing strictly adheres to **Semantic Versioning (SemVer 2.0.0)** formatted as `vMAJOR.MINOR.PATCH`:

- **MAJOR (`X.0.0`):** Breaking engine architecture shifts, major IPC protocol breaks, or fundamental storage migrations.
- **MINOR (`0.X.0`):** New functional capabilities, new browser subsystems, or expanded Web platform APIs.
- **PATCH (`0.0.X`):** Memory safety fixes, critical security advisories, filter-list engine performance patches, and bug fixes.

### Release Channels

| Channel | Target Audience | Branch / Git Tag | Update Cycle |
| :--- | :--- | :--- | :--- |
| **Nightly** | Active contributors & core engine testers | `main` | Continuous rolling builds |
| **Beta** | Community testers & plugin developers | `beta` (`vX.Y.0-beta.N`) | Bi-weekly freeze & QA |
| **Stable** | General production end-users | `release` (`vX.Y.Z`) | Monthly scheduled releases & zero-day hotfixes |

---

## 2. Release Milestones & Phase Tracking

### v0.1.0 — Phase 1: Core Foundation & Essential Shell
- [x] Workspace architecture setup with latest stable Rust.
- [ ] Embed Servo runtime with WebRender surface compositor in `craft-engine`.
- [ ] Implement declarative browser chrome with tabs, address bar (Omnibox), and search keyword shortcuts in `craft-app`.
- [ ] Local SQLite storage layer for bookmarks, history, tabs, and user-selected search engines (`craft-storage`).
- [ ] Session restore engine and tab hibernation / memory discarding for inactive tabs.
- [ ] Ephemeral Private Browsing mode with memory-only SQLite (`:memory:`) and zero disk touches.
- [ ] Integrate native adblocking (`adblock-rust`) directly into network request pipeline with per-site quick-toggle.
- [ ] Firefox-style Multi-Account Containers (`UserContextId` cookie jar, cache, and storage partitioning).
- [ ] Containerized Download Manager with HTTP Range pause/resume and streaming `.part` writes (`docs/downloads.md`).
- [ ] Unified Site Permissions prompt manager (camera, mic, notifications, clipboard).
- [ ] Privacy & Security Settings dashboard (`craft://settings/privacy`) with granular enable/disable controls.

### v0.2.0 — Phase 2: Developer Experience, PWAs & Hardware Flashing
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
  - [ ] Media playback pipeline (AV1, VP9, Opus via OS media backends) and Picture-in-Picture (PiP).
  - [ ] Built-in PDF viewer and native print pipeline.
  - [ ] Distraction-free Reader View text extractor.

### v0.3.0 — Phase 3: Zero-Knowledge Local Sync (Opt-In)
- [ ] Cryptographic engine using `x25519-dalek` and `chacha20poly1305` (`docs/sync.md`).
- [ ] Zero-broadcast architecture: network listeners completely dormant until explicitly turned on by user.
- [ ] Local mDNS peer-to-peer device discovery and QR-code/PIN mutual authentication.
- [ ] Fine-grained sync selectors: independent toggles for bookmarks, history, tabs, containers, blocklists, and PWAs.
- [ ] Optional lightweight, zero-knowledge Docker sync relay for cross-network synchronization.

### v0.4.0 — Phase 4: Extensions, Security Hardening & Ecosystem
- [ ] WebExtensions manifest parser and sandboxed JavaScript runtime (Boa Engine / Deno Core).
- [ ] Firefox-compatible extension APIs (`tabs`, `storage`, `contextualIdentities`, `webRequest`).
- [ ] External password manager Native Messaging IPC bridge (Bitwarden, 1Password, KeePassXC).
- [ ] Anti-fingerprinting defenses (canvas blunting, audio context jitter, font enumeration shielding).
- [ ] Built-in DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT) resolver selector with zero-log presets (Quad9, Mullvad).
- [ ] Customizable theme engine and UI chrome layout presets (including vertical tabs).

---

## 3. Maintenance & Change Log Conventions

All architectural revisions, feature additions, and milestone completions must be recorded in git history using [Conventional Commits](https://www.conventionalcommits.org/):

- `feat(subsystem):` New user-facing or architectural capability.
- `fix(subsystem):` Bug, memory, or security patch.
- `docs(spec):` Updates to specifications, roadmap, or documentation.
- `sec(perimeter):` Critical security boundary or sandboxing enhancement.
