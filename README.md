# CraftBrowsing

A modern, privacy-focused, pure-Rust web browser built on top of the [Servo](https://servo.org/) rendering engine.

CraftBrowsing is designed for users who want total independence from Chromium and Gecko, with native adblocking, multi-account container tabs, progressive web app (PWA) support, a built-in download manager, customizable UI theming, developer tools, hardware flashing access (WebSerial/WebUSB), and an optional, zero-knowledge local sync engine.

---

## Security & Privacy: Mission Critical Foundation

In CraftBrowsing, security and privacy are not optional secondary features—they form the core foundation of every architectural boundary:

- **Zero-Telemetry & Local Defaults:** No phone-home diagnostics, metrics, or default network probes.
- **Pure-Rust Memory Safety:** Rendered via Servo and WebRender, eliminating entire classes of memory safety bugs (use-after-free, buffer overflows).
- **Process Isolation & OS Sandboxing:** Worker threads, rendering surfaces, and extension runtimes operate in isolated sandboxes.
- **Strict Anti-Fingerprinting:** Standardized canvas readback noise, WebGL uniform blunting, AudioContext jitter, and user-agent normalization.
- **HTTPS-First & Encrypted DNS:** Strict HTTPS enforcement with configurable DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT).
- **Zero-Knowledge Architecture:** No native cloud credential storage; external password managers (Bitwarden, 1Password, KeePassXC) handle keys via Native Messaging.
- **Zero-Broadcast Local Sync:** Completely dormant until explicitly turned on by the user; encrypted end-to-end via ChaCha20-Poly1305 and X25519.

For full architectural details, see [docs/security-and-privacy.md](docs/security-and-privacy.md).

---

## Key Highlights

- **Pure Rust Engine:** Powered by Servo and WebRender for fast, memory-safe layout and parallel rendering.
- **Native Ad & Tracker Blocking:** Integrated filter-list parsing via `adblock-rust` running directly in the network pipeline.
- **Inbuilt Containers:** Firefox-style contextual identities isolating cookies, storage, and sessions per tab.
- **Progressive Web Apps (PWAs):** Install web apps into standalone, chromeless windows with native OS shortcuts, full adblocking, and container isolation.
- **Built-in Download Manager:** Pause/resume support via HTTP Range requests, container-aware cookie/proxy routing, streaming disk writes (`.part`), and executable safety prompts.
- **Hardware Flashing & Device Access (WebSerial / WebUSB / WebHID):** Opt-in, zero-trust hardware access allowing browser-based MCU flashing (ESP32, Arduino, WebDFU, QMK) with explicit permission prompts, container scoping, and instant disconnect killswitches.
- **Local Self-Hosted Sync (Disabled by Default):** Complete user privacy out of the box. Zero network sync broadcasts or background listeners until explicitly turned on by the user. Supports E2EE local mDNS sync and optional self-hosted relays for bookmarks, history, tabs, containers, adblock filter lists, and installed PWAs.
- **Built-in Developer Tools:** In-process DOM/CSS inspector, interactive JS console, network waterfall monitor, and remote debugging protocol (CDP).
- **WebExtensions & UI Customization:** Firefox-like plugin architecture and customizable UI chrome.
- **External Password Manager Integration:** Zero-knowledge credential handling delegating directly to Bitwarden, 1Password, or KeePassXC via standard native messaging.

---

## Documentation

- [Security & Privacy Architecture](docs/security-and-privacy.md)
- [Architecture Overview](docs/architecture.md)
- [Containers Specification](docs/containers.md)
- [Progressive Web Apps (PWA)](docs/pwa.md)
- [Download Manager](docs/downloads.md)
- [Hardware & Device Access (WebSerial/WebUSB)](docs/hardware-and-devices.md)
- [Local Sync Engine](docs/sync.md)
- [Web Developer Tools](docs/devtools.md)
- [Plugin System & Extensions](docs/plugin-system.md)
- [Passwords & External Manager Integration](docs/passwords.md)
- [Roadmap](docs/roadmap.md)

---

## Roadmap

### Phase 1: Core Foundation & Essential UX
- [ ] Embed Servo runtime with WebRender surface compositor in `craft-engine`.
- [ ] Implement declarative browser chrome with tabs, address bar (Omnibox), and search keyword shortcuts in `craft-app`.
- [ ] Build session restore engine and tab hibernation / memory discarding for inactive tabs.
- [ ] Implement Private Browsing mode with ephemeral memory-backed storage.
- [ ] Hook `craft-adblock` into the network request interception pipeline.
- [ ] Introduce containerized cookie & storage isolation (`docs/containers.md`).
- [ ] Build basic download manager with pause/resume and container context (`docs/downloads.md`).
- [ ] Unified Site Permissions prompt manager (camera, microphone, notifications, clipboard).

### Phase 2: Media, Hardware Flashing, DevTools & Sync
- [ ] Build in-process Web Developer Tools panel (`docs/devtools.md`).
- [ ] Add Chrome DevTools Protocol (CDP) WebSocket listener for remote debugging.
- [ ] Implement Progressive Web App (PWA) installation and standalone windows (`docs/pwa.md`).
- [ ] Implement `craft-hardware` for WebSerial, WebUSB, and WebHID device flashing with zero-trust prompts (`docs/hardware-and-devices.md`).
- [ ] Media playback pipeline (AV1, VP9, Opus via OS media backends) and Picture-in-Picture (PiP).
- [ ] Built-in PDF viewer and print layout integration.
- [ ] Distraction-free Reader View text extractor.
- [ ] Implement opt-in peer-to-peer sync for bookmarks, history, containers, block lists, and PWAs (`docs/sync.md`).

### Phase 3: Ecosystem, Extensibility & Advanced Security
- [ ] WebExtensions runtime supporting Manifest v2/v3 APIs (`docs/plugin-system.md`).
- [ ] Native messaging bridge for external password managers (Bitwarden, KeePassXC).
- [ ] Community theming and customizable UI layouts (including vertical tabs).
- [ ] Anti-fingerprinting protections (canvas blunting, audio context jitter, referrer trimming).
- [ ] Built-in DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT) resolver selector.

---

## Rust Toolchain

CraftBrowsing targets the latest stable Rust release:

```bash
rustup update stable
cargo build
```
