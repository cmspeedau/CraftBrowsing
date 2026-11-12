# CraftBrowsing

A modern, highly customizable, pure-Rust web browser built on Servo, featuring native ad-blocking, local self-hosted sync, and a Firefox-style extension & theming system.

---

## Key Pillars

1. **Pure Rust Core:** Rendering powered by Servo (`servoshell` / `webrender`) with a responsive native UI (Iced / Slint).
2. **Built-in Ad-Blocking:** Native request-interception engine powered by `adblock` (Brave's pure-Rust engine), supporting standard EasyList & uBlock filter lists.
3. **Local Self-Hosted Sync:** End-to-end encrypted synchronization for bookmarks, history, and tabs using local protocols (mDNS / WebSockets) or self-hosted relays (no central telemetry).
4. **Firefox-Style Extension & Theming System:**
   - Manifest-based declarative extensions (`manifest.json`).
   - WebExtensions API compatibility layer (`browser.tabs`, `browser.storage`, `browser.bookmarks`).
   - `userChrome.css` and `userContent.css` support for complete browser UI customization.
5. **External Password Manager Integration:** Delegation to Bitwarden, 1Password, and KeePassXC via WebExtension content scripts and Native Messaging (`browser.runtime.connectNative`).

---

## Documentation

- [Architecture Overview](docs/architecture.md)
- [Plugin & WebExtensions System](docs/plugin-system.md)
- [Password Manager Integration](docs/passwords.md)
- [Local Sync Protocol](docs/sync.md)
- [Project Roadmap](docs/roadmap.md)

---

## Getting Started (Planned)

```bash
# Clone the repository
git clone https://github.com/cmspeedau/CraftBrowsing.git
cd CraftBrowsing

# Build with Cargo
cargo build --release

# Run CraftBrowsing
cargo run --release
```
