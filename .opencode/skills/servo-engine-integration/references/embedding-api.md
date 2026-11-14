# Servo 0.8.0 embedding API

Source of truth: the `servo` 0.8.0 crate plus `servo-paint-api` 0.8.0, `servo-config` 0.8.0, `servo-embedder-traits` 0.8.0. Docs.rs may fail to build; download the crate from crates.io (see the skill).

## Core types

- `Servo` (Rc-wrapped, not Send): the engine handle.
- `ServoBuilder`: `default()`, `opts(Opts)`, `preferences(Preferences)`, `event_loop_waker(Box<dyn EventLoopWaker>)`, `protocol_registry(ProtocolRegistry)`, `build() -> Servo`.
- `WebView`: `Rc<RefCell<...>>`, not Send. Always the size of its `RenderingContext`.
- `WebViewBuilder`: `new(&Servo, Rc<dyn RenderingContext>)`, `new_for_create_request(&Servo, Rc<dyn RenderingContext>, responder)` (pub(crate), reached via `CreateNewWebViewRequest::builder`), `delegate(Rc<dyn WebViewDelegate>)`, `url(Url)`, `hidpi_scale_factor(Scale<f32, DeviceIndependentPixel, DevicePixel>)`, `user_content_manager(Rc<UserContentManager>)`, `clipboard_delegate(Rc<dyn ClipboardDelegate>)`, `gamepad_delegate(Rc<dyn GamepadDelegate>)`, `geolocation_delegate(Rc<dyn GeolocationDelegate>)`, `build() -> WebView`.
- `RenderingContext` trait: `prepare_for_rendering`, `read_to_image`, `size`, `size2d`, `resize`, `present`, `make_current`, `gleam_gl_api`, `glow_gl_api`, `create_texture`, `destroy_texture`, `connection`, `refresh_driver`.
- `WindowRenderingContext`: `new(display_handle, window_handle, size)`, `new_with_refresh_driver(...)`, `offscreen_context(size) -> OffscreenRenderingContext`, `take_window`, `set_window`, `surfman_details`, `resize`, `make_current`, `present`.
- `OffscreenRenderingContext`: `parent_context()`, `render_to_parent_callback() -> Option<RenderToParentCallback>`, `size`, `resize`, `prepare_for_rendering`, `present` (no-op), `make_current` (delegates to parent).
- `SoftwareRenderingContext`: `new(size) -> Result<Self, Error>`. For headless tests and CI.

## Servo methods

- `delegate()` / `set_delegate(Rc<dyn ServoDelegate>)`
- `spin_event_loop()`
- `setup_logging()`
- `set_preference(&str, PrefValue)`
- `network_manager() -> Ref<NetworkManager>`
- `site_data_manager() -> &SiteDataManager`
- `forward_accessibility_action(ActionRequest)`
- `initialize_gl_accelerated_media(display, api, context)` (static-ish, before creating Servo)
- `paint() -> Ref<Paint>` (crate-internal, used via WebView methods)
- `event_loop_waker() -> &dyn EventLoopWaker`
- `constellation_proxy()`, `javascript_evaluator_mut()`, `webviews_mut()`

## WebView methods

Identity and state: `delegate`, `clipboard_delegate`, `gamepad_delegate`, `geolocation_delegate`, `id`, `rendering_context`, `load_status`, `url`, `status_text`, `page_title`, `favicon`, `focused`, `cursor`, `animating`, `size`, `hidpi_scale_factor`.

Mutation: `set_focused(bool)`, `resize(PhysicalSize<u32>)`, `set_hidpi_scale_factor(Scale<f32, DeviceIndependentPixel, DevicePixel>)`, `show()`, `hide()`, `notify_theme_change(Theme)`, `load(Url)`, `load_request(UrlRequest)`, `reload()`, `go_back(usize) -> TraversalId`, `go_forward(usize) -> TraversalId`, `can_go_back()`, `can_go_forward()`, `clear_session_history()`, `set_page_zoom(f32)`, `page_zoom()`, `adjust_pinch_zoom(f32, DevicePoint)`, `pinch_zoom()`, `device_pixels_per_css_pixel()`, `exit_fullscreen()`, `toggle_webrender_debugging(WebRenderDebugOption)`, `capture_webrender()`, `paint()`, `user_content_manager()`, `evaluate_javascript<T: ToString>(script, callback)`, `take_screenshot(Option<WebViewRect>, callback)`, `set_accessibility_active(bool) -> Option<TreeId>`, `accesskit_tree_id()`, `notify_scroll_event(Scroll, WebViewPoint)`, `notify_input_event(InputEvent) -> InputEventId`, `notify_media_session_action_event(MediaSessionActionType)`, `notify_theme_change(Theme)`.

