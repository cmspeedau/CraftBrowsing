INTERNAL EVIDENCE ONLY - DO NOT SUBMIT THIS TEXT AS AN ISSUE.
Servo's AI policy forbids AI-authored issue text. A human validates the finding
against a Servo main build, then writes the report. Shape per docs/making-submissions.md.

---

Result (on servo 0.8.0): navigating to https://www.youtube.com/ fails at the
transport layer with `Could not load the requested page: client error
(SendRequest)`. The failure is a local HTTP/2 stream reset, initiated by our own
h2 client (Initiator = Library), not a server rejection.

Status vs main: likely still open. The only recent h2 merges are a dependency
bump (#48917, h2 0.4.19 -> 0.4.20) and a connection timer (#48657); neither is a
conformance fix. Our Cargo.lock already pins h2 0.4.20 (same as main), so a
version bump is unlikely to help. Verify with tools/upstream-servo-check.

## Reproduce (smallest input)

1. servo 0.8.0, RUST_LOG=warn,h2=trace,hyper=trace.
2. Navigate to https://www.youtube.com/ (or https://youtube.com/, which 301s there).
3. Observe the document never loads. Grep the log for PROTOCOL_ERROR.

Cross-check (proves it is our client, not Google): run curl against the same URL
on the same host. If curl negotiates h2 and 200s where servo's client self-resets,
that difference is the report.

## Evidence (captured)

```
network error: hyper_util::client::legacy::Error(
  SendRequest,
  hyper::Error(Http2, Error { kind: Reset(StreamId(1), PROTOCOL_ERROR, Library) })
)
script::event_loop::script_thread: Network error: HTTP failure: client error (SendRequest)
```

Same signature also killed an ABC News subresource, so this is repeating on strict
h2 servers, not a one-off. Reset is on StreamId(1), the first request.

## Where in the code

- servo-net builds the client with both protocols:
  `hyper_rustls::HttpsConnectorBuilder::new() ... .enable_http1().enable_http2()`
  (connector.rs::create_http_client). ALPN selects h2 with Google.
- http_loader.rs sets `TRANSFER_ENCODING: chunked` for streamed bodies with the
  comment "this should not be set for HTTP/2". That explains failures for
  POST/streamed bodies (HTTP/2 forbids Transfer-Encoding), but the YouTube case is
  a GET with no body, so there is a second, separate conformance bug for plain GETs.
- No embedder switch disables h2 for navigation. The only ALPN pin to http/1.1 is
  the WebSocket path (http_loader.rs). So normal navigation has no h2 -> h1 fallback.

## Environment

- servo crate 0.8.0. hyper 1.12.0, h2 0.4.20, hyper-rustls 0.27.10, rustls 0.23.46,
  tokio-rustls 0.26.6 (from Cargo.lock). Linux x86_64.

## Questions for maintainers

- Is there a supported way to prefer HTTP/1.1 in a servo-net client build for
  testing this?
- Related open issues that may share the root cause: #49396 (YouTube content
  intermittent), #39785 (PROTOCOL_ERROR on gemini, Android), #47611 (gstreamer
  registry, media).
