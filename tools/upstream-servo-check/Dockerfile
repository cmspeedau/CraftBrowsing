# Build environment to compile Servo's current `main`.
# Base = Servo's own prebuilt devcontainer image (all apt deps + rust toolchain),
# so we do not maintain a drifting dependency list. See .devcontainer/Ubuntu.Dockerfile
# in the servo/servo repo for the upstream definition.
FROM ghcr.io/servo/servo/devcontainer-ubuntu:latest

# Keep the target dir on a volume (case-insensitive host FS breaks mozjs; also faster).
ENV CARGO_TARGET_DIR=/var/servo-cargo-target \
    CC=clang \
    CXX=clang++

# xvfb provides a virtual X display for display-backed servoshell runs in a container.
# Best-effort: skip if apt is unavailable or it is already present.
RUN (command -v xvfb-run >/dev/null 2>&1) \
    || (apt-get update && apt-get install -y --no-install-recommends xvfb xauth) \
    || true

WORKDIR /work

# Clone Servo `main` (shallow). Overridable via build arg for pinned testing.
ARG SERVO_REF=main
RUN git clone --depth 1 --branch "${SERVO_REF}" https://github.com/servo/servo.git /work/servo

WORKDIR /work/servo
