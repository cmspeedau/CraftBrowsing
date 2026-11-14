INTERNAL EVIDENCE ONLY - DO NOT SUBMIT THIS TEXT AS AN ISSUE.
Servo's AI policy forbids AI-authored issue text. A human validates the finding
against a Servo main build, then writes the report. Our own standard for the
shape lives in docs/making-submissions.md.

---

Result (on servo 0.8.0): `new IntersectionObserver(...)` throws
`ReferenceError: IntersectionObserver is not defined`, so ABC News renders its
Next.js client-side error page instead of the site. The class is implemented in
the engine but not installed on the window global.

Status vs main: UNVERIFIED. Merge history (#49407, #48581, and today's #49800
which uses IO for lazy loading) suggests main already exposes and uses it.
Confirm with tools/upstream-servo-check before treating this as reportable. If
main is fine, the real gap is that our pinned 0.8.0 predates the fix, and the
finding is "needs a newer engine baseline", not "unimplemented".

## Reproduce (smallest input)

Open fixture io-repro.html (one file, only touches the API):
- main output line: `typeof window.IntersectionObserver`
- on 0.8.0: `undefined`, and construction throws `ReferenceError`.
- expected: `function`, and a no-arg `observe()` runs without throwing.

Real-site repro: load https://www.abc.net.au/ and watch the page console for
`IntersectionObserver is not defined`.

## Evidence (captured, not summarized)

```
[page console] Error: (new ReferenceError("IntersectionObserver is not defined",
  "https://www.abc.net.au/news-web/_next/static/chunks/pages/_app-...js", 1))
[page console] Error: A client-side exception has occurred,
  https://nextjs.org/docs/messages/client-side-exception-occurred
```

The ABC document returns HTTP 200 and parses; the app throws during hydration, so
this is an API-exposure gap, not a network or layout bug.

## Environment

- Engine baseline: servo crate 0.8.0 (latest tagged on crates.io; no nightly).
- Code present: components/script/dom/intersectionobserver/ with a real Constructor.
- Platform: Linux x86_64, X11/Wayland, NVIDIA.
- Related upstream tracking: issue #35767 (Tracking issue: IntersectionObserver).

## Expected on a compliant engine

`IntersectionObserver` is on the Window global (`[Exposed=Window]`) and
`new IntersectionObserver(cb, opts)` constructs and observes per
https://wicg.github.io/intersectionobserver/.
