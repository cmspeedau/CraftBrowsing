# CraftBrowsing code mapped to Servo 0.8.0

## Current state

`crates/craft-engine/src/lib.rs` defines `EngineContext`, `EngineCommand`, `EngineEvent`, and `ServoEngine`. None of it touches `servo`. `ServoEngine::new` spawns a thread named `servo-engine-worker` that reads `EngineCommand`s and replies with synthetic `NavigationEvent`s carrying `status_code: Some(200)`. `events.rs` defines `PageLoadState` and `NavigationEvent`. `compositor.rs` defines `CompositorSurface` as a dirty flag with no renderer. `pipeline.rs` is a placeholder. `craft-ui/src/window.rs` uses `EventLoop::new()` and `ControlFlow::Poll`.

`craft-ui/src/lib.rs` owns a `ServoEngine`, a `cmd_sender`, and an `adblock: Arc<AdblockManager>`. `tab_manager.rs` stores `TabNode` with `container_id`, `group_id`, `state`, and hibernation metadata. `craft-core/src/container.rs` defines `Container` with `user_context_id: u32`. `craft-storage/src/containers.rs` keeps per-container cookie jars in SQLite.

## What has to change

### Engine ownership

- Move `Servo` onto the UI thread. Remove the `servo-engine-worker` thread. Keep a channel only if some producer needs to queue work into the UI loop; the `Servo` itself cannot cross threads.
- `ServoEngine` becomes the owner of `Servo`, the webview map, and the delegate implementations. It lives in `craft-ui` or behind a thin facade in `craft-engine`, but it must be driven from the winit loop.

### EngineCommand mapping

| Current command | Servo 0.8.0 equivalent | Notes |
| --- | --- | --- |
| `CreateContext { tab_id, container_id, url }` | `WebViewBuilder::new(&servo, offscreen_context(size)).url(url).delegate(...).build()` | Store `tab_id -> WebView` and `WebViewId -> tab_id`. `container_id` is recorded but not enforced by the engine. |
| `DestroyContext { tab_id }` | remove from the map and `hide()` if a close frame is needed | Dropping the `WebView` destroys it. |
| `Navigate { tab_id, url }` | `webview.load(Url::parse(url)?)` | Update `EngineContext.current_url` from `notify_url_changed`, not from the command. |
| `Reload { tab_id, bypass_cache }` | `webview.reload()` | `bypass_cache` has no equivalent. Use `load_request` with a `Cache-Control: no-cache` header, or drop the flag. |
| `Stop { tab_id }` | none | No stop API in 0.8.0. Options: `evaluate_javascript("window.stop()")`, or navigate away. Document the gap. |
| `GoBack { tab_id }` | `webview.go_back(1)` | Read `can_go_back()` for chrome state; expect `notify_traversal_complete`. |
| `GoForward { tab_id }` | `webview.go_forward(1)` | Same. |
| `ResizeViewport { tab_id, width, height }` | `webview.resize(PhysicalSize { width, height })` | Resizes the shared context. Use per-tab contexts. |
| `ExecuteScript { tab_id, script }` | `webview.evaluate_javascript(script, callback)` | Callback runs on the UI thread during `spin_event_loop`. |

### EngineEvent mapping

Replace the single `Navigation(NavigationEvent)` variant with events driven by delegate callbacks:

- `notify_url_changed` -> `Navigation { state: ..., url }`
- `notify_load_status_changed` -> `PageLoadState` mapping: `LoadStatus::Started` -> `Started`, `HeadParsed` -> `Loading`, `Complete` -> `Completed`, `Stopped` -> `Failed` or a new `Stopped`.
- `notify_page_title_changed`, `notify_favicon_changed`, `notify_status_text_changed` -> chrome updates.
- `notify_history_changed` -> back/forward button state.
- `notify_new_frame_ready` -> mark the tab dirty and request a redraw; it is not a navigation event.
- `notify_crashed` -> tab error state and removal.
- `notify_closed` -> tab removal.
- `request_permission`, `request_navigation`, `request_create_new`, `show_embedder_control` -> prompts and dialogs, not events. Handle them in the delegate and call back into chrome through a callback or a queue.

