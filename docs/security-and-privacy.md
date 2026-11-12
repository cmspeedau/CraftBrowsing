# Security & Privacy Architecture Specification

In CraftBrowsing, **security and privacy are non-negotiable, mission-critical principles**. The architecture operates under a **Zero-Trust by Default** model: the browser trusts neither arbitrary web content, third-party network listeners, telemetry servers, nor peripheral hardware until explicitly authenticated and authorized by the user.

---

## 1. Zero-Trust Core Architecture

| Vector | Default Stance | Enforcement Mechanism |
| :--- | :--- | :--- |
| **Telemetry / Phone-Home** | **Zero / Eliminated** | Absolutely no background analytics, crash report pings, or usage telemetry exist in the codebase. |
| **P2P Sync** | **Disabled & Dormant** | No listening ports, mDNS broadcasts, or background network calls until manually enabled and paired via E2EE key exchange. |
| **Hardware Devices** | **Disabled & Scoped** | WebSerial, WebUSB, and WebHID APIs remain disabled until explicitly activated in settings and approved via native permission pickers. |
| **Third-Party Trackers** | **Blocked** | In-engine network interception using `adblock-rust` on both DOM requests and subresource network calls. |
| **State Partitioning** | **Isolated** | Strict multi-account container identities isolating cookies, IndexedDB, CacheStorage, service workers, and `localStorage`. |
| **Storage Cleardown** | **Ephemeral Options** | Private Browsing sessions run purely in RAM (`:memory:` SQLite) and securely purge all crypto keys and page buffers on close. |

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

---

## 3. Network Privacy & Anti-Surveillance Pipeline

### 3.1. DNS Privacy & Strict TLS
* **DNS-over-HTTPS (DoH) / DNS-over-TLS (DoT):** Built-in DNS resolver bypassing OS-level ISP DNS sniffing. Ships with privacy-first default options (Quad9, Mullvad) and custom endpoint support.
* **HTTPS-First & HTTPS-Only:** Crafts upgrade requests automatically from `http://` to `https://`. Downgrades present full-page interstitial blocking warnings.
* **Encrypted Client Hello (ECH):** Prevents network snoopers and ISPs from seeing the target Server Name Indication (SNI) during TLS handshakes.

### 3.2. Native Tracking Protection
* **Filter Execution:** Sub-millisecond rule evaluation (`adblock-rust`) matching EasyList, EasyPrivacy, uBlock Origin filter rules, and URL parameter tracking strippers (e.g., stripping `fbclid`, `gclid`, `utm_*`).
* **CNAME Uncloaking:** Resolves canonical DNS names internally prior to rule evaluation to catch trackers masquerading under first-party subdomains.
* **Referrer Trimming:** Cross-origin requests trim referrers strictly to the origin (`https://example.com/` instead of full URL path and search queries).

---

## 4. Advanced Anti-Fingerprinting Defenses

Fingerprinting scripts attempt to build a persistent hardware signature without using cookies. CraftBrowsing implements systematic randomization and blunting:

* **Canvas & WebGL Jitter:** Subtle, imperceptible noise injection into HTML5 2D canvas and WebGL readback operations (`toDataURL`, `getImageData`, `readPixels`). Identical scripts run on the same device return diverging hashes across containers.
* **AudioContext Farbling:** Micro-timing perturbations added to Web Audio API dynamics compressions and frequency domain transforms to break acoustic fingerprinting.
* **Font Enumeration Shielding:** Sites can only query standard system base fonts; queries for user-installed niche fonts return `false` to prevent unique font collection signatures.
* **Uniform User-Agent & Hardware Concurrency:** User-Agent strings, OS platform strings, and `navigator.hardwareConcurrency` report standardized tiers to blend into a broad anonymity crowd.
* **Battery & Sensor API Nullification:** Battery Status API, Ambient Light Sensor, and Gyroscope APIs return mock or disabled responses to prevent battery level/hardware physical profiling.

---

## 5. Ephemeral Private Browsing Architecture

When a Private Browsing window is spawned:
1. **In-Memory SQLite:** All session data (history, local storage, cookies, IndexedDB, cached network responses) is backed by an in-memory SQLite database (`file::memory:?cache=shared`).
2. **Zero Disk Touches:** No cache files, swap partitions, temporary files, or crash logs are written to non-volatile storage.
3. **Cryptographic Key Zeroization:** All in-memory cryptographic keys and decrypted session caches are wrapped in `zeroize::ZeroizeOnDrop` to scrub memory buffers when private tabs/windows are closed.

---

## 6. Zero-Knowledge Local Sync Cryptography

When synchronization is explicitly turned on by the user (`docs/sync.md`):

```text
Local Store ──► ChaCha20-Poly1305 (Payload) ──► X25519 Ephemeral Key Exchange ──► Local P2P Sync
                      ▲
                      │ PBKDF2 / Argon2id
                 Master Password
```

* **Client-Side Encryption:** All synced payloads (bookmarks, history, container mappings, adblock lists, PWAs) are encrypted on the client machine using authenticated symmetric encryption (**ChaCha20-Poly1305**) derived via **Argon2id** from the user's master secret.
* **Zero-Knowledge Relays:** If an optional self-hosted sync relay is used, the relay server acts as a blind mailbox. The server cannot inspect or parse any synced payload, tab URL, or configuration entry.
* **Mutual Authentication (Local mDNS):** Devices on the local network pair via QR code or out-of-band numeric PIN exchange; unauthenticated devices attempting to connect are dropped immediately.

---

## 7. External Password Management Integration

CraftBrowsing does not store master passwords or auto-fill credentials through internal unencrypted mechanisms:
* **Zero Credential Retention:** The browser does not build a proprietary credential store.
* **Native Messaging Isolation:** Secure IPC connects directly to audited password manager daemons (Bitwarden, KeePassXC, 1Password) using standard stdin/stdout JSON-RPC protocol.
* **Explicit User Focus Requirement:** Form autofill triggers only upon explicit user selection via a native secure dropdown prompt, preventing hidden CSS iframe credential phishing attacks.
