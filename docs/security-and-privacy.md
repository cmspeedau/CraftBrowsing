# Security & Privacy Architecture Specification

In CraftBrowsing, **security and privacy are non-negotiable, mission-critical principles**. The architecture operates under a **Zero-Trust by Default** model: the browser trusts neither arbitrary web content, third-party network listeners, telemetry servers, nor peripheral hardware until explicitly authenticated and authorized by the user.

Equally important is **Granular User Agency**: every security and privacy control is designed with transparent, user-accessible settings (`craft://settings/privacy` and `craft://settings/security`) allowing users to inspect, enable, disable, or customize protection levels to fit their exact workflow (e.g., standard browsing vs. high-security vs. local hardware experimentation).

---

## 1. Zero-Trust Core Architecture & Granular Controls

| Feature / Subsystem | Default State | User Toggle / Options | Enforcement Mechanism |
| :--- | :--- | :--- | :--- |
| **Telemetry & Phone-Home** | **Permanently Off** | Completely eliminated from codebase | Zero background analytics, crash report pings, or telemetry sockets. |
| **Ad & Tracker Blocking** | **Enabled** | Global toggle + per-site exception shield | In-engine request interception using `adblock-rust` on DOM and network requests. |
| **Hardware Device Access** | **Disabled** | Global toggle (`craft://settings/hardware`) + per-site/container permission | WebSerial, WebUSB, and WebHID APIs remain completely inactive until explicitly enabled. |
| **Local P2P Sync** | **Disabled** | Manual opt-in switch (`craft://settings/sync`) | Zero listening ports, zero mDNS broadcasts, and zero network activity until configured. |
| **Containers Isolation** | **Enabled** | Global toggle + container manager (`craft://containers`) | Contextual identities isolating cookies, storage, cache, and TLS connection pools. |
| **DNS-over-HTTPS (DoH)** | **Enabled** | Toggle on/off + custom endpoint selector | Bypasses OS/ISP DNS sniffing with privacy resolvers (Quad9, Mullvad, or custom URL). |
| **HTTPS-Only Mode** | **Enabled** | Toggle on/off + site exception list | Automatically upgrades HTTP to HTTPS; presents full warning interstitial on downgrade. |
| **Anti-Fingerprinting Shield** | **Enabled (Balanced)** | Off / Balanced / Strict modes | Canvas/WebGL jitter, AudioContext farbling, and hardware concurrency normalization. |
| **Tracking Param Stripper** | **Enabled** | Toggle on/off | Cleans tracking tokens (`utm_*`, `fbclid`, `gclid`) from loaded URLs and link clicks. |
| **Referrer Trimming** | **Enabled** | Strict origin-only / Relaxed / Disabled | Trims cross-origin referrers strictly to the origin (`https://example.com/`). |
| **Encrypted Client Hello (ECH)** | **Enabled** | Toggle on/off | Masks Server Name Indication (SNI) during TLS handshake when supported by DNS/server. |
| **External Password Manager** | **Configurable** | Select Bitwarden, KeePassXC, 1Password, or None | Native Messaging IPC via stdin/stdout JSON-RPC; no internal plaintext credential store. |

---

## 2. Process & Memory Isolation Model

To prevent single-compromise vulnerabilities and side-channel data exfiltration:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                         craft-app (Main / UI)                          │
│     - Native window chrome (Iced / Slint)                              │
│     - Tab orchestration, bookmarks, containers, sync state            │
│     - External password manager Native Messaging IPC bridge            │
└─────────────────────────────────┬──────────────────────────────────────┘
                                  │ IPC (Unix Domain Socket / Named Pipe)
                                  ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        craft-engine (Renderers)                        │
│   ┌───────────────────────────┐      ┌───────────────────────────────┐ │
│   │ Tab Process: Container A  │      │ Tab Process: Container B      │ │
│   │ - Servo Layout / DOM      │      │ - Servo Layout / DOM          │ │
│   │ - Ephemeral SQLite state  │      │ - Persistent Container state  │ │
│   │ - Sandboxed JS context    │      │ - Sandboxed JS context        │ │
│   └───────────────────────────┘      └───────────────────────────────┘ │
└─────────────────────────────────┬──────────────────────────────────────┘
                                  │ IPC Channel
                                  ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        craft-hardware (Isolated)                       │
│     - Drops raw network and file system access permissions             │
│     - Strictly mediates WebSerial / WebUSB / WebHID requests           │
│     - Live permission status badge & instant hardware killswitch       │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Process Sandboxing:** Renderers run with restricted privilege rings (seccomp on Linux, AppContainer on Windows, sandbox seatbelt on macOS).
2. **Memory Safety by Construction:** Render tree calculation, style cascades, HTML parsing, and networking run in pure Rust without raw C/C++ pointer arithmetic vulnerabilities.
3. **Container Context Partitioning:** Every HTTP request dispatched through `reqwest` inherits the tab's container cookie jar and connection pool. Connections across different containers never reuse TLS session tickets or HTTP/2 multiplexed streams, preventing cross-container connection correlation.
4. **User Override for Performance or Legacy Support:** Users can adjust process limits or disable strict site isolation on constrained devices via advanced flags.

---

## 3. Network Privacy & Anti-Surveillance Pipeline (With User Toggles)

### 3.1. DNS Privacy & Strict TLS
* **DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT):** Built-in DNS resolver bypassing OS-level ISP DNS sniffing.
  * *User Controls:* Toggle DoH on/off; choose presets (Quad9, Mullvad, Cloudflare) or specify custom RFC 8484 DoH URLs; optional fallback to system DNS toggle.
