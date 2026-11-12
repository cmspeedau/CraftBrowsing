# Contextual Identity Containers

CraftBrowsing features native, built-in container tabs—similar to Firefox Multi-Account Containers—allowing users to isolate web storage, cookies, caches, and sessions while keeping container definitions, rules, and container-assigned tabs synchronized across devices.

---

## Core Isolation Guarantees

Each container acts as an isolated browsing context bound to a unique `contextual_identity_id`:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        CraftBrowsing Process                          │
│                                                                        │
│  ┌─────────────────────────┐            ┌───────────────────────────┐  │
│  │ Container: Personal     │            │ Container: Work           │  │
│  │ ID: container-personal  │            │ ID: container-work        │  │
│  │ Color: Blue | Icon: User│            │ Color: Orange | Icon: Brief│  │
│  ├─────────────────────────┤            ├───────────────────────────┤  │
│  │ - Isolated Cookie Jar   │            │ - Isolated Cookie Jar     │  │
│  │ - IndexedDB & LocalStor │            │ - IndexedDB & LocalStor   │  │
│  │ - Service Workers Cache │            │ - Service Workers Cache   │  │
│  │ - HTTP Cache Partition  │            │ - HTTP Cache Partition    │  │
│  └─────────────────────────┘            └───────────────────────────┘  │
│                                                                        │
│                      Shared Between Containers                         │
│  - Bookmarks & History (tagged by container ID)                        │
│  - Browser Preferences & Extension Configs                            │
│  - Native UI & Window Management                                       │
└────────────────────────────────────────────────────────────────────────┘
```

### Data Boundaries
1. **Network & Cookie Store:** Every network request passes through Servo's resource task tagged with the active tab's `contextual_identity_id`.
2. **Web Storage & IndexedDB:** Storage directories are partitioned on disk:
   `~/.local/share/craftbrowsing/storage/containers/{container_id}/`
3. **HTTP Cache:** Partitioned by both Top-Level Origin and Container ID to prevent cross-container cache-timing attacks.
4. **Certificate & TLS Session Cache:** Optional session resumption isolation to prevent TLS fingerprint cross-talk.

---

## Container Schema & Sync Data Model

Container definitions and site-association rules are first-class sync primitives.

### SQLite Schema (`craft-storage`)

```sql
-- Container definitions
CREATE TABLE containers (
    id TEXT PRIMARY KEY,               -- e.g. "default", "personal", "work", "banking"
    name TEXT NOT NULL,
    color TEXT NOT NULL,              -- Hex color code (#3b82f6)
    icon TEXT NOT NULL,               -- Icon identifier (e.g. "briefcase", "cart", "fingerprint")
    user_context_id INTEGER NOT NULL UNIQUE,
    is_ephemeral BOOLEAN DEFAULT 0,   -- Delete cookies/storage on tab close
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER                -- Tombstone for sync deletion
);

-- Site routing rules (auto-open domain in designated container)
CREATE TABLE container_site_rules (
    id TEXT PRIMARY KEY,
    container_id TEXT NOT NULL REFERENCES containers(id) ON DELETE CASCADE,
    domain_pattern TEXT NOT NULL,     -- e.g. "*.github.com", "mycompany.okta.com"
    exact_match BOOLEAN DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

-- Active open tabs state for multi-device sync
CREATE TABLE synced_tabs (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    container_id TEXT REFERENCES containers(id),
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    position INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

---

## Synchronizing Containers Across Clients

Container sync is integrated into `craft-sync` and leverages the existing E2EE pipeline (ChaCha20-Poly1305 with local mDNS / self-hosted relay).

### Synchronized Payload Types
1. **Container Manifest:** Container name, icon, color, and preferences.
2. **Container Routing Rules:** Automatic domain-to-container assignments.
3. **Open Tabs (by Container):** Remote tab groups retain their container assignments when viewed from another device.

### Non-Synchronized (Local Only)
- **Active Session Cookies and Storage:** By default, credentials and storage data remain strictly on the local device for security and compliance.
- **Ephemeral Container Storage:** Destroyed automatically when the last associated tab closes.

---

## Integration with Extension System (WebExtensions API)

CraftBrowsing exposes the standard `contextualIdentities` JavaScript API:

```javascript
// Example extension creating a banking container
browser.contextualIdentities.create({
    name: "Banking",
    color: "green",
    icon: "dollar"
}).then((container) => {
    // Open a new tab locked into this container
    browser.tabs.create({
        url: "https://mybank.com",
        cookieStoreId: container.cookieStoreId
    });
});
```

Extensions can also query and react to container assignments using `browser.contextualIdentities.query()`, `onCreated`, and `onRemoved` listeners.
