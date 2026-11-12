# Architecture Overview

CraftBrowsing is structured as a modular Cargo workspace separating browser chrome, rendering, networking, extensions, containers, and sync.

---

## Workspace Layout

```text
CraftBrowsing/
├── crates/
│   ├── craft-app/           # Main entry point & window management (Iced / Slint)
│   ├── craft-engine/        # Servo engine embedding & WebRender bindings
│   ├── craft-adblock/       # Adblock rule engine (brave/adblock crate)
│   ├── craft-containers/    # Multi-account container isolation, cookie store partitioning
│   ├── craft-sync/          # E2EE local sync client (mDNS / SQLite / ChaCha20)
│   ├── craft-extensions/    # Plugin host, contextualIdentities API, JS runtime (Boa / V8)
│   └── craft-storage/       # Local SQLite database for history, bookmarks, containers, tabs
├── docs/                    # Architectural specifications and developer guides
├── Cargo.toml               # Workspace manifest
└── README.md
```

---

## Core Technologies

| Subsystem | Choice | Purpose |
| :--- | :--- | :--- |
| **Rendering & DOM** | [Servo](https://servo.org/) / WebRender | Parallelized, pure-Rust layout, HTML parsing, and GPU rendering |
| **Browser Chrome (UI)** | [Iced](https://github.com/iced-rs/iced) or [Slint](https://slint.dev/) | High-performance, GPU-accelerated declarative UI |
| **Container Isolation** | `craft-containers` | Cookie jar, IndexedDB, and cache partitioning by Contextual Identity ID |
| **Ad-Blocking** | [`adblock-rust`](https://github.com/brave/adblock-rust) | Sub-millisecond URL filtering matching uBlock / Brave rulesets |
| **Extension Runtime** | [Boa Engine](https://github.com/boa-dev/boa) / [Deno Core](https://github.com/denoland/deno_core) | Secure, sandboxed JavaScript runtime for extension scripts |
| **Storage Engine** | `rusqlite` | Structured local storage for bookmarks, history, containers, and search configs |
| **Crypto & Sync** | `x25519-dalek` & `chacha20poly1305` | Zero-knowledge, peer-to-peer syncing of profile data and containers |
