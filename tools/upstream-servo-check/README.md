# tools/upstream-servo-check

## Policy note (read first)

Servo's AI-usage policy (https://book.servo.org/policy/ai-usage.html) forbids
AI-authored issue/PR text, but allows AI-assisted bug *finding* only if a human
reproduces and validates the result before filing. Concretely:
- The container verification below is the human validation step: it reproduces
  the gaps against Servo `main` so findings are confirmed, not trusted from a tool.
- We have abandoned drafting upstream reports (the AI-authored issue text this
  folder once held). The validated findings live in `docs/webcompat.md` and the
  archived `out/` summaries. If anyone does file upstream later, a person writes it
  in their own words from reproduced findings, never from tool output.
- Because we are not filing, the outcome here feeds `docs/engine-decision.md`
  (switch the primary engine) rather than a pull request.

Builds Servo's current `main` in an isolated container and tests the exact URLs
that fail on our pinned Servo 0.8.0. Purpose: decide whether our engine bugs
(HTTP/2 navigation `PROTOCOL_ERROR`; `IntersectionObserver` not exposed) are
already fixed upstream, and capture authoritative evidence that feeds
`docs/webcompat.md` and `docs/engine-decision.md`.

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
