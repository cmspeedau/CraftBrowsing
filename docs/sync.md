# Local Self-Hosted Sync

CraftBrowsing offers peer-to-peer, end-to-end encrypted synchronization without reliance on centralized third-party servers.

---

## Privacy First: Disabled by Default

In keeping with a strict privacy-by-default posture:
- **Default State: Disabled.** CraftBrowsing will never initiate network discovery (mDNS), open background sockets, or broadcast device availability until explicitly enabled by the user in settings (`craft://settings/sync`).
- **Explicit User Opt-In:** Enabling sync requires generating or pairing via an encryption passphrase / device recovery key.
- **Granular Controls:** When enabled, users can selectively toggle what syncs (Bookmarks, History, Containers, Block Lists, Installed PWAs, Open Tabs & Tab Groups).

---

## Features

- **End-to-End Encryption:** Profile databases (bookmarks, history, open tabs, container definitions, adblock filter lists, installed progressive web apps) are encrypted locally using ChaCha20-Poly1305 before transmission.
- **Open Tabs & Synced Sessions:**
  - **Multi-Device Tab Tray (`craft://tabs/synced`):** View all active tabs organized by remote device name and timestamp.
  - **Container & Group Context Retention:** Synced tabs maintain their assigned Multi-Account Container identity (e.g. "Work", "Personal") and Tab Group hierarchy so opening them on another device preserves container isolation.
  - **Send Tab to Device:** Instantly push a single tab or active tab group to another paired device with optional auto-focus notification.
  - **Session History Depth:** Synced tabs serialize up to 10 entries of back/forward navigation history per tab for seamless reading handoff.
  - **Zero Session Cookie Leakage:** Tab state synchronizes URLs, titles, container context, and scroll position; authentication cookies and storage stay isolated on their originating device.
- **Installed PWA Synchronization:**
  - **Manifest & App Definitions:** App ID, start URL, scope, display mode, theme colors, and custom icon hashes.
  - **Container Binding:** Retains the assigned container identity (e.g. syncing that "Slack" runs inside the "Work" container).
  - **Local Native Registration:** When a new PWA payload is received on a paired client, the client fetches necessary assets locally and registers desktop shortcuts (Linux `.desktop`, Windows Start Menu, or macOS `.app` bundle) seamlessly.
- **Block List & Filter Rule Synchronization:**
  - **Subscribed Filter Lists:** URLs, update schedules, and toggle states (e.g., EasyList, EasyPrivacy, uBlock filters) sync across paired devices.
  - **Custom User Rules:** User-defined block/allow rules (`||example.com^`, cosmetic filters) sync seamlessly.
  - **Per-Site Allowlist/Exceptions:** Sites where adblocking has been disabled by the user sync consistently so exceptions apply across all of the user's machines.
  - **Bandwidth Optimization:** Filter lists themselves are not re-transmitted over the wire if already publicly fetchable; devices sync list metadata/URLs and download locally, while custom rules sync as lightweight delta records.
- **Container Synchronization:** Container setups (names, icons, colors, domain routing rules) and container-tagged open tabs sync seamlessly across devices. (Session cookies and credentials remain local only).
- **Local Network Discovery (Opt-In):** When sync is toggled on, uses mDNS (`mdns-sd`) to discover other CraftBrowsing instances on the local Wi-Fi/LAN automatically.
- **Self-Hosted Relay Option:** For syncing across different networks, users can run a lightweight Docker container relaying encrypted blobs over WebSockets.
- **Conflict Resolution:** CRDTs (Conflict-free Replicated Data Types) or Last-Write-Wins (LWW) timestamps manage concurrent updates for bookmarks, history, containers, block lists, open tabs, and PWAs.

---

## Sync Configuration & Tab Data Model

```rust
pub struct SyncPreferences {
    /// Master toggle (defaults to false)
    pub sync_enabled: bool,
    /// Granular item toggles
    pub sync_bookmarks: bool,
    pub sync_history: bool,
    pub sync_open_tabs: bool,
    pub sync_containers: bool,
    pub sync_block_lists: bool, // Subscriptions, custom filters, and site exceptions
    pub sync_pwa_apps: bool,    // Installed PWAs, manifests, and container bindings
}

/// Representation of an active browser window / tab session on a synced device
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceTabSessionPayload {
    pub device_id: String,
    pub device_name: String,
    pub client_version: String,
    pub updated_at: u64,
    pub tab_groups: Vec<SyncedTabGroup>,
    pub tabs: Vec<SyncedTabEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncedTabEntry {
    pub tab_id: String,
    pub title: String,
    pub url: String,
    pub favicon_url: Option<String>,
    pub container_id: Option<String>,
    pub group_id: Option<String>,
    pub scroll_position: (u32, u32),
    pub back_forward_history: Vec<String>,
    pub is_pinned: bool,
    pub last_accessed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncedTabGroup {
    pub group_id: String,
    pub name: String,
    pub color: String,
    pub container_binding: Option<String>,
    pub is_collapsed: bool,
}

pub struct BlockListSyncPayload {
    /// Active filter list subscription URLs & metadata
    pub subscribed_lists: Vec<FilterSubscriptionMetadata>,
    /// Custom adblock filter rules authored by the user
    pub custom_rules: Vec<String>,
    /// Domains added to the user's global or container-specific allowlist
    pub site_exceptions: Vec<String>,
}

pub struct PwaAppSyncPayload {
    /// Unique identifier computed from origin and manifest start_url
    pub app_id: String,
    /// Web App Manifest URL and canonical start URL
    pub manifest_url: String,
    pub start_url: String,
    pub name: String,
    pub short_name: Option<String>,
    pub scope: String,
    pub display_mode: String,
    pub theme_color: Option<String>,
    /// Bound container identity (e.g. "Work", "Personal")
    pub container_id: Option<String>,
    /// Version/timestamp for Last-Write-Wins or CRDT conflict resolution
    pub updated_at: u64,
}
```

---

## Remote Actions: Send Tab to Device

CraftBrowsing supports encrypted point-to-point command payloads:

1. **Context Menu / Omnibox Trigger:** User right-clicks any tab or clicks the device icon in the Omnibox and selects **"Send Tab to [Device Name]"**.
2. **Encrypted Action Packet:** A transient action message containing target device ID, URL, title, container identifier, and optional tab group metadata is encrypted via X25519 / ChaCha20-Poly1305.
3. **Receipt on Target Device:**
   - If target device is running, it prompts via an in-app banner or native notification: *"Received tab from [Device]: Open in [Container] container?"*.
   - Can automatically open in the matched container while strictly preventing cross-container cookie leakage.
