# Local Self-Hosted Sync

CraftBrowsing offers peer-to-peer, end-to-end encrypted synchronization without reliance on centralized third-party servers.

---

## Privacy First: Disabled by Default

In keeping with a strict privacy-by-default posture:
- **Default State: Disabled.** CraftBrowsing will never initiate network discovery (mDNS), open background sockets, or broadcast device availability until explicitly enabled by the user in settings (`craft://settings/sync`).
- **Explicit User Opt-In:** Enabling sync requires generating or pairing via an encryption passphrase / device recovery key.
- **Granular Controls:** When enabled, users can selectively toggle what syncs (Bookmarks, History, Container Rules, Tabs).

---

## Features

- **End-to-End Encryption:** Profile databases (bookmarks, history, open tabs, container definitions) are encrypted locally using ChaCha20-Poly1305 before transmission.
- **Container Synchronization:** Container setups (names, icons, colors, domain routing rules) and container-tagged open tabs sync seamlessly across devices. (Session cookies and credentials remain local only).
- **Local Network Discovery (Opt-In):** When sync is toggled on, uses mDNS (`mdns-sd`) to discover other CraftBrowsing instances on the local Wi-Fi/LAN automatically.
- **Self-Hosted Relay Option:** For syncing across different networks, users can run a lightweight Docker container relaying encrypted blobs over WebSockets.
- **Conflict Resolution:** CRDTs (Conflict-free Replicated Data Types) or Last-Write-Wins (LWW) timestamps manage concurrent updates for bookmarks, history, and containers.
