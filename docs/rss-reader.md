# Local RSS/Atom Feed Reader (`craft://feeds`)

CraftBrowsing includes a fully integrated, privacy-focused RSS, Atom, and JSON feed reader built directly into the browser shell. It eliminates the need for proprietary third-party feed aggregators, cloud tracking, and algorithmic news feeds.

---

## 1. Core Principles

- **100% Offline & Local First:** Feed discovery, scheduling, storage, and parsing happen entirely on your machine in `craft-storage` (SQLite).
- **Zero-Cloud / Zero-Tracking:** No central server knows which feeds you follow or what articles you read.
- **Automatic Feed Discovery:** An address bar feed badge automatically indicates when the current webpage exposes an RSS/Atom link tag.
- **Distraction-Free Reading:** Native integration with Reader View for clean, ad-free typography.
- **End-to-End Encrypted Sync:** Opt-in synchronization of subscriptions, folders, and read statuses across your personal devices via `docs/sync.md`.

---

## 2. Feed Discovery & Subscription Flow

```
[ User visits blog.rust-lang.org ]
               │
               ▼
[ Engine scans HTML <link rel="alternate" type="application/rss+xml"> ]
               │
               ▼
[ Address Bar: Displays 📡 Feed Icon ]
               │ (Click)
               ▼
[ Dropdown Dialog: "Subscribe to 'Rust Blog'?" ]
   ├── Choose Folder / Category
   ├── Set Refresh Interval (Default: 1h, 6h, 24h, Manual)
   └── Assign Multi-Account Container (Optional proxy/Tor routing)
```

---

## 3. Storage & Feed Parsing Pipeline

- **Feed Parser Engine:** Pure-Rust feed parsing leveraging memory-safe XML and JSON deserializers.
- **Database Schema (`craft-storage`):**
  - `feeds`: `id`, `url`, `title`, `site_url`, `folder_id`, `refresh_interval`, `last_fetched`, `etag`, `last_modified`.
  - `feed_items`: `id`, `feed_id`, `guid`, `title`, `url`, `author`, `content_html`, `published_at`, `is_read`, `is_starred`.
- **Bandwidth & Cache Optimization:**
  - Uses standard HTTP `If-None-Match` (ETag) and `If-Modified-Since` headers to minimize network overhead.
  - Background polling occurs on a low-priority thread pool with zero UI stutter.
- **OPML Import / Export:** One-click import and export of OPML XML files to migrate to/from existing feed readers.

---

## 4. User Interface (`craft://feeds`)

- **Three-Pane Layout:**
  1. **Left Sidebar:** Feeds categorized by custom folders, unread counts, and starred filters.
  2. **Middle Column:** Article list sorted chronologically with unread indicators and search filtering.
  3. **Right Pane:** Full article preview formatted with CraftBrowsing's native Reader View typography, image zooming, and font controls.
- **Keyboard Shortcuts:**
  - `J` / `K`: Next / Previous article.
  - `M`: Toggle Read / Unread.
  - `S`: Toggle Star / Bookmark.
  - `O`: Open original article in a new tab.
  - `R`: Refresh all feeds.
