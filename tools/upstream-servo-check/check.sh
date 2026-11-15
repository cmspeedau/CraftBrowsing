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
./mach run --help > "$OUT/mach-help.txt" 2>&1 || true
grep -iE "screenshot|headless|exit|window-size|pref|user-agent|disable-gpu" "$OUT/mach-help.txt" > "$OUT/relevant-flags.txt" 2>&1 || true

# servoshell must be run --release (we built --release above; `mach run`
# otherwise defaults to --dev and fails BuildNotFound). Run HEADED under xvfb:
# the headless window stubs show_console_message, so page console (where the
# IntersectionObserver ReferenceError appears) is only emitted in headed mode.
run_headed() {
  local logfile="$1"; shift
  if command -v xvfb-run >/dev/null 2>&1; then
    timeout "${SERVO_TIMEOUT:-90}" xvfb-run -a ./mach run --release --software "$@" > "$logfile" 2>&1
  else
    echo "xvfb-run not available" > "$logfile"
  fi
}

echo "== 4. HTTP/2 probe: www.youtube.com =="
RUST_LOG="warn,h2=trace,hyper=trace,net=warn,script=info" \
  run_headed "$OUT/youtube.log" "https://www.youtube.com/"
grep -iE "PROTOCOL_ERROR|SendRequest|Http2|Reset\(|GOAWAY|Network error|Could not load" "$OUT/youtube.log" \
  > "$OUT/youtube-h2.txt" 2>&1 || true
echo "  h2 hits: $(wc -l < "$OUT/youtube-h2.txt" 2>/dev/null || echo 0)"

echo "== 5. IntersectionObserver probe (headed, so console is captured) =="
run_headed "$OUT/io.log" "file:///work/fixture/io-repro.html"
grep -iE "IntersectionObserver|DISPOSITION|THREW|not defined" "$OUT/io.log" \
  > "$OUT/io-summary.txt" 2>&1 || true

echo "== 6. ABC (real-world IO consumer) =="
run_headed "$OUT/abc.log" "https://www.abc.net.au/"
grep -iE "IntersectionObserver|client-side exception|is not defined|Application error" "$OUT/abc.log" \
  > "$OUT/abc-summary.txt" 2>&1 || true

echo "== done. artifacts in $OUT =="
ls -1 "$OUT"
