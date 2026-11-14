INTERNAL EVIDENCE ONLY - DO NOT SUBMIT THIS TEXT AS AN ISSUE.
Servo's AI policy forbids AI-authored issue descriptions. Use this only to help a
human reproduce and validate the finding, then write the upstream report yourself.
---

HTTP/2 navigation fails on www.youtube.com: local h2 reset with PROTOCOL_ERROR (Library)

Component: net
Severity: load-breaking (top-level navigation fails outright)

## Summary

A plain GET navigation to `https://www.youtube.com/` fails at the transport layer with an
HTTP/2 stream reset. The page shows:

```
Could not load the requested page: client error (SendRequest)
```

The reset is initiated by hyper's h2 **library** (not the server):

```
network error: hyper_util::client::legacy::Error(
  SendRequest,
  hyper::Error(Http2, Error { kind: Reset(StreamId(1), PROTOCOL_ERROR, Library) })
)
```

`Initiator = Library` on `StreamId(1)` (the first request) means our client is sending
something that is invalid for HTTP/2, i.e. a conformance bug on our side rather than a
server rejection.

## Environment

- Engine baseline: `servo` crate 0.8.0.
- HTTP stack (from Cargo.lock): `hyper 1.12.0`, `h2 0.4.20`,
  `hyper-rustls 0.27.10`, `rustls 0.23.46`, `tokio-rustls 0.26.6`.
- `servo-net` builds the client with both protocols enabled:
  `hyper_rustls::HttpsConnectorBuilder::new() ... .enable_http1().enable_http2()`,
  so ALPN negotiates `h2` with Google and the failure is on the h2 path.
- Platform: Linux x86_64.

## Repro steps

1. Build against `servo` 0.8.0 (crates.io).
2. Navigate to `https://www.youtube.com/` (or `https://youtube.com/`, which 301s there).
3. Observe the document never loads; log shows the `Reset(StreamId(1), PROTOCOL_ERROR, Library)`
   line above.

## Related / observed

- Same signature observed on an ABC News subresource.
- I noticed `http_loader.rs` inserts `TRANSFER_ENCODING: chunked` for streamed bodies
  with the comment "this should not be set for HTTP/2 (currently not supported?)".
  That alone explains failures for POST/streamed bodies over h2 (HTTP/2 forbids
  `Transfer-Encoding`), but the YouTube failure is a GET with no body, so there is a
  second, separate h2 conformance issue for plain GETs.
- Existing issues that may overlap: #49396 (YouTube content works intermittently),
  #39785 (PROTOCOL_ERROR on gemini, Android).

## Expected

Standard HTTP/2 request framing that the `h2` library accepts and Google's servers honor,
so the navigation succeeds like HTTP/1.1.

## Questions

- Is there a supported way to make the embedder prefer HTTP/1.1 for testing? The only
  ALPN pin to `http/1.1` I found in the code is the WebSocket handshake
  (`http_loader.rs`), so normal navigation has no h2->h1 fallback.
- Would a `--disable-http2`/pref or an opt-in `enable_http2` be acceptable upstream while
  this is sorted?
