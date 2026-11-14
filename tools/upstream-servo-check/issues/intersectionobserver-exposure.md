INTERNAL EVIDENCE ONLY - DO NOT SUBMIT THIS TEXT AS AN ISSUE.
Servo's AI policy forbids AI-authored issue descriptions. Use this only to help a
human reproduce and validate the finding, then write the upstream report yourself.
---

IntersectionObserver class exists in script but is not exposed on the Window global

Component: DOM / script
Severity: load-breaking for many real sites

## Summary

`servo-script` ships a full `IntersectionObserver` implementation
(`components/script/dom/intersectionobserver/`) including a `Constructor`, but the
interface is not reachable from JS: `typeof window.IntersectionObserver === "undefined"`
and `new IntersectionObserver(...)` throws `ReferenceError: IntersectionObserver is not
defined`. Sites that construct it eagerly fail before render.

This appears to be the same root cause as the exposure part of tracking issue #35767
(Tracking issue: IntersectionObserver); filing as a focused, minimized repro.

## Environment

- Engine baseline: `servo` crate 0.8.0 (latest tagged release on crates.io; no
  nightly artifacts).
- Platform: Linux x86_64 (X11/Wayland), GPU NVIDIA.
- Embedding: a custom winit + Servo embed (CraftBrowsing). Page console captured via
  `WebViewDelegate::show_console_message`.

## Minimized repro

```html
<script>
  console.log(typeof window.IntersectionObserver);           // "undefined" (expected "function")
  console.log("IntersectionObserver" in window);             // false
  new IntersectionObserver(function () {}, { threshold: 0 }); // ReferenceError
</script>
```

## Observed (from a real site, ABC News, Next.js)

```
[page console] Error: (new ReferenceError("IntersectionObserver is not defined",
  "https://www.abc.net.au/news-web/_next/static/chunks/pages/_app-...js", 1))
[page console] Error: A client-side exception has occurred,
  see https://nextjs.org/docs/messages/client-side-exception-occurred
```

The document itself returns HTTP 200 and parses; the app throws during hydration and
renders its error boundary. So this is an API-exposure gap, not a network or layout bug.

## Expected

`IntersectionObserver` is defined on `Window` (it is `[Exposed=Window]`) and
`new IntersectionObserver(cb, opts)` constructs and `observe()` runs, per
https://wicg.github.io/IntersectionObserver/.

## Notes

- The class has a real `Constructor` and `#[dom_struct]`; my read is that the generated
  `Install`/interface registration is not wired into the window global list, even though the
  implementation is present. If this is intentional (feature-gated), then it should at
  minimum be gated consistently with the spec's default-on status for browsers.
- Happy to retest against `main`; we are currently on the published 0.8.0 because there is
  no prebuilt nightly to run.