* **HTTPS-First & HTTPS-Only:** Crafts upgrade requests automatically from `http://` to `https://`. Downgrades present full-page interstitial blocking warnings.
  * *User Controls:* Enable HTTPS-Only everywhere, enable only in Private Browsing, or add permanent site exceptions for local development (`http://localhost`, `http://192.168.*`).
* **Encrypted Client Hello (ECH):** Prevents network snoopers and ISPs from seeing the target Server Name Indication (SNI) during TLS handshakes. Toggleable in settings.

### 3.2. Native Tracking Protection & Content Filtering
* **Filter Execution:** Sub-millisecond rule evaluation (`adblock-rust`) matching EasyList, EasyPrivacy, uBlock Origin filter rules, and URL parameter tracking strippers.
  * *User Controls:* Quick address-bar shield toggle to disable blocking on trusted sites; custom filter list subscription manager (`craft://settings/shields`).
* **CNAME Uncloaking:** Resolves canonical DNS names internally prior to rule evaluation to catch trackers masquerading under first-party subdomains (toggleable).
* **Referrer Trimming:** Cross-origin requests trim referrers strictly to the origin (`https://example.com/` instead of full URL path and search queries). Users can switch between "Strict Origin", "Origin on Cross-Origin", or "Full Referrer (Legacy)".

---

## 4. Advanced Anti-Fingerprinting Defenses & Granular Levels

Fingerprinting scripts attempt to build a persistent hardware signature without using cookies. CraftBrowsing implements systematic randomization and blunting with three user-selectable levels:

```text
craft://settings/fingerprinting
  [ ] Disabled (Maximum compatibility with specialized web apps)
  [X] Balanced (Default: Blocks known fingerprinting scripts, farbles AudioContext & Canvas, normalizes User-Agent)
  [ ] Strict (Maximum privacy: Spoofs hardware concurrency, blocks niche system fonts, forces standard window dimensions)
```

* **Canvas & WebGL Jitter:** Subtle, imperceptible noise injection into HTML5 2D canvas and WebGL readback operations (`toDataURL`, `getImageData`, `readPixels`). Identical scripts run on the same device return diverging hashes across containers.
* **AudioContext Farbling:** Micro-timing perturbations added to Web Audio API dynamics compressions and frequency domain transforms to break acoustic fingerprinting.
* **Font Enumeration Shielding:** Sites can only query standard system base fonts; queries for user-installed niche fonts return `false` to prevent unique font collection signatures.
* **Uniform User-Agent & Hardware Concurrency:** User-Agent strings, OS platform strings, and `navigator.hardwareConcurrency` report standardized tiers to blend into a broad anonymity crowd.
* **Battery & Sensor API Nullification:** Battery Status API, Ambient Light Sensor, and Gyroscope APIs return mock or disabled responses to prevent battery level/hardware physical profiling.
* **Per-Site Exceptions:** Any site encountering functional breakdown (e.g. online DAW or WebGL design tools) can be granted an anti-fingerprinting exception via the Omnibox shield menu.

---

## 5. Ephemeral Private Browsing Architecture

When a Private Browsing window is spawned:
1. **In-Memory SQLite:** All session data (history, local storage, cookies, IndexedDB, cached network responses) is backed by an in-memory SQLite database (`file::memory:?cache=shared`).
2. **Zero Disk Touches:** No cache files, swap partitions, temporary files, or crash logs are written to non-volatile storage.
3. **Cryptographic Key Zeroization:** All in-memory cryptographic keys and decrypted session caches are wrapped in `zeroize::ZeroizeOnDrop` to scrub memory buffers when private tabs/windows are closed.
4. **User Preferences for Standard Browsing:** Users can configure regular browsing to automatically clear history, cookies, or cache upon browser exit (`craft://settings/clear-on-exit`).

---

## 6. Zero-Knowledge Local Sync Cryptography

When synchronization is explicitly turned on by the user (`docs/sync.md`):

```text
Local Store ──► ChaCha20-Poly1305 (Payload) ──► X25519 Ephemeral Key Exchange ──► Local P2P Sync
                      ▲
                      │ PBKDF2 / Argon2id
                 Master Password
```

* **Disabled by Default:** Complete radio silence out of the box. No listeners or mDNS ports open until the user clicks "Set Up Sync".
* **Granular Category Sync:** Users can individually check/uncheck what synchronizes:
  - `[X]` Bookmarks
  - `[X]` History
  - `[X]` Open Tabs & Sessions
  - `[X]` Containers & Identifies
  - `[X]` Adblock Rule Lists
  - `[ ]` Installed PWAs
* **Client-Side Encryption:** All synced payloads are encrypted on the client machine using authenticated symmetric encryption (**ChaCha20-Poly1305**) derived via **Argon2id** from the user's master secret.
* **Zero-Knowledge Relays:** Optional self-hosted sync relay servers act as blind mailboxes with zero decryption capabilities.
* **Instant Disconnect:** A single click on "Disconnect Device" purges local pairing keys and halts all sync network tasks.

---

## 7. External Password Management Integration

CraftBrowsing does not store master passwords or auto-fill credentials through internal unencrypted mechanisms:
* **Zero Credential Retention:** The browser does not build a proprietary credential store.
* **User Provider Selector:** Choose your preferred external provider (`craft://settings/passwords`):
  - Bitwarden (via desktop app / native messaging)
  - KeePassXC (via keepassxc-proxy)
  - 1Password (via 1Password CLI / agent)
  - Custom Native Messaging Host
  - Disabled / Manual Entry Only
* **Native Messaging Isolation:** Secure IPC connects directly to audited password manager daemons using standard stdin/stdout JSON-RPC protocol.
* **Explicit User Focus Requirement:** Form autofill triggers only upon explicit user selection via a native secure dropdown prompt, preventing hidden CSS iframe credential phishing attacks.
