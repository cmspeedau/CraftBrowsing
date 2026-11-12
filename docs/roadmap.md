# CraftBrowsing Project Roadmap

This roadmap outlines the milestones for delivering a pure-Rust, privacy-first web browser.

---

## Phase 1: Engine Foundation & Shell
- [x] Workspace architecture setup with latest stable Rust.
- [ ] Servo engine integration & basic WebRender surface embedding.
- [ ] Core browser chrome (URL bar, tab management, back/forward navigation).
- [ ] SQLite storage layer for bookmarks, history, and search engine defaults.

## Phase 2: Privacy, Containers & Networking
- [ ] Native ad-blocking pipeline using `adblock-rust`.
- [ ] Firefox-style Multi-Account Containers (`UserContextId` cookie jar & storage partitioning).
- [ ] User-configurable search engine selection (DuckDuckGo, SearXNG, Brave Search, etc.).
- [ ] External password manager bridge (Bitwarden, 1Password, KeePassXC via native messaging).

## Phase 3: Built-in Web Developer Tools
- [ ] **DOM & Style Inspector:** Live DOM tree inspection, CSS style rule editor, and box-model viewer.
- [ ] **Network Waterfall:** Real-time request logging, timing breakdowns, header inspection, and adblocker hit badges.
- [ ] **Interactive Console:** JavaScript REPL and console error/warning streaming.
- [ ] **Container Storage Inspector:** Inspect cookies, `localStorage`, `sessionStorage`, and IndexedDB partitioned per container.
- [ ] **Remote Debugging / CDP Bridge:** Optional WebSocket endpoint for remote inspection and headless test automation.

## Phase 4: Extensions & Customization
- [ ] WebExtensions manifest parser and Boa/Deno JS sandbox runtime.
- [ ] Firefox-compatible extension APIs (`tabs`, `storage`, `contextualIdentities`, `webRequest`).
- [ ] Customizable theme engine and user styles.

## Phase 5: Local Self-Hosted Sync (Opt-In)
- [ ] End-to-end encryption engine (`x25519-dalek`, `chacha20poly1305`).
- [ ] Local mDNS peer-to-peer discovery on LAN (disabled by default).
- [ ] Container settings, bookmark, and tab synchronization.
- [ ] Optional lightweight Docker relay server for cross-network syncing.
