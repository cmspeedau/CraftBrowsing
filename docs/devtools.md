# Web Developer Tools (DevTools) Specification

CraftBrowsing provides a built-in suite of developer tools integrated directly with the Servo DOM and networking pipeline.

---

## 1. Key Panels & Capabilities

### Inspector (Elements & Styles)
- **Live DOM Tree:** Bidirectional mapping between Servo's DOM node hierarchy and the DevTools tree view.
- **Computed Styles & CSS Box Model:** Live inspection and editing of CSS rules, margins, padding, borders, and flex/grid overlays.
- **Element Picker:** Click-to-inspect tool in the viewport to instantly jump to the corresponding DOM node.

### Console
- **JavaScript REPL:** Evaluates code within the active page or container context.
- **Logging & Stack Traces:** Captures `console.log`, `warn`, `error`, `info`, and uncaught runtime errors.
- **Container Awareness:** Clearly marks the active `UserContextId` when executing expressions in isolated container tabs.

### Network Monitor
- **Waterfall Timeline:** Real-time logging of HTTP/HTTPS requests, WebSocket streams, and fetch/XHR calls.
- **Detailed Inspection:** Headers, query parameters, payload bodies, response cookies, and timing breakdown (DNS, TLS, TTFB, download).
- **Adblock & Filter Indicators:** Badges requests blocked or redirected by `craft-adblock`.

### Storage & State Viewer
- **Per-Container Partitioning:** View and inspect cookies, `localStorage`, `sessionStorage`, and IndexedDB entries separated by tab container ID.
- **Cache Management:** Clear or modify stored data per origin or container.

---

## 2. DevTools Architecture & CDP Support

CraftBrowsing implements a Chrome DevTools Protocol (CDP) / Firefox Remote Debugging compatible bridge via `craft-devtools`:

1. **Internal IPC:** Communicates with `craft-engine` over lightweight message channels (`crossbeam-channel` or `tokio::sync::mpsc`).
2. **Embedded & Detachable UI:** Can dock to bottom/side of browser tab or open as a standalone window.
3. **Remote Debugging Server (Optional):** Ability to open an inspection WebSocket port (e.g., `localhost:9222`) to attach external tooling or automation suites (Playwright, Puppeteer).
