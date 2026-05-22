#!/usr/bin/env sh
# RustyCode installer — downloads the latest binary from GitHub Releases.
# Usage: curl -fsSL https://rustycode-ai.github.io/install.sh | sh
#   or:  curl -fsSL https://rustycode-ai.github.io/install.sh | sh -s -- --nightly

set -eu

REPO="rustycode-ai/rustycode"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="rustycode"
NIGHTLY=0

for arg in "$@"; do
  case "$arg" in
    --nightly) NIGHTLY=1 ;;
    --dir=*) INSTALL_DIR="${arg#--dir=}" ;;
    --help|-h)
      echo "Usage: curl -fsSL ... | sh -s -- [--nightly] [--dir=/path]"
      exit 0
      ;;
  esac
done

# --- Detect platform ---
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) PLATFORM="macos" ;;
  Linux)  PLATFORM="linux" ;;
  *)
    echo "Error: unsupported OS '$OS'"
    exit 1
    ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH_TAG="arm64" ;;
  x86_64|amd64)  ARCH_TAG="x64" ;;
  *)
    echo "Error: unsupported architecture '$ARCH'"
    exit 1
    ;;
esac

github_api() {
  curl -fsSL -H "User-Agent: RustyCode-Installer" "$1"
}

# --- Resolve version ---
if [ "$NIGHTLY" = 1 ]; then
  TAG=$(github_api "https://api.github.com/repos/${REPO}/releases?per_page=100" \
    | grep -m 1 '"tag_name": "nightly-' \
    | sed -E 's/.*"tag_name": "([^"]+)".*/\1/')
else
  TAG=$(github_api "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | head -1 \
    | sed -E 's/.*"([^"]+)".*/\1/')
fi

if [ -z "$TAG" ]; then
  echo "Error: could not determine release version"
  exit 1
fi

FILENAME="rustycode-${PLATFORM}-${ARCH_TAG}.tar.gz"
if [ "$PLATFORM" = "linux" ]; then
  BINARY="${BINARY_NAME}-${PLATFORM}-${ARCH_TAG}"
else
  BINARY="${BINARY_NAME}-${PLATFORM}-${ARCH_TAG}"
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/${FILENAME}"

# --- Download ---
echo "Installing RustyCode ${TAG} for ${PLATFORM}-${ARCH_TAG}..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
curl -fsSL "$URL" -o "${TMPDIR}/${FILENAME}"

# --- Extract ---
tar xzf "${TMPDIR}/${FILENAME}" -C "$TMPDIR"

# Find the binary in the extracted content
BINARY_PATH=$(find "$TMPDIR" -type f -name "rustycode*" -perm -u+x | head -1)
if [ -z "$BINARY_PATH" ]; then
  # Try without execute permission (might need chmod)
  BINARY_PATH=$(find "$TMPDIR" -type f \( -name "rustycode" -o -name "rustycode-*" \) | head -1)
fi

if [ -z "$BINARY_PATH" ]; then
  echo "Error: could not find rustycode binary in archive"
  exit 1
fi

chmod +x "$BINARY_PATH"

# --- Install ---
if [ -w "$INSTALL_DIR" ]; then
  cp "$BINARY_PATH" "${INSTALL_DIR}/${BINARY_NAME}"
else
  echo "Installing to ${INSTALL_DIR} (requires sudo)..."
  sudo cp "$BINARY_PATH" "${INSTALL_DIR}/${BINARY_NAME}"
fi

# --- Verify ---
INSTALLED="${INSTALL_DIR}/${BINARY_NAME}"
VERSION=$("$INSTALLED" --version 2>/dev/null || echo "installed")
echo "RustyCode installed: ${INSTALLED} (${VERSION})"
if ! command -v "$BINARY_NAME" >/dev/null 2>&1; then
  echo "Add ${INSTALL_DIR} to your PATH if it's not already there."
fi