### CompositorSurface

Delete `CompositorSurface` as the rendering abstraction. Replace it with:

- `WindowRenderingContext` per window.
- `OffscreenRenderingContext` per tab that needs its own surface.
- A dirty flag per tab, set in `notify_new_frame_ready` and cleared after paint.

Keep `SurfaceMetrics` if the chrome needs it, but source the values from `RenderingContext::size()` and `hidpi_scale_factor()`, not from a parallel struct.

### Tab manager

- `TabState::Hibernated` cannot use engine throttling because 0.8.0 has no `set_throttled`. Drop the `WebView` and keep the URL, title, scroll position if you can capture it, and a screenshot from `take_screenshot`. Restore by recreating the webview.
- `container_id` stays on `TabNode` and `EngineContext`, but it is a chrome and storage concept only. Do not pass it to Servo.
- `is_active` determines which webview gets `show()` and paint priority.
- Group and nesting logic is unaffected; only the engine calls change.

### Adblocking

- Remove the idea that adblocking lives in a network pipeline inside the engine. `craft-adblock` stays as the matcher.
- Implement `WebViewDelegate::load_web_resource` and `ServoDelegate::load_web_resource` to call `craft-adblock` and either drop the load (`DoNotIntercept`) or `intercept(...).cancel()`.
- Keep the filter check synchronous and fast. The request waits on the embedder thread.
- Add tests that feed a `WebResourceRequest` to the delegate and assert block or allow.

### Containers

- `Container.user_context_id` has no consumer in 0.8.0. Do not wire it to Servo.
- Per-container cookie jars in `craft-storage` remain the storage-layer view. The engine's own cookie jar is process-global and persisted to `cookie_jar.json` in `config_dir`.
- If a release claims container isolation, it must run one engine process per container. See `references/containers-and-processes.md`.

### Event loop

- Replace `EventLoop::new()` with `EventLoop::with_user_event()`.
- Replace `ControlFlow::Poll` with `ControlFlow::Wait` and a waker.
- Implement `EventLoopWaker` as a cloneable proxy sending a user event.
- Call `servo.spin_event_loop()` in `user_event` and at the top of `window_event`.
- Move keyboard shortcut handling to run before `notify_input_event`, and re-check on `notify_input_event_handled` with `NotHandled`.

### Storage and config

- Set `Opts.config_dir` to the profile directory so cookies and HSTS persist where the browser expects.
- Set `temporary_storage = true` only for throwaway profiles.
- Keep `craft-storage` as the owner of bookmarks, history, tabs, and container metadata. Do not duplicate that state in the engine.

## Gaps to record, not paper over

- No stop API. `EngineCommand::Stop` cannot be implemented faithfully.
- No cache-busting reload. `bypass_cache` cannot be honored by `reload()`.
- No per-webview container or private mode.
- No engine-level throttle for hibernation.
- No per-webview rect; size is the context size.
- Webview identity is not stored on the `WebView`; the application must keep the map.

## Suggested order of work

1. Add the `servo` dependency and get `cargo build -p craft-engine` to pass with an empty `Servo` on the UI thread.
2. Bootstrap the process, waker, and window context; render `about:blank`.
3. Add one webview, navigation, and paint. Verify by looking.
4. Add tabs with per-tab offscreen contexts.
5. Wire delegates for title, URL, load state, history, and favicon.
6. Add input and focus.
7. Add adblocking through `load_web_resource`.
8. Add permissions, create-new, unload, fullscreen, and embedder controls.
9. Decide and document the container story.
10. Add tests for state mapping and decisions.

Do not start later roadmap phases from this skill.
