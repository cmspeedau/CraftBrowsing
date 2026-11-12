# Architecture Overview

CraftBrowsing is structured as a modular Cargo workspace separating browser chrome, rendering, networking, downloads, devtools, extensions, containers, PWAs, and sync.

---

## Workspace Layout

```text
CraftBrowsing/
├── crates/
│   ├── craft-app/           # Main entry point & window management (Iced / Slint)
│   ├── craft-engine/         # Servo engine embedding & WebRender bindings
│   ├── craft-adblock/        # Adblock rule engine (brave/adblock crate)
│   ├── craft-downloads/      # Download manager (HTTP Range, pause/resume, stream writes)
│   ├── craft-pwa/            # Progressive Web App manager, manifest parser & OS shortcuts
│   ├── craft-devtools/       # Web Developer Tools (DOM inspector, network, console, storage)
│   ├── craft-sync/           # E2EE local sync client (Disabled by default, opt-in)
│   ├── craft-extensions/     # Plugin host, manifest parser, JS runtime (Boa / V8)
│   └── craft-storage/        # Local SQLite database for history, bookmarks, tabs, containers, downloads, PWAs
├── docs/                     # Architectural specifications and developer guides
├── Cargo.toml                # Workspace manifest
└── README.md
```

---

## Core Technologies

| Subsystem | Choice | Purpose |
| :--- | :--- | :--- |
| **Rendering & DOM** | [Servo](https://servo.org/) / WebRender | Parallelized, pure-Rust layout, HTML parsing, and GPU rendering |
| **Browser Chrome (UI)** | [Iced](https://github.com/iced-rs/iced) or [Slint](https://slint.dev/) | High-performance, GPU-accelerated declarative UI |
| **Ad-Blocking** | [`adblock-rust`](https://github.com/brave/adblock-rust) | Sub-millisecond URL filtering matching uBlock / Brave rulesets |
| **Download Manager** | `craft-downloads` (`tokio::io`, `reqwest`) | Resumable chunked file transfers, container proxy/cookie context, and `.part` streaming |
| **Progressive Web Apps** | `craft-pwa` | W3C manifest parsing, chromeless standalone windows, and OS desktop shortcuts |
| **Developer Tools** | `craft-devtools` / Chrome DevTools Protocol (CDP) | Native DOM/CSS inspector, console repl, network waterfall, and storage inspector |
| **Extension Runtime** | [Boa Engine](https://github.com/boa-dev/boa) / [Deno Core](https://github.com/denoland/deno_core) | Secure, sandboxed JavaScript runtime for extension background scripts |
| **Storage Engine** | `rusqlite` | Structured local storage for bookmarks, history, containers, downloads, and search configs |
| **Crypto & Sync** | `x25519-dalek` & `chacha20poly1305` | Zero-knowledge, peer-to-peer sync (Opt-in only; inactive by default) |
