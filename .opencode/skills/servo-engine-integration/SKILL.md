---
name: servo-engine-integration
description: Embed and debug the Servo engine in CraftBrowsing: Servo and WebView lifecycle, rendering contexts, winit event-loop ownership, paint and present, WebView delegates, adblocking through resource interception, multiprocess content processes, and the container and private-browsing gaps in servo 0.8.0. Use this whenever work touches craft-engine, a servo crate API, a page that never paints, input that never reaches the page, navigation or permission callbacks, tab creation or closing, engine process setup, or anything crossing the UI and engine boundary, even when the user only says "hook up the engine", "the page is blank", or "wire the browser to Servo".
---

# Servo Engine Integration

CraftBrowsing is a real browser. Treat the engine as the product, not as a demo behind a fake event stream. This skill is the checklist for embedding `servo` and for changing anything that crosses the UI and engine boundary.

Verified against the `servo` crate 0.8.0, released 2026-11-07. The crate source is the authority: docs.rs failed to build 0.8.0, so download it from crates.io when a signature matters:

```sh
curl -sSL https://static.crates.io/crates/servo/servo-0.8.0.crate -o /tmp/opencode/servo-0.8.0.crate
tar xzf /tmp/opencode/servo-0.8.0.crate -C /tmp/opencode
```

Servo's own references are `examples/winit_minimal.rs` in that crate and servoshell at `ports/servoshell` in the Servo repository. Read the example before inventing a lifecycle, and read servoshell before inventing a browser feature.

## Start here

1. Read the current engine code before changing it: `crates/craft-engine/src/lib.rs`, `compositor.rs`, `events.rs`, then `crates/craft-ui/src/lib.rs` and `window.rs`.
2. Confirm what is real. At the time this skill was written, `craft-engine` had no `servo` dependency, no `Cargo.lock` entry for it, and `ServoEngine` faked navigation events on a worker thread. Do not grow that simulation. Replace it.
3. Confirm the API against 0.8.0 source. Do not copy snippets from old blog posts or from docs for 0.4.0; the embedding API moved substantially between releases, including the `WebViewBuilder` shape and the delegate traits.
4. Load `rust` before editing Rust, and `rust-testing` if you add tests.

## Hard constraints

These decide the architecture. If a proposed design violates one, stop and redesign instead of working around it.

- One `Servo` per process. `Servo::new` calls `opts::initialize_options(opts)`, which does `OPTIONS.set(opts).expect("Already initialized")`. A second `Servo` panics. Options and most preferences are process-global.
- `Servo` and `WebView` are not `Send`. They must live on the thread that runs the winit event loop and owns the GL context. A channel that ships commands to a separate "engine thread" cannot own them.
- A `WebView` is always exactly the size of its `RenderingContext`. There is no rectangle setter. `WebView::resize` resizes the context and therefore every other `WebView` sharing it.
- `Servo::spin_event_loop` is the pump. Call it on the UI thread after input, after commands, and whenever the `EventLoopWaker` fires. Delegate callbacks only run inside it.
- Paint is two steps in order: make the context current, `webview.paint()`, then `rendering_context.present()`. Request the redraw from `notify_new_frame_ready`; do not paint on a timer.
- Multiprocess content processes are re-execs of the same binary. Detect `--content-process <token>` before creating any window and call `run_content_process(token)`.
- There is no per-webview container or user-context ID in 0.8.0, and no per-webview private-browsing flag. Read `references/containers-and-processes.md` before promising isolation.

## Workflow

### 1. Wire the dependency

Add versions to the root `[workspace.dependencies]`, then reference them with `{ workspace = true }`:

```toml
servo = "0.8.0"
euclid = "0.22"
rustls = { version = "0.23", default-features = false, features = ["aws_lc_rs", "logging", "std", "tls12"] }
```

`craft-engine` needs `servo`, `euclid` (for `Scale<f32, DeviceIndependentPixel, DevicePixel>`), `url`, and `winit`. Servo requires Rust 1.88 or newer for this release; the workspace toolchain is newer, so keep the toolchain file as is.

The `servo` default features include `multiprocess`, `devtools`, `webgl`, `webgpu`, `webxr`, `bundled`, and `js_jit`. Start with defaults, then trim deliberately per platform. Do not disable `multiprocess` and quietly claim process isolation.

### 2. Bootstrap the process

Order matters:

1. Install a rustls crypto provider before anything touches TLS: `rustls::crypto::aws_lc_rs::default_provider().install_default()`. Missing it produces a runtime panic deep in the network stack.
2. Scan arguments for `--content-process`. If present, call `servo::run_content_process(token)` and exit the function. This must happen before the window and the `Servo` are created.
3. Build the event loop with a user-event type, because the waker needs a proxy.
4. Decide `Opts` once for the process: `config_dir`, `multiprocess`, `sandbox`, `temporary_storage`, `certificate_path`, `ignore_certificate_errors`, `hard_fail`.

