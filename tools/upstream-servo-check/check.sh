#!/usr/bin/env bash
# Runs INSIDE the container. Compiles Servo main and captures evidence for the
# two open webcompat gaps. Output dir is /out (host: tools/upstream-servo-check/out).
set -uo pipefail

OUT="${OUT:-/out}"
mkdir -p "$OUT"
cd /work/servo

echo "== 1. bootstrap =="
./mach bootstrap --yes || { echo "bootstrap failed"; exit 2; }

echo "== 2. build (long: SpiderMonkey + stylo) =="
./mach build --release 2>&1 | tee "$OUT/build.log"

echo "== 3. authoritative CLI (--help) =="
# Dump real flags so we never guess option names.
./mach run --help > "$OUT/mach-help.txt" 2>&1 || \
  "$(./mach find-path servoshell 2>/dev/null || echo target/release/servoshell)" --help > "$OUT/mach-help.txt" 2>&1
grep -iE "screenshot|headless|exit|window-size|pref|user-agent|disable-gpu" "$OUT/mach-help.txt" > "$OUT/relevant-flags.txt" 2>&1 || true

# Detect the real screenshot flag from help; fall back to common ones.
SCREENSHOT_FLAG="$(grep -oE '\-\-screenshot[=][ ]*[^ ]+' "$OUT/mach-help.txt" | head -1)"
[ -z "$SCREENSHOT_FLAG" ] && SCREENSHOT_FLAG="$(grep -oE '\-\-screenshot' "$OUT/mach-help.txt" | head -1)"
echo "screenshot flag: ${SCREENSHOT_FLAG:-<none found>}"

display_run() {
  # $@ = args to servoshell; runs under xvfb if present.
  local logfile="$1"; shift
  if command -v xvfb-run >/dev/null 2>&1; then
    xvfb-run -a ./mach run "$@" > "$logfile" 2>&1
  else
    ./mach run "$@" > "$logfile" 2>&1
  fi
}

echo "== 4. HTTP/2 probe: www.youtube.com =="
# Turn on hyper/h2 tracing to capture the exact rejected frame/header on main.
RUST_LOG="warn,h2=trace,hyper=trace,net=warn" \
  timeout 120 ./mach run --pref=network.http2-enabled:true "https://www.youtube.com/" \
  > "$OUT/youtube.log" 2>&1
grep -iE "PROTOCOL_ERROR|SendRequest|Http2|Reset\(|GOAWAY" "$OUT/youtube.log" \
  > "$OUT/youtube-h2.txt" 2>&1 || true
echo "  h2 hits: $(wc -l < "$OUT/youtube-h2.txt" 2>/dev/null || echo 0)"

echo "== 5. IntersectionObserver probe =="
# Load the minimized fixture (mounted at /work/fixture) and capture console.
display_run "$OUT/io.log" --enable-experimental-web-platform-features \
  "file:///work/fixture/io-repro.html"
grep -iE "IntersectionObserver|page console|ReferenceError" "$OUT/io.log" \
  > "$OUT/io-summary.txt" 2>&1 || true

echo "== 6. ABC (real-world IO consumer) =="
display_run "$OUT/abc.log" "https://www.abc.net.au/"
grep -iE "IntersectionObserver|client-side exception|Error" "$OUT/abc.log" \
  > "$OUT/abc-summary.txt" 2>&1 || true

echo "== done. artifacts in $OUT =="
ls -1 "$OUT"
