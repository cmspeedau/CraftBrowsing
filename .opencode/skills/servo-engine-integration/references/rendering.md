# Rendering with Servo 0.8.0

## Contexts

- `WindowRenderingContext`: surfman-backed OpenGL context bound to a winit window handle. Created on the UI thread after the window exists. `WindowRenderingContext::new(display_handle, window_handle, inner_size)`.
- `WindowRenderingContext::offscreen_context(size) -> OffscreenRenderingContext`: creates an offscreen framebuffer sharing the parent's GL context. Use one per tab that needs its own surface, or one per window if only one webview is visible.
- `OffscreenRenderingContext`: an FBO with `prepare_for_rendering` (binds the framebuffer) and `present` (no-op). `parent_context()` returns the owning window context. `render_to_parent_callback()` builds a blit closure from the offscreen FBO into the parent context at a target rect.
- `SoftwareRenderingContext`: surfman-free, in-memory. Use for headless tests and for environments without a display server.

A `WebView` is always the size of its `RenderingContext`. `WebView::resize(new_size)` resizes the context, and therefore every other `WebView` sharing that context. Sharing a context between tabs is only correct when they are never visible at the same time, or when they share one rect.

## Lifecycle order

1. Create the window.
2. Create the `WindowRenderingContext` and call `make_current()`.
3. Create `Servo`.
4. Create offscreen contexts and webviews.
5. In `RedrawRequested`: make the webview's context current, `webview.paint()`, then `present()` on the context that owns the screen (the window context, or the offscreen's `parent_context()`).

The winit_minimal example does exactly this with a single webview:

```rust
WindowEvent::RedrawRequested => {
    state.webviews.borrow().last().unwrap().paint();
    state.rendering_context.present();
}
```

For a browser with tabs, call `webview.paint()` for the visible tab, then composite its offscreen framebuffer into the window context using `render_to_parent_callback()` inside the GUI paint, then `window_context.present()`.

## HiDPI

- `WebViewBuilder::hidpi_scale_factor(Scale::new(window.scale_factor() as f32))`.
- On `ScaleFactorChanged`: `webview.set_hidpi_scale_factor(...)`, resize the contexts, redraw.
- `webview.device_pixels_per_css_pixel()` returns the effective scale, including page zoom and pinch zoom.
- Coordinates from winit are physical pixels; divide by the scale factor when the API wants device-independent pixels (for example, the toolbar geometry in servoshell).

## Resize

- `WindowEvent::Resized(new_size)`: set the cell, `window_rendering_context.resize(new_size)`, then `webview.resize(new_size)`, then `request_redraw()`.
- Resize before paint. Painting into a buffer of the old size stretches the frame.
- `OffscreenRenderingContext::resize` blits the old framebuffer into the new one, so a resize does not flash white.
- Minimum webview size is 1x1; smaller requests are clamped by `WebView::resize`.

## Redraw and animation

- `notify_new_frame_ready` is the only paint trigger. Call `window.request_redraw()` and nothing else there.
- `animating()` on a webview means CSS animations, transitions, or `requestAnimationFrame` are running. Spin the event loop at display cadence until `notify_animating_changed(false)`.
- Do not use `ControlFlow::Poll` to keep animations alive. Poll burns a core while idle and masks missing waker calls.

## Compositing multiple tabs

Two workable models:

1. Active tab only: one offscreen context, swap the webview's URL or recreate the webview when switching tabs. Cheapest, but loses scroll position unless you keep the webview alive in a hidden context.
2. One offscreen context per tab: keep every tab's webview alive, paint only the active one into the window context, and use `render_to_parent_callback()` for the active tab's rect. Servoshell uses a single context per window because egui owns the compositor; a vertical tab browser with previews needs per-tab contexts.

If you keep webviews alive but hidden, call `webview.hide()` so Servo stops painting them and `show()` when they become visible again.

## Headless and tests

Use `SoftwareRenderingContext` for unit tests and CI. Drive the loop manually: create the context, build the webview, call `servo.spin_event_loop()`, then `take_screenshot` or `evaluate_javascript` and assert. No display server is required.

## Common failures

- Page is blank: `notify_new_frame_ready` was not wired to `request_redraw`, or `paint()` and `present()` are out of order, or the context was never made current.
- Page paints once then stops: `spin_event_loop` is not being called on `user_event` and `window_event`, so delegate callbacks queue up.
- Stretched or clipped frame after resize: resize happened after paint, or only the window context was resized and not the offscreen one.
- Crash in `make_current`: two contexts are being used on different threads, or a context was dropped while still current.
- Screenshot is upside down: `read_to_image` returns a flipped buffer; flip vertically before encoding, as servoshell does.
- Animations stutter: the loop is on `ControlFlow::Poll` with no throttling, or `notify_animating_changed` is ignored.
- `present()` on an offscreen context does nothing: it is a no-op by design. Present on the window context.
