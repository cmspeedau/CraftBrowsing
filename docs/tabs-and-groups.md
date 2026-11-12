# Tab Management, Grouping & Vertical Tabs Specification

CraftBrowsing provides a keyboard-first, high-density tab management architecture designed for power users, developers, and privacy enthusiasts. It supports both traditional horizontal tab strips and collapsible Vertical Tabs (Tree-Style Tabs), deep Multi-Account Container bindings, and intelligent memory hibernation.

---

## 1. Architectural Highlights

- **Dual Layout Engines:** Switch seamlessly between horizontal tabs and a collapsible vertical sidebar (`Ctrl+Shift+V` / `Cmd+Shift+V`).
- **Tree-Style Hierarchy:** Child tabs opened from a parent tab indent into collapsible sub-trees, making multi-topic workflows structured.
- **Container-Bound Tab Groups:** Tab groups can be bound to specific Multi-Account Containers, auto-assigning contextual cookie/storage isolation to every tab inside the group.
- **Tab Hibernation & Memory Optimization:** Inactive tabs automatically serialize their state to memory-mapped SQLite cache and drop their WebRender surface, reducing RAM consumption by up to 85%.

---

## 2. Vertical Tabs & Tree-Style Navigation

Vertical tabs utilize the full height of widescreen displays and prevent tab title truncation:

```
+--------------------------------------------------------------------------+
| [=] CraftBrowsing   [ < ] [ > ] [ R ]   [ https://servo.org      | 🛡️ ]  |
+-------------------+------------------------------------------------------+
| [x] New Tab       |                                                      |
|                   |                                                      |
| v DevOps [Work]   |                 Rendered Web Page                    |
|   |-- AWS Console |                                                      |
|   `-- Grafana     |                                                      |
|                   |                                                      |
| v Research [Pers] |                                                      |
|   |-- Rust Docs   |                                                      |
|   `-- Crates.io   |                                                      |
|                   |                                                      |
| [⚙️ Settings]     |                                                      |
+-------------------+------------------------------------------------------+
```

### Display States:
1. **Expanded Mode (Default ~240px):** Shows tab favicons, full page titles, container color pills, audio playback indicators, and close buttons.
2. **Collapsed Icon-Only Mode (~48px):** Shows only favicons with hover popover tooltips for titles.
3. **Auto-Hide Flyout:** Stays collapsed until the cursor hovers near the left bezel.

---

## 3. Tab Grouping & Container Integration

Tab groups provide structured organization and context management:

- **Color & Tag Customization:** Users can label groups (e.g., `Finance`, `DevOps`, `Shopping`) and pick distinct visual accent colors.
- **Automatic Container Assignment:** Setting a container default for a group ensures all links opened within that group inherit the group's `UserContextId` (cookies, cache, credentials).
- **Group Collapse & Snooze:** Collapsing a tab group can optionally hibernate all tabs within it immediately to release CPU and memory resources.
- **Group Export / Save:** Save entire groups into named session bundles that can be restored with a single click or synced across devices.

---

## 4. Intelligent Tab Hibernation & Memory Discarding

To prevent memory leaks and runaway background tab bloat:

```
[ Active Tab ]  -- 20m Idle --> [ Serialized State (DOM/Scroll) ] --> [ WebRender Dropped ]
                                              ^
                                     User clicks tab
                                              |
[ WebRender Re-attached ] <----- [ Instant Session Restore ]
```

1. **Idle Detection:** Tabs idle for a configurable period (default: 30 minutes, customizable under `craft://settings/tabs`) are marked for hibernation.
2. **State Serialization:** Form inputs, scroll coordinates, and navigation history are stored in `craft-storage`.
3. **Surface Teardown:** The underlying Servo WebRender surface and JavaScript execution context are paused and deallocated.
4. **Instant Awakening:** When clicked, the tab loads its cached state instantly without triggering a full network reload if the page cache is valid.
5. **Exceptions:** Tabs with active audio/video playback, active WebSerial/WebUSB sessions, active downloads, or pinned status are immune to automatic discarding.