`Opts::default()` is not browser-ready: `multiprocess` is false, `sandbox` is false, and `hard_fail` is true (a thread failure exits instead of showing `about:failure`). Set each one explicitly and write down the choice.

### 3. Create the window and rendering contexts

In `ApplicationHandler::resumed`, create the winit window, then:

```rust
let context = Rc::new(WindowRenderingContext::new(
    event_loop.display_handle()?,
    window.window_handle()?,
    window.inner_size(),
)?);
context.make_current()?;
```

One `WindowRenderingContext` per window. For each tab that should be individually composited, create `Rc::new(context.offscreen_context(size))` and give that to the tab's `WebView`. For a browser that only shows one tab at a time, a single per-window offscreen context reused by the active webview is enough and cheaper. Decide which model the chrome actually needs before creating contexts.

### 4. Build Servo and the waker

Implement `EventLoopWaker` as a cloneable proxy that sends a user event, then:

```rust
let servo = ServoBuilder::default()
    .opts(opts)
    .preferences(preferences)
    .event_loop_waker(Box::new(waker))
    .build();
servo.setup_logging();
```

`setup_logging` installs a global `log` logger. Call it once. If CraftBrowsing needs `tracing` span output as well, bridge the two rather than installing a second logger.

### 5. Create webviews

```rust
let webview = WebViewBuilder::new(&servo, rendering_context)
    .url(url)
    .hidpi_scale_factor(Scale::new(window.scale_factor() as f32))
    .delegate(delegate)
    .build();
```

Keep a `HashMap<TabId, WebView>` and a reverse `HashMap<WebViewId, TabId>`. Servo does not expose a lookup from `Servo`, and a `WebView` does not carry the application's tab identity. Dropping the last strong reference to a `WebView` hides and destroys it, so "close tab" is a map removal plus `hide()` if a close animation needs a frame.

`WebViewBuilder` options: `delegate`, `url`, `hidpi_scale_factor`, `user_content_manager`, `clipboard_delegate`, `gamepad_delegate`, `geolocation_delegate`. There is no user-context, private, or container option.

### 6. Own the event loop

- Set `ControlFlow::Wait`. The waker is what wakes the loop. The current shell uses `ControlFlow::Poll`, which burns a core while idle.
- In `user_event` and at the top of `window_event`, call `servo.spin_event_loop()`. This is also how pending delegate requests get delivered.
- In `notify_new_frame_ready`, call `window.request_redraw()` and nothing else.
- In `WindowEvent::RedrawRequested`, make the webview's context current, `webview.paint()`, then `present()`.
- In `WindowEvent::Resized`, call `window_rendering_context.resize(new_size)` and `webview.resize(new_size)`, then request a redraw. Resize before painting; painting into a stale buffer produces a stretched frame.
- In `WindowEvent::ScaleFactorChanged`, call `webview.set_hidpi_scale_factor(...)`, resize, and redraw.
- While any visible webview reports `animating() == true`, spin at display cadence until `notify_animating_changed(false)`. Do not keep a global poll loop for this.

### 7. Forward input

Translate winit events into `servo::InputEvent` and call `webview.notify_input_event(event)` on the focused webview. Coordinates are device pixels relative to the webview: `WebViewPoint::Device(DevicePoint)` or, when using CSS pixels, `WebViewPoint::Page`.

- Cursor movement: `InputEvent::MouseMove(MouseMoveEvent::new(point))`.
- Buttons: `InputEvent::MouseButton(MouseButtonEvent::new(action, button, point))`.
- Wheel and trackpad: `InputEvent::Wheel(WheelEvent::new(delta, point))` with `WheelMode::DeltaLine` or `DeltaPixel`.
- Keyboard, IME, touch, and editing actions: `Keyboard`, `Ime`, `Touch`, `EditingAction`.
- Pointer leaving: `MouseLeftViewport`.

Call `webview.set_focused(false)` when the chrome takes focus, and `true` when the content does. Also call `notify_theme_change(Theme)` when the system theme changes. `notify_input_event` returns an `InputEventId`; `WebViewDelegate::notify_input_event_handled` is how unhandled events bubble back to chrome shortcuts. Read `references/input-and-focus.md` before wiring shortcuts.

### 8. Implement the delegates

A browser needs more than paint:

