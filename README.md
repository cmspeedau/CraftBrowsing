# CraftBrowsing

A modern, privacy-focused, pure-Rust web browser built on top of the [Servo](https://servo.org/) rendering engine.

CraftBrowsing is designed for users who want total independence from Chromium and Gecko, with native adblocking, multi-account container tabs, customizable UI theming, and an optional, zero-knowledge local sync engine.

---

## Key Highlights

- **Pure Rust Engine:** Powered by Servo and WebRender for fast, memory-safe layout and parallel rendering.
- **Native Ad & Tracker Blocking:** Integrated filter-list parsing via `adblock-rust` running directly in the network pipeline.
- **Inbuilt Containers:** Firefox-style contextual identities isolating cookies, storage, and sessions per tab.
- **Local Self-Hosted Sync (Disabled by Default):** Complete user privacy out of the box. Zero network sync broadcasts or background listeners until explicitly turned on by the user. Supports E2EE local mDNS sync and optional self-hosted relays.
- **WebExtensions & UI Customization:** Firefox-like plugin architecture and customizable UI chrome.
- **External Password Manager Integration:** Zero-knowledge credential handling delegating directly to Bitwarden, 1Password, or KeePassXC via standard native messaging.

---

## Documentation

- [Architecture Overview](docs/architecture.md)
- [Containers Specification](docs/containers.md)
- [Local Sync Engine](docs/sync.md)
- [Plugin System & Extensions](docs/plugin-system.md)
- [Passwords & Security](docs/passwords-and-security.md)

---

## Rust Toolchain

CraftBrowsing targets the latest stable Rust release:

```bash
rustup update stable
cargo build
```
