#!/usr/bin/env bash
# Build rustycode release binaries for all platforms.
#
# Usage:
#   ./scripts/build-release.sh              # Build Linux amd64 (default, for TB 2.0)
#   ./scripts/build-release.sh linux-amd64  # Cross-compile via cargo-zigbuild
#   ./scripts/build-release.sh linux-arm64  # Build native arm64 via Docker
#   ./scripts/build-release.sh macos-arm64  # Build native macOS
#
# Outputs:
#   target/dist/rustycode-linux-amd64
#   target/dist/rustycode-linux-arm64
#   target/dist/rustycode-macos-arm64
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/target/dist"

PLATFORM="${1:-linux-amd64}"

mkdir -p "$DIST_DIR"

case "$PLATFORM" in
    linux-amd64)
        echo "=== Building linux/amd64 (static musl) via cargo-zigbuild ==="
        # TB 2.0 prebuilt images are all amd64 (x86_64).
        # Uses musl for static linking — no dynamic linker dependency.
        # This ensures the binary runs in ANY container, even fresh ones
        # that lack /lib64/ld-linux-x86-64.so.2.
        # Requires: rustup target add x86_64-unknown-linux-musl --toolchain nightly
        # Requires: zig, cargo-zigbuild installed
        export RUSTC="$(rustup which rustc --toolchain nightly 2>/dev/null || echo '')"
        if [ -z "$RUSTC" ]; then
            echo "Error: nightly toolchain not installed. Run: rustup install nightly"
            exit 1
        fi
        ~/.cargo/bin/cargo-zigbuild zigbuild \
            --release -p rustycode-cli --no-default-features \
            --target x86_64-unknown-linux-musl
        cp "$PROJECT_ROOT/target/x86_64-unknown-linux-musl/release/rustycode-cli" \
           "$DIST_DIR/rustycode-linux-amd64"
        echo "  -> $DIST_DIR/rustycode-linux-amd64"
        ;;
    linux-arm64)
        echo "=== Building linux/arm64 via Docker (rust:1.88 + nightly = Debian bookworm, glibc 2.36) ==="
        # Uses rust:1.88 base (Debian bookworm = glibc 2.36) + installs nightly toolchain
        # --no-default-features excludes vector-memory (ONNX Runtime).
        docker run --rm \
            -v "$PROJECT_ROOT":/src \
            -w /src \
            rust:1.88 \
            sh -c "rustup install nightly 2>&1 | tail -1 && rustup run nightly cargo build --release -p rustycode-cli --no-default-features"
        cp "$PROJECT_ROOT/target/release/rustycode-cli" "$DIST_DIR/rustycode-linux-arm64"
        echo "  -> $DIST_DIR/rustycode-linux-arm64"
        ;;
    macos-arm64)
        echo "=== Building macOS arm64 (native) ==="
        cargo build --release -p rustycode-cli
        cp "$PROJECT_ROOT/target/release/rustycode-cli" "$DIST_DIR/rustycode-macos-arm64"
        echo "  -> $DIST_DIR/rustycode-macos-arm64"
        ;;
    *)
        echo "Unknown platform: $PLATFORM"
        echo "Supported: linux-amd64, linux-arm64, macos-arm64"
        exit 1
        ;;
esac

ls -lh "$DIST_DIR/"
echo ""
echo "=== Build complete ==="
