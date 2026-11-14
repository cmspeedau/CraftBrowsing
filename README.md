# CraftBrowsing

A modern, privacy-focused, pure-Rust web browser built on top of the [Servo](https://servo.org/) rendering engine.

CraftBrowsing is designed for users who want total independence from Chromium and Gecko, with native adblocking, multi-account container tabs, progressive web app (PWA) support, a built-in download manager, customizable UI theming, developer tools, hardware flashing access (WebSerial/WebUSB), vertical tabs & tab grouping, local RSS reader, encrypted portable vault backups, global media controls with PiP scrubber, and an optional, zero-knowledge local sync engine.

---

## Security & Privacy: Mission Critical & User-Controlled

In CraftBrowsing, security and privacy are not optional secondary features—they form the core foundation of every architectural boundary, **with granular user enable/disable control over every feature**:

- **User Sovereignty & Granular Toggles (`craft://settings/privacy`):** Every single privacy protection (ad/tracker blocking, anti-fingerprinting, HTTPS-Only, DoH, containers, hardware device access) can be individually toggled on/off globally or overridden on a per-site / per-container basis.
- **Zero-Telemetry & Local Defaults:** No phone-home diagnostics, metrics, or default network probes.
- **Pure-Rust Memory Safety:** Rendered via Servo and WebRender, eliminating entire classes of memory safety bugs (use-after-free, buffer overflows).
- **Process Isolation & OS Sandboxing:** Worker threads, rendering surfaces, and extension runtimes operate in isolated sandboxes.
- **Configurable Anti-Fingerprinting:** Standardized canvas readback noise, WebGL uniform blunting, AudioContext jitter, and user-agent normalization (all with user disable options for web app compatibility).
- **HTTPS-First & Encrypted DNS:** Strict HTTPS enforcement with configurable DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT) and custom resolver support.
- **Zero-Knowledge Architecture:** No native cloud credential storage; external password managers (Bitwarden, 1Password, KeePassXC) handle keys via Native Messaging.
- **Zero-Broadcast Local Sync (Off by Default):** Completely dormant until explicitly turned on by the user; encrypted end-to-end via ChaCha20-Poly1305 and X25519.

For full architectural details and the settings matrix, see [docs/security-and-privacy.md](docs/security-and-privacy.md).

---

## Key Highlights

- **Pure Rust Engine:** Powered by Servo and WebRender for fast, memory-safe layout and parallel rendering.
- **Cross-Platform Releases & Auto-Update:** Native builds for Windows (with silent background auto-updates), macOS (universal notarized `.dmg` with in-app updater), and Linux (**AppImage with `zsync` delta updates**).
- **Native Ad & Tracker Blocking:** Integrated filter-list parsing via `adblock-rust` running directly in the network pipeline (with per-site shield toggle).
- **Inbuilt Containers:** Firefox-style contextual identities isolating cookies, storage, and sessions per tab.
- **Tab Groups & Vertical Tabs:** Color-coded collapsible groups, container auto-binding, and idle memory hibernation (`docs/tabs-and-groups.md`).
- **Global Media Hub & PiP Scrubber:** Central toolbar popup for background media playback with timeline scrubbers and mute toggles (`docs/media-hub.md`).
- **Local RSS/Atom Feed Reader (`craft://feeds`):** In-browser feed subscription, full-text offline caching, zero cloud dependencies (`docs/rss-reader.md`).
- **Encrypted Vault Export & Portable Mode:** Password-encrypted `.craftvault` export (ChaCha20-Poly1305) and zero-footprint `--portable` execution from USB drives (`docs/backup-and-portability.md`).
- **Progressive Web Apps (PWAs):** Install web apps into standalone, chromeless windows with native OS shortcuts, full adblocking, and container isolation.
- **Built-in Download Manager:** Pause/resume support via HTTP Range requests, container-aware cookie/proxy routing, streaming disk writes (`.part`), and executable safety prompts.
- **Hardware Flashing & Device Access (WebSerial / WebUSB / WebHID):** Opt-in, zero-trust hardware access allowing browser-based MCU flashing (ESP32, Arduino, WebDFU, QMK) with explicit permission prompts, container scoping, and instant disconnect killswitches.
- **Local Self-Hosted Sync (Disabled by Default):** Complete user privacy out of the box. Zero network sync broadcasts or background listeners until explicitly turned on by the user. Supports E2EE local mDNS sync and optional self-hosted relays for bookmarks, history, tabs, containers, adblock filter lists, installed PWAs, and RSS feeds.
- **Built-in Developer Tools:** In-process DOM/CSS inspector, interactive JS console, network waterfall monitor, and remote debugging protocol (CDP).
- **WebExtensions & UI Customization:** Firefox-like plugin architecture and customizable UI chrome.
- **External Password Manager Integration:** Zero-knowledge credential handling delegating directly to Bitwarden, 1Password, or KeePassXC via standard native messaging.

---

## Documentation

- [Project Roadmap & Release Schedule](docs/roadmap.md) *(Authoritative Source of Truth)*
- [Packaging, Distribution & Auto-Update Architecture](docs/releases-and-packaging.md)
- [Tab Management, Groups & Vertical Tabs](docs/tabs-and-groups.md)
- [Local RSS/Atom Feed Reader](docs/rss-reader.md)
- [Backup, Encrypted Vault Export & Portability](docs/backup-and-portability.md)
- [Global Media Hub & Picture-in-Picture](docs/media-hub.md)
- [Security & Privacy Architecture & User Toggles](docs/security-and-privacy.md)
- [Architecture Overview](docs/architecture.md)
- [Containers Specification](docs/containers.md)
- [Progressive Web Apps (PWA)](docs/pwa.md)
- [Download Manager](docs/downloads.md)
- [Hardware & Device Access (WebSerial/WebUSB)](docs/hardware-and-devices.md)
- [Local Sync Engine](docs/sync.md)
- [Web Developer Tools](docs/devtools.md)
- [Plugin System & Extensions](docs/plugin-system.md)
- [Passwords & External Manager Integration](docs/passwords.md)

---

## Rust Toolchain

CraftBrowsing targets the latest stable Rust release:

```bash
rustup update stable
cargo build
```

### Build notes

CraftBrowsing links Servo 0.8, which has two build-time requirements that are easy to hit:

- **Style-thread stack.** `.cargo/config.toml` sets `SERVO_STYLE_THREAD_STACK_SIZE_KB = "8192"`. Stylo reads this at compile time; with the 512 KB default, deeply nested CSS on sites like Facebook overflows a style worker and aborts the process. It only reserves virtual address space, so the cost is negligible (upstream Servo uses the same value, see servo/servo#43888). If you change it, rebuild stylo: `cargo clean -p stylo && cargo build`.
- **`rust-version`.** Servo 0.8 needs Rust 1.88+, so `craft-storage` and `craft-transfers` declare `rust-version = "1.88.0"` and the workspace resolves `image` and other deps against that.

Servo also needs native libraries for GPU media and the web engine (fontconfig, OpenGL/EGL, and on Linux the X11/Wayland client libraries). The default features enable `webgpu`, `webxr`, and `media-gstreamer`; trim them in `Cargo.toml` if your target does not need them.