- `notify_url_changed`, `notify_page_title_changed`, `notify_load_status_changed`, `notify_history_changed`, `notify_favicon_changed`, `notify_status_text_changed`, `notify_cursor_changed`, `notify_animating_changed` drive chrome state.
- `request_navigation` must call `allow()` or `deny()`. Dropping it allows by default, which is the wrong default for a privacy browser.
- `request_permission` must call `allow()` or `deny()`. Wire it to the permission store and prompt manager, and deny unknown features by default.
- `request_create_new` is `window.open` and `target=_blank`. Build the new webview with `request.builder(context)`. Dropping the request denies the popup.
- `request_unload`, `request_authentication`, `request_protocol_handler`, `request_resize_to`, `request_move_to` are user-visible behaviors. Implement or explicitly reject them; do not leave the defaults if the default is unsafe.
- `show_embedder_control` covers `<select>`, `<input type=file>`, `<input type=color>`, IME, `alert`/`confirm`/`prompt`, and context menus. Pages break visibly without these.
- `notify_crashed` and `notify_closed` must update tab state and never leave a dead `WebView` in the map.
- `notify_fullscreen_state_changed` drives window fullscreen, not page zoom.

`ServoDelegate` covers engine-wide events: `notify_error`, DevTools start and connection requests, and resource loads that are not attached to a webview (service workers).

### 9. Route adblocking through resource interception

`NetworkManager` in 0.8.0 only exposes cache inspection, cache clearing, and online state. It cannot block requests.

Every HTTP and HTTPS request passes through `RequestInterceptor` in `servo-net`, which asks the embedder's `load_web_resource` before the request proceeds. In `ServoDelegate` and in `WebViewDelegate`, decide per request:

```rust
fn load_web_resource(&self, _webview: WebView, load: WebResourceLoad) {
    if self.adblock.should_block(load.request()) {
        load.intercept(WebResourceResponse::new(load.request().url.clone())).cancel();
    }
    // Dropping the load means DoNotIntercept.
}
```

Rules:

- Run the filter check synchronously and fast. The request is waiting on the embedder thread for the response. No disk, no network, no locks held across work in this callback.
- `WebViewDelegate` sees webview loads; `ServoDelegate` sees loads with no webview, including service workers. Both need the filter.
- Blocking is `intercept(...).cancel()`. `WebResourceLoad` itself has no `cancel()`.
- Synthetic responses are possible (`intercept`, `send_body_data`, `finish`) and belong to reader mode, internal pages, and tests, not to everyday browsing.
- Keep filter-list matching in `craft-adblock` and keep the engine crate free of rule parsing.

### 10. Shut down in order

Drop webviews, drop the `Servo`, then exit the event loop. Do not call `spin_event_loop` after the `Servo` is dropped. Do not persist engine state that the browser's SQLite layer already owns.

## CraftBrowsing mapping

The engine crate currently models a command and event stream that does not match Servo's ownership rules. Read `references/craftbrowsing-mapping.md` before editing it. The short version:

- `EngineCommand` variants become direct `WebView` method calls on the UI thread, or queued user events that the UI thread executes. They do not become messages to a worker thread.
- `EngineEvent` notifications come from `WebViewDelegate` callbacks during `spin_event_loop`.
- `CompositorSurface` is replaced by `WindowRenderingContext` plus per-tab `OffscreenRenderingContext`s.
- `EngineContext` survives as the chrome-side cache of URL, title, load state, and history flags. It must not duplicate widget ownership.
- `Container.user_context_id` currently has no consumer in the engine. It cannot partition Servo cookies or storage in 0.8.0.

## Verification

A change is not done until you have seen it work:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p craft-engine
cargo test -p craft-engine
RUST_LOG=info cargo run -p craft-ui
```

Check by looking, not by trusting an event log: the page paints, scrolling moves it, keyboard input reaches a text field, a link navigates, back and forward update the chrome, and closing the tab releases the screenshot or paint resources. For engine-only work, a headless test can drive `SoftwareRenderingContext` and assert on `take_screenshot` or `evaluate_javascript` results.

Add tests for navigation state mapping, permission decisions, adblock decisions, and tab creation and teardown. Mock the engine boundary where the real engine would need a GPU.

## References

Read only what the task needs:

- `references/embedding-api.md`: the 0.8.0 API surface, builder options, webview methods, delegate methods, input types, options and preferences.
- `references/rendering.md`: contexts, surfman, paint and present, compositing tabs, HiDPI, resize, redraw, headless rendering.
- `references/input-and-focus.md`: winit to Servo input translation, keyboard vs character events, IME, focus, shortcuts, and accessibility.
- `references/containers-and-processes.md`: multiprocess, sandboxing, the missing container and private-browsing APIs, and the isolation options that remain.
- `references/craftbrowsing-mapping.md`: the current crate code mapped onto the real API, including gaps such as stop and reload-cache.
