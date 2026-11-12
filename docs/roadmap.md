# Project Roadmap

CraftBrowsing development phases:

---

## Phase 1: Engine & Shell Foundations
- [ ] Initialize Cargo workspace (`craft-app`, `craft-engine`, `craft-storage`).
- [ ] Integrate Servo embedder with basic window management and navigation controls.
- [ ] Implement SQLite storage for history and bookmarks.

## Phase 2: Privacy & Blocking
- [ ] Integrate `adblock-rust` for subresource request filtering.
- [ ] Add filter list manager (EasyList, EasyPrivacy, uBlock filters).
- [ ] Configurable search engine switcher (DuckDuckGo, Brave, Startpage, SearXNG).

## Phase 3: Extension Host & Theming
- [ ] Implement `manifest.json` parser.
- [ ] Embed JS engine (Boa / Deno core) for background scripts.
- [ ] Expose `browser.tabs`, `browser.storage`, and `browser.bookmarks` APIs.
- [ ] Support `userChrome.css` and `userContent.css`.

## Phase 4: Password Manager & Native Messaging
- [ ] Support WebExtension content scripts for form detection.
- [ ] Implement Native Messaging Host protocol (`stdin`/`stdout` IPC) for Bitwarden & 1Password.

## Phase 5: Local E2EE Sync
- [ ] Implement ChaCha20-Poly1305 encryption for SQLite snapshots.
- [ ] mDNS local network discovery and peer-to-peer synchronization.
- [ ] Standalone lightweight relay server binary.
