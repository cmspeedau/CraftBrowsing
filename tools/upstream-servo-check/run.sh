#!/usr/bin/env bash
# Host entrypoint: build the check image and run the in-container verification.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE="craftbrowsing-servo-main-check"
mkdir -p "$HERE/out"

echo ">> building image (Servo main) ..."
docker build -t "$IMAGE" "$HERE"

echo ">> running verification (long on first build) ..."
docker run --rm \
  -v "$HERE/out:/out" \
  -v "$HERE/fixture:/work/fixture:ro" \
  -v "$HERE/check.sh:/check.sh:ro" \
  -v servo-cargo-target:/var/servo-cargo-target \
  -e OUT=/out \
  "$IMAGE" bash /check.sh

echo
echo ">> artifacts in $HERE/out"
ls -1 "$HERE/out" || true
