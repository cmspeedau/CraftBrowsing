# AGENTS.md

Guidance for AI coding agents (and human contributors) working on CraftBrowsing. Read it together with [README.md](README.md), [docs/roadmap.md](docs/roadmap.md), and [docs/architecture.md](docs/architecture.md).

## Project

CraftBrowsing is a privacy-first, pure-Rust web browser built on the Servo rendering engine. It targets Windows, macOS, and Linux (AppImage with `zsync` delta updates) and ships adblocking, containers, tab groups, a transfer engine, sync (off by default), extensions, and a Rust workspace split by subsystem.

The roadmap is the scope authority. The code and `Cargo.toml` files are the truth for what actually exists; some docs still describe planned crates.

## Skills to load

Load the matching skill with the `skill` tool before starting the work, not after. Treat a skill's instructions as the checklist for that area.

| Work area | Load these skills |
| --- | --- |
| Any Rust change | `rust` |
| Servo embedding, rendering contexts, winit event loop, delegates, content processes (`craft-engine`) | `servo-engine-integration` (plus `rust`) |
| Tests, benchmarks, proptest, coverage | `rust-testing` |
| Browser chrome, settings pages, dialogs, themes, any visual surface in `craft-ui` | `antislop` first, then `antislop-ui`, `antislop-human`, `antislop-layoutmobile`, `antislop-copywriting`, `antislop-code` |
| Transfer engine, segmented HTTP, resume, torrent plumbing (`craft-transfers`) | `bounded-transfer-workers` |
| Sync crypto, keys, vault export (`craft-sync`) | `crypto` |
| Sync protocol, wire formats, versioning | `api-protocol` |
| mDNS discovery, relay, self-hosted sync transport | `self-hosted-mesh` |
| SQLite schema, migrations, query tuning (`craft-storage`) | `sql` |
| Fetching user-supplied URLs, proxies, downloaders, SSRF-sensitive code | `safe-fetching-ssrf` |
| RSS/Atom parsing, reader view extraction, HTML to text | `web-scraping` |
| AppImage, `zsync`, desktop entry, release packaging | `appimage` |
| GitHub Actions workflows, CI, release automation | `github-actions` |
| Extension runtime and injected JavaScript | `javascript` |
| HTML, CSS, or markup produced by the browser or extensions | `html`, `css` |

### Skill rules

- `antislop` resolves its own mode (during, after, or ask) on first use in a session. Follow its resolution order: an explicit user choice in the conversation, then the saved preference file, then ask. Do not silently pick a mode for the user.
- There is no `DESIGN.md` in this repository yet. Do not invent a visual identity for it. For UI work, take direction from the user's brief or the existing chrome, and if neither exists follow `antislop` R-37 (ask, or label the result "draft without direction" with the honest default dials).
- `antislop` is the filter, not the direction. Its Delivery Gate is mandatory for UI deliverables; include the PASS/FAIL report with the change.
- The `antislop` sub-skills are meant to be loaded with the core, not on their own.
- If a skill conflicts with this file on architecture, security, or scope, stop and ask the user instead of guessing.

## Workspace layout

| Crate | Purpose |
| --- | --- |
| `craft-core` | Shared types: tabs and groups, containers, downloads, permissions |
| `craft-engine` | Servo embedding, request pipeline, compositor and events |
| `craft-ui` | Browser chrome: window, omnibox, tab manager, settings surface |
| `craft-storage` | SQLite layer: containers, downloads, permissions, privacy settings |
| `craft-adblock` | Filter-list parsing and request matching |
| `craft-sync` | E2EE sync: crypto, protocol, dormant until enabled |
| `craft-transfers` | Segmented HTTP and magnet/torrent transfers |
| `craft-plugins` | Extension host and sandboxed JavaScript runtime |

Keep boundaries clean: `craft-ui` may depend on the other crates; core and subsystem crates must not depend on `craft-ui`.

## Build and quality gates

Run these before declaring work complete:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p craft-core -p craft-engine -p craft-storage -p craft-transfers
cargo test -p craft-transfers -p craft-storage
```

- Never claim a build, test, or lint passed unless you ran it and saw the output.
- Fix new clippy warnings rather than suppressing them. A justified `#[allow]` needs a comment explaining why.
- Separate pre-existing failures from regressions caused by your change, and say which is which.
- Add tests for new behavior, bug fixes, and edge cases. Storage and transfer logic are expected to have tests.
- Set `RUST_LOG` and run the UI when the change is user-visible. A chrome change that was never launched is not finished.

## Code conventions

- Edition 2024, workspace resolver 3, latest stable toolchain. `craft-storage` and `craft-transfers` declare MSRV 1.85.
- Shared dependency versions live in the root `[workspace.dependencies]`. Add a dependency there first, then reference it with `{ workspace = true }`.
- Prefer `thiserror` for typed errors at crate boundaries; avoid `anyhow` in library APIs. No `unwrap()` or `expect()` on untrusted input outside tests.
- Validate everything crossing a boundary: paths, URLs, headers, manifests, filter lists, downloaded files, sync payloads.
- Follow existing module layout and naming. Use `tracing` for logs, never `println!` in library code.
- Keep changes minimal and focused. No opportunistic refactors or unrelated formatting churn.
- Commit style follows the history: `feat(area):`, `fix(area):`, `ci:`, `docs:`, `style:`. One concern per commit.

## Security and privacy invariants

These are not optional features; do not regress them for convenience:

- Security protections are on by default, and every one is individually toggleable globally and per site or container.
- Sync is completely dormant until the user enables it. No listeners, broadcasts, or background probes before that.
- No telemetry, analytics, or default network calls.
- Containers partition cookies, storage, and caches. Never let data cross a container boundary.
- Credentials stay out of the process: password managers are reached through native messaging.
- Hardware access (WebSerial, WebUSB, WebHID) is opt-in, per site, and killable at any time.
- Vault export and sync use ChaCha20-Poly1305 with Argon2id or X25519 as documented. Never invent your own crypto or weaken parameters.

## Working agreement

- Build the phase or item you were asked to build. Do not start later roadmap phases.
- Roadmap entries are checkboxes. Tick them only when the feature is implemented, tested, and documented.
- Update `docs/roadmap.md` and the relevant doc (or README) for user-visible behavior, packaging, or configuration changes.
- Ask before changing the data model, sync format, packaging pipeline, or any security default. Those changes need explicit approval.
- Never commit secrets, keys, or user data. Use placeholders in examples.
- Do not run destructive commands (`git reset --hard`, force push, mass deletion) without explicit confirmation. The user reviews and merges.

## Definition of done

- [ ] The requested scope is implemented, with no dead controls or placeholder behavior presented as final
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] Relevant `cargo build` and `cargo test` commands pass
- [ ] Security and privacy invariants above still hold
- [ ] `docs/roadmap.md` and affected docs are updated
- [ ] For UI work, the `antislop` Delivery Gate report is included
- [ ] Summary lists what changed, what was skipped, and any open questions
