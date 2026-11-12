# Plugin & Extension System Specification

CraftBrowsing implements a hybrid extension model combining **Modern WebExtensions** with **Classic Firefox / userChrome customization**.

---

## 1. Extension Manifest (`manifest.json`)

Extensions are distributed as directory packages or `.zip` files containing a standard manifest:

```json
{
  "manifest_version": 2,
  "name": "Tree Style Tabs & Notes",
  "version": "1.0.0",
  "description": "Vertical tree-style tabs and sidebar scratchpad.",
  "permissions": [
    "tabs",
    "bookmarks",
    "storage",
    "ui"
  ],
  "theme": {
    "user_chrome_css": "chrome.css"
  },
  "browser_action": {
    "default_icon": "icons/icon-32.png",
    "default_title": "Notes",
    "default_popup": "popup.html"
  },
  "sidebar_action": {
    "default_title": "Sidebar",
    "default_panel": "sidebar.html"
  },
  "background": {
    "scripts": ["background.js"]
  }
}
```

---

## 2. Sandboxed Runtime Architecture

- **Host (Rust):** The `craft-extensions` crate creates isolated JS execution contexts per extension.
- **IPC Bridge:** A bidirectional message broker (JSON-RPC) routes requests between the JS engine and Rust browser state.
- **Permission Checking:** APIs (`browser.tabs.*`, `browser.bookmarks.*`) verify declared permissions before executing privileged actions.

---

## 3. UI Customization (`userChrome.css`)

Similar to Firefox, power users can drop custom styling files into their profile folder:

```text
~/.config/craftbrowsing/
└── profiles/
    └── default/
        ├── chrome/
        │   ├── userChrome.css       # Modifies the browser chrome (tabs, URL bar)
        │   └── userContent.css      # Injected into web pages
        └── extensions/              # Installed extensions
```