Important: there is no `stop()` and no cache-busting reload parameter. `reload()` always uses the cache. `evaluate_javascript` is the escape hatch for `window.stop()`.

## Delegate traits

`WebViewDelegate` (one per webview, may be shared): notifications `screen_geometry`, `notify_url_changed`, `notify_page_title_changed`, `notify_status_text_changed`, `notify_animating_changed`, `notify_load_status_changed`, `notify_cursor_changed`, `notify_favicon_changed`, `notify_new_frame_ready`, `notify_history_changed`, `notify_traversal_complete`, `notify_closed`, `notify_input_event_handled`, `notify_crashed`, `notify_media_session_event`, `notify_fullscreen_state_changed`, `notify_accessibility_tree_update`.

Requests: `request_navigation(&self, webview, NavigationRequest)` (allow/deny), `request_unload(webview, AllowOrDenyRequest)`, `request_move_to(webview, DeviceIntPoint)`, `request_protocol_handler(webview, ProtocolHandlerRegistration, AllowOrDenyRequest)`, `request_resize_to(webview, DeviceIntSize)`, `request_create_new(parent_webview, CreateNewWebViewRequest)`, `request_permission(webview, PermissionRequest)`, `request_authentication(webview, AuthenticationRequest)`, `show_bluetooth_device_dialog(webview, BluetoothDeviceSelectionRequest)`, `show_embedder_control(webview, EmbedderControl)`, `hide_embedder_control(webview, EmbedderControlId)`, `load_web_resource(webview, WebResourceLoad)`, `show_notification(webview, Notification)`, `show_console_message(webview, ConsoleLogLevel, String)`.

`ServoDelegate` (engine-wide): `notify_error`, `notify_devtools_server_started`, `request_devtools_connection`, `load_web_resource` (no-webview loads, including service workers), `show_notification`, `show_console_message`.

### NavigationRequest

`url: Url`. `allow()` / `deny()`. Drop defaults to allow, so a privacy browser must call one explicitly.

### PermissionRequest

`feature() -> PermissionFeature`. `allow()` / `deny()`. Deny unknown features by default.

### WebResourceLoad (adblock hook)

`request() -> &WebResourceRequest` (method, url, headers, destination, referrer_url, is_for_main_frame, is_redirect). `intercept(WebResourceResponse) -> InterceptedWebResourceLoad`. `InterceptedWebResourceLoad::send_body_data(Vec<u8>)`, `finish()`, `cancel()`. Dropping a `WebResourceLoad` sends `DoNotIntercept`. Blocking a request is `load.intercept(WebResourceResponse::new(url)).cancel()`.

### CreateNewWebViewRequest

`builder(rendering_context) -> WebViewBuilder` (the only public way to create the popup). Dropping the request denies the popup.

### EmbedderControl

`SelectElement`, `ColorPicker`, `FilePicker`, `InputMethod`, `SimpleDialog`, `ContextMenu`. `id()` returns `EmbedderControlId`. `show_embedder_control` and `hide_embedder_control` are paired by id.

## Content processes

`run_content_process(token: String)` is gated behind the `multiprocess` feature, which is on by default. It is the content-process entry point. The `--content-process <token>` argument pattern is the only supported launch shape. See `tests/multiprocess.rs` in the servo crate for the exact parsing.

## Options and preferences

`Opts` fields: `time_profiling`, `time_profiler_trace_path`, `hard_fail`, `debug`, `multiprocess`, `force_ipc`, `background_hang_monitor`, `sandbox`, `random_pipeline_closure_probability`, `random_pipeline_closure_seed`, `shaders_path`, `config_dir`, `temporary_storage`, `certificate_path`, `host_file`, `ignore_certificate_errors`, `unminify_js`, `local_script_source`, `unminify_css`. Defaults: `hard_fail = true`, `multiprocess = false`, `sandbox = false`, `temporary_storage = false`.

