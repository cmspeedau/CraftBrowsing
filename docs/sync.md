# Local Self-Hosted Sync

CraftBrowsing offers peer-to-peer, end-to-end encrypted synchronization without reliance on centralized third-party servers.

---

## Features

- **End-to-End Encryption:** Profile databases (bookmarks, history, open tabs, container definitions) are encrypted locally using ChaCha20-Poly1305 before transmission.
- **Container Synchronization:** Container setups (names, icons, colors, domain routing rules) and container-tagged open tabs sync seamlessly across devices.
- **Local Network Discovery:** Uses mDNS (`mdns-sd`) to discover other CraftBrowsing instances on the local Wi-Fi/LAN automatically.
- **Self-Hosted Relay Option:** For syncing across different networks, users can run a lightweight Docker container relaying encrypted blobs over WebSockets.
- **Conflict Resolution:** CRDTs (Conflict-free Replicated Data Types) or Last-Write-Wins (LWW) timestamps manage concurrent updates for bookmarks, history, and containers.
