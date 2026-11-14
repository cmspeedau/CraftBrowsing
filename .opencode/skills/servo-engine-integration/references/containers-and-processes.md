# Containers, private browsing, and processes

## What 0.8.0 actually offers

Process and sandbox controls live in `Opts`:

- `multiprocess`: run script and layout in separate content processes.
- `sandbox`: enter the platform sandbox in content processes.
- `background_hang_monitor`: sample for hangs.
- `force_ipc`: force IPC even in single-process mode (useful for testing).
- `config_dir`: directory for persisted cookies, HSTS, and auth cache.
- `temporary_storage`: in-memory storage instead of disk for storage threads.
- `certificate_path`, `ignore_certificate_errors`: TLS configuration.
- `hard_fail`: exit on thread failure instead of showing `about:failure`.

Content processes are re-execs of the same binary. Detect them before any window is created:

```rust
let mut token = None;
let mut take_next = false;
for arg in std::env::args() {
    if take_next { token = Some(arg); break; }
    if arg == "--content-process" { take_next = true; }
}
if let Some(token) = token {
    return servo::run_content_process(token);
}
```

`run_content_process` is gated behind the `multiprocess` feature, which is on by default. Each content process calls `opts::initialize_options` itself from the opts sent over IPC, so a second `Servo` in the browser process is still the only `Servo`.

Content processes are grouped by script event loop, which is roughly by origin and opener, not by tab. One tab can span several processes, and one process can host several tabs. Do not assume one process equals one tab.

## What 0.8.0 does not offer

- No per-webview user context or container id. `WebViewBuilder` has no container, cookie-jar, or storage-partition option.
- No per-webview private-browsing flag. The engine maintains public and private resource threads, but the embedding API does not let you pick one per webview.
- `Container.user_context_id` in `craft-core` and `ContainerId` in `craft-storage` have no consumer in the engine.
- `SiteDataManager` can list and clear site data and cookies, but only for the whole process, split into public and private jars. It cannot assign a jar to a webview.
- `NetworkManager` exposes cache inspection, cache clearing, and online state only.
- `config_dir` is process-global, so cookies, HSTS, and auth cache are shared across every tab and container in the process.

In short, the Firefox-style Multi-Account Container model in `docs/` and `craft-storage` cannot be enforced at the engine level in 0.8.0. The storage layer can keep per-container cookie jars, but the network stack will still use its own.

## Options for CraftBrowsing

Three honest choices. Pick one and document it; do not pretend a fourth exists.

1. One engine process per container. The UI process hosts the chrome and composites frames from one Servo process per container. This is the only option that gives real isolation with 0.8.0, and it is a large change: separate event loops, separate `config_dir` per container, separate content-process trees, and a frame transport between processes.
2. Upstream a per-webview user context in Servo. Add a container or user-context id to `WebViewBuilder` and to the constellation and net layers, then partition cookies, cache, and storage by it. This is the correct long-term fix and needs a Servo contribution.
3. Drop the claim. Remove container isolation from the browser's stated features until one of the above lands, and keep per-container cookie jars only as a best-effort convenience.

A fourth option, intercepting requests through `load_web_resource` and rewriting cookies, does not work: the interceptor can replace a response, but it cannot partition the request's cookie jar or storage, and re-implementing HTTP there is not viable.

Private browsing has the same shape. `temporary_storage` can keep localStorage in memory for the whole process, but it cannot make one tab ephemeral while others persist. Ephemeral mode needs option 1 or 2 as well.

## Security invariants to keep

From `AGENTS.md`, none of these may be weakened for convenience:

- Security protections on by default and individually toggleable globally and per site or container.
- Sync dormant until enabled. No listeners, broadcasts, or probes before that.
- No telemetry or default network calls.
- Containers partition cookies, storage, and caches. Never let data cross a boundary. If the engine cannot enforce this yet, the browser must not claim that it does.
- Credentials stay out of the process through native messaging.
- Vault export and sync keep their documented crypto parameters.

## Practical guidance

- Set `config_dir` to a per-profile directory and `temporary_storage = true` only for a throwaway profile. Never share a `config_dir` between a normal and a private profile.
- Keep `sandbox = true` in production builds. Test with it on; sandbox bugs often only appear with it enabled.
- Keep `hard_fail = false` in release builds so a thread failure shows `about:failure` instead of killing the browser.
- When running content processes, ensure the binary can re-exec itself. In development, `cargo run` works because the same binary is used. In packaged builds, the AppImage or installer must preserve argv[0] resolution.
- Do not add a `Servo` per container in one process. It will panic on the second `initialize_options`.
- Document any isolation limitation in the UI and in `docs/`, and gate container-scoped features behind the chosen option.
- If you upstream a container API, keep `craft-core::Container.user_context_id` as the value passed through and add a conversion layer rather than renaming it.