`config_dir` is the directory where Servo persists `cookie_jar.json`, `auth_cache.json`, and `hsts_list.json` for the whole process. `temporary_storage` switches localStorage-style storage to in-memory. Both are process-global.

`Preferences` is a large struct with `fonts_*`, `devtools_server_*`, `dom_*`, `dom_async_clipboard_enabled`, `dom_bluetooth_enabled`, and many more. `servo::prefs::set(Preferences)`, `servo::prefs::get()`, `servo.set_preference(name, PrefValue)`, `Preferences::set_value`. `PrefValue` is the variant used by `set_preference`.

`ProtocolRegistry` via `servo::protocol_handler` module: `with_internal_protocols()`, `merge(...)`. Used to register `craft://` and other custom schemes with handlers.

`UserContentManager`: `new(&Servo)`, `add_script(Rc<UserScript>)`, `remove_script`, `add_stylesheet`, `remove_stylesheet`. `UserScript::new(script, source_file)`.

`NetworkManager`: `cache_entries()`, `clear_cache()`, `set_online_state(bool)`.

`SiteDataManager`: `site_data(storage_types)`, `clear_site_data(sites, storage_types)`, `clear_cookies(callback)`, `clear_session_cookies(callback)`, `cookies_for_url(url, CookieSource)`, `cookies_for_url_async`, `set_cookie_for_url`, `clear_cookies_for_sites`. `CookieSource` is `servo::CookieSource` (public or private).

`EventLoopWaker`: trait with `clone_box()` and `wake()`. Must be `Send + Sync + 'static`. The waker may fire from another thread; the implementation must post a wake to the UI thread.

## Input types

`InputEvent` variants: `EditingAction(ClipboardAction)`, `Gamepad(GamepadEvent)` (feature), `Ime(ImeEvent)`, `Keyboard(KeyboardEvent)`, `MouseButton(MouseButtonEvent)`, `MouseLeftViewport(MouseLeftViewportEvent)`, `MouseMove(MouseMoveEvent)`, `Touch(TouchEvent)`, `Wheel(WheelEvent)`.

`WheelEvent::new(WheelDelta { x, y, z, mode }, WebViewPoint)`. `WheelMode::DeltaLine` or `DeltaPixel`.

`WebViewPoint`: `Device(DevicePoint)` or `Page(Point2D<f32, CSSPixel>)`. `From<DevicePoint>` and `From<Point2D<f32, CSSPixel>>`.

`KeyboardEvent` is from `keyboard_types`, re-exported as `servo::Key`, `servo::Code`, `servo::Modifiers`, `servo::NamedKey`, `servo::KeyState`, `servo::Location`, `servo::CompositionEvent`, `servo::CompositionState`. `KeyboardEvent` has `state`, `key`, `code`, `location`, `modifiers`, `repeat`, `is_composing`, `text`.

`ImeEvent`: `Composition(CompositionEvent)` or `Dismissed`.

`MouseButtonEvent::new(action, button, point)`, with `is_compatibility_event_for_touch`.

`MouseMoveEvent::new(point)`.

`InputEventId` returned by `notify_input_event`. `InputEventResult` delivered via `notify_input_event_handled(webview, id, result)`.

## Re-exports worth knowing

`servo::` also exposes `accesskit`, `embedder_traits::*` (including `submit_resource_reader`), `image::RgbaImage`, `keyboard_types`, `webrender_api::units::*` (`DevicePixel`, `DevicePoint`, `DeviceIntPoint`, `DeviceIntRect`, `DeviceIntSize`, `DevicePixel`), `servo_url::ServoUrl`, `servo_config::{opts, prefs}`, `servo_geometry`, `net_traits::CookieSource`, `paint::WebRenderDebugOption`, `media` (GL types), `resources`, `ServoUrl`, `Zero`.

## Accessibility

`set_accessibility_active(true)` activates the AccessKit tree; `accesskit_tree_id()` returns the tree id. `forward_accessibility_action(ActionRequest)` forwards an AccessKit action. See servoshell's `desktop/headed_window.rs` and `webview_widget.rs` for grafting the tree into the host's AccessKit tree.
