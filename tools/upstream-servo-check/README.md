#tools/upstream-servo-check
Builds Servo's current `main` in an isolated container and tests the exact URLs
that fail on our pinned Servo 0.8.0. Purpose: decide whether our engine bugs
(HTTP/2 navigation `PROTOCOL_ERROR`; `IntersectionObserver` not exposed) are
already fixed upstream, and capture authoritative evidence for upstream issues.

This is a development/verification tool. It does not ship and does not touch the
CraftBrowsing build graph.

## Why a container

Servo only publishes tagged binaries (we already run v0.8.0); there is no nightly
artifact, so testing `main` requires a full build (mozjs/SpiderMonkey + stylo),
which is ~30-90 min and needs many system deps. We use Servo's OWN prebuilt
devcontainer image (`ghcr.io/servo/servo/devcontainer-ubuntu:latest`) so we do not
hand-maintain an apt list that drifts from upstream.

## Run

    ./tools/upstream-servo-check/run.sh

- First run pulls the base image and builds Servo `main` (long). Subsequent runs
  reuse the named volume `servo-cargo-target`.
- Outputs land in `./out/`: `mach-help.txt`, `build.log`, `youtube.log`,
  `abc.log`, plus screenshots if `--screenshot` is available.
- Network: container uses the default bridge; Google/ABC are fetched from inside.

## What it proves

1. `mach-help.txt` -> the authoritative servoshell CLI (screenshot/exit flags) so
   the script is never guessing.
2. `youtube.log` -> whether the HTTP/2 `Reset(PROTOCOL_ERROR)` still occurs on
   `main` for `www.youtube.com`.
3. `abc.log` -> whether `IntersectionObserver` is now exposed on the window
   (the page-console `ReferenceError` gone, plus the fixture result).

## Notes / assumptions

- Uses `xvfb-run` for the display-backed runs so headless CI/container has an X
  server. If the image lacks it, install is handled at the base layer; the script
  falls back to `--headless` screenshot attempts and reports what it found.
- A GPU may be absent in a plain container; Servo falls back to software GL. If a
  run dies on GL init, read `build.log`/per-site logs and add `--disable-gpu`
  style fallbacks; the `mach-help.txt` dump lists real option names.
- To validate the hypothesis before this heavy build, the cheaper evidence already
  gathered in `docs/webcompat.md` (open upstream issues #35767 IO, #49396 youtube)
  strongly suggests `main` is NOT a clean fix.
