# Input and focus

## The rule

All input goes through the webview that owns the focus, in device pixels, followed by `servo.spin_event_loop()`. Servo never sees winit events directly. If a page does not respond to keyboard or mouse, the cause is almost always a missing `spin_event_loop` call or a missing focus call.

## Focus

- `webview.set_focused(true)` when content takes focus, `false` when chrome does. All webviews start focused, so chrome must explicitly defocus them.
- `webview.set_focused` sends `SetWebViewHasSystemFocus` to the constellation. Without it, keyboard events are dropped.
- Track focus in the chrome and restore it on window regain, or the first keystroke after alt-tab lands in the wrong place.
- `notify_cursor_changed(webview, cursor)` updates the OS cursor. Forward it to winit's `set_cursor`.

## Mouse

- `InputEvent::MouseMove(MouseMoveEvent::new(WebViewPoint::Device(point)))`.
- `InputEvent::MouseButton(MouseButtonEvent::new(action, button, point))` where `action` is `MouseButtonAction::Down` or `Up`, and `button` is `MouseButton::Left`, `Right`, `Middle`, `Back`, `Forward`, or `None`.
- `InputEvent::MouseLeftViewport(MouseLeftViewportEvent)` when the pointer leaves the webview rect.
- Convert winit `LogicalPosition` to physical by multiplying by the scale factor before constructing `DevicePoint`.

## Wheel and trackpad

```rust
InputEvent::Wheel(WheelEvent::new(
    WheelDelta { x, y, z, mode },
    WebViewPoint::Device(point),
))
```

- `WheelMode::DeltaLine` for line-based deltas (multiply by 76.0 for Servo's expected pixel-ish scaling, as the example does).
- `WheelMode::DeltaPixel` for pixel deltas from trackpads.
- Do not synthesize wheel events from scroll gestures the platform already provides; servoshell handles gesture recognition separately.

## Keyboard and IME

- `InputEvent::Keyboard(KeyboardEvent)` from `keyboard_types`. Fill `state`, `key`, `code`, `location`, `modifiers`, `repeat`, `is_composing`, and `text`.
- `InputEvent::Ime(ImeEvent::Composition(CompositionEvent))` for composition, `ImeEvent::Dismissed` when composition ends.
- `InputEvent::EditingAction(ClipboardAction::Copy | Cut | Paste)` for clipboard commands that come from menus or shortcuts.
- Map winit's `KeyEvent` to `KeyboardEvent` carefully: distinguish `key` (logical) from `code` (physical), handle dead keys and composition, and respect `modifiers`. Servoshell's `desktop/keyutils.rs` is the reference implementation; do not reimplement from scratch without reading it.
- IME support requires `show_embedder_control` handling for `EmbedderControl::InputMethod`.

## Touch

- `InputEvent::Touch(TouchEvent)` with the touch identifier, phase, and location. Touch points are in device pixels.

## Event bubbling

`notify_input_event` returns an `InputEventId`. When the page does not consume the event, `WebViewDelegate::notify_input_event_handled(webview, id, result)` fires with `InputEventResult::NotHandled` (or `Handled`). Use this to chain unhandled shortcuts back to chrome, for example Escape when nothing is focused or Backspace in an empty field. Do not guess from timing.

## Shortcuts

Keep browser shortcuts (new tab, close tab, reload, focus omnibox) out of the webview's input path. Check them in the winit handler before calling `notify_input_event`, and only when the webview is not focused or the event is reported unhandled. Otherwise a page cannot bind the same key.

## Accessibility

- Activate the AccessKit tree with `webview.set_accessibility_active(true)` when accessibility is on.
- `webview.accesskit_tree_id()` returns the tree id to graft into the host's AccessKit tree, as servoshell does in `webview_widget.rs`.
- Forward AccessKit actions with `servo.forward_accessibility_action(request)`.
- `notify_accessibility_tree_update` fires when the tree changes; update the host tree in response.

## Common failures

- Keystrokes never reach the page: the webview was never `set_focused(true)`, or `spin_event_loop` was not called after `notify_input_event`.
- Shortcuts fire twice: the chrome handler and the page both consume the event because bubbling was not checked.
- IME composition breaks: `Ime` events were not forwarded, or `InputMethod` embedder control was not handled.
- Cursor stays an arrow over links: `notify_cursor_changed` is ignored.
- Touch scrolling is jittery: wheel deltas are being sent as `DeltaLine` instead of `DeltaPixel`.
