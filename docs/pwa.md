# Progressive Web Apps (PWA) Specification

CraftBrowsing provides native, first-class support for Progressive Web Apps (PWAs), allowing websites to be installed as standalone desktop applications while preserving CraftBrowsing's privacy guarantees, container isolation, and adblocking engine.

---

## 1. Core Principles

1. **Standalone Window Experience:** Installed PWAs launch in dedicated, chromeless application windows (omitting tabs and navigation bars, while retaining essential security badges and window controls).
2. **Container-Aware Installation:** PWAs can be tied to specific Container identities (e.g., install Slack inside the "Work" container to keep session cookies isolated from personal browsing).
3. **Privacy & Native Blocking Intact:** All installed PWAs run through the `craft-adblock` network pipeline, preventing tracker beacons and telemetry even when launched outside the standard browser shell.
4. **Offline Capability & Service Workers:** Standard Service Worker lifecycle caching and offline asset management supported natively via Servo's cache storage.
5. **Desktop & OS Integration:** Native desktop launchers (`.desktop` entries on Linux, `.app` bundles on macOS, and Start Menu shortcuts on Windows).

---

## 2. Architecture: `craft-pwa` Crate

PWA capabilities are managed through a dedicated workspace crate:

```text
crates/craft-pwa/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate entry point and registration API
    ├── manifest.rs       # Web App Manifest parser (W3C standard JSON)
    ├── install.rs        # Installation pipeline & shortcut generator
    ├── window.rs         # Standalone app window styling & lifecycle
    └── registry.rs       # SQLite-backed PWA database & metadata manager
```

### Manifest Parsing (`manifest.rs`)
CraftBrowsing detects and parses standard W3C Web App Manifest files (`<link rel="manifest" href="...">`):
- `name` and `short_name`: Display name on title bars and system app menus.
- `start_url` and `scope`: Navigation boundary constraints. Navigating outside `scope` opens in a normal CraftBrowsing tab.
- `display`: Supports `standalone`, `minimal-ui`, and `fullscreen`. Defaults to `standalone`.
- `icons`: High-resolution PNG/SVG icons parsed and extracted for native OS dock/taskbar icons.
- `theme_color` & `background_color`: Applied to window titlebars and splash/loading states.

---

## 3. Installation Flow & User Experience

```text
[Web Page Loaded]
       │
       ▼
[Detect Manifest & Service Worker]
       │
       ▼
[Install Icon in Address Bar (PWA Badge)]
       │
       ▼ (User Clicks "Install App")
[Install Dialog Prompt]
  ├── Name & URL Scope Preview
  ├── Container Identity Selector (Default: Current Tab Container)
  └── Create Desktop & Start Menu Shortcuts (Checked by default)
       │
       ▼ (User Confirms)
[Save to `craft-pwa` Registry & Register OS Shortcuts]
       │
       ▼
[Launch in Standalone PWA Window]
```

---

## 4. Native OS Integration

When a PWA is installed, CraftBrowsing registers OS-level desktop entries:

- **Linux (XDG):** Creates a `.desktop` file under `~/.local/share/applications/craft-pwa-<app-id>.desktop` with appropriate `Name`, `Exec=craft-app --app-id=<app-id>`, and `Icon` paths.
- **macOS:** Generates an application bundle wrapper under `~/Applications/CraftBrowsing Apps/`.
- **Windows:** Creates application shortcuts in `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Craft Apps`.

Each launcher invokes CraftBrowsing with the `--app-id` flag, instructing `craft-app` to render a single, chromeless window routed directly to the app's `start_url` and bound to its assigned container profile.

---

## 5. Security & Isolation Model

- **Scope Boundary Enforcement:** Any link pointing outside the manifest `scope` will not navigate within the PWA window. It opens in a new tab within the main CraftBrowsing browser window, keeping apps contained.
- **Container Isolation:** If an app is installed inside Container "Work", its cookies, local storage, indexedDB, and cache partitions never bleed into the default or "Personal" container.
- **Opt-In Syncing of Installed Apps:** When Local Sync is enabled (`sync.md`), installed PWA metadata (manifest URL, container affinity, custom window dimensions) can sync across user devices.
