#!/usr/bin/env sh
# RustyCode installer — downloads the latest release from GitHub and installs binaries.
#
# Usage:
#   curl -fsSL https://rustycode-ai.github.io/install.sh | sh
#   curl -fsSL https://rustycode-ai.github.io/install.sh | sh -s -- --nightly
#   curl -fsSL https://rustycode-ai.github.io/install.sh | sh -s -- --dir ~/bin
#   curl -fsSL https://rustycode-ai.github.io/install.sh | sh -s -- --bin rustycode-bench

set -eu

REPO="rustycode-ai/rustycode"
INSTALL_DIR="/usr/local/bin"
NIGHTLY=0
SELECTED_BIN=""

while [ $# -gt 0 ]; do
  case "$1" in
    --nightly)         NIGHTLY=1; shift ;;
    --dir)
      INSTALL_DIR="$2"; shift 2 ;;
    --dir=*)           INSTALL_DIR="${1#--dir=}"; shift ;;
    --bin)
      SELECTED_BIN="$2"; shift 2 ;;
    --bin=*)           SELECTED_BIN="${1#--bin=}"; shift ;;
    --help|-h)
      echo "Usage: curl -fsSL ... | sh -s -- [--nightly] [--dir=/path] [--bin=NAME]"
      echo ""
      echo "  --nightly      Install the latest nightly build"
      echo "  --dir /path     Install directory (default: /usr/local/bin)"
      echo "  --bin NAME      Install only a specific binary (e.g. rustycode-mcp-computer-use)"
      echo ""
      echo "Released binaries:"
      echo "  rustycode                    Main CLI and TUI"
      echo "  rustycode-mcp-computer-use   Computer-use MCP server"
      exit 0
      ;;
    *) shift ;;
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

# Extract first "tag_name":"..." value from compact JSON.
# GitHub API returns single-line JSON, so line-anchored sed won't work.
# grep -o extracts only the match; head -1 takes the first (field-level) hit,
# which comes before any "tag_name" literal that might appear in the body text.
extract_tag() {
  grep -o '"tag_name":"[^"]*"' | head -1 | sed 's/"tag_name":"//;s/"$//'
}

# --- Resolve version ---
if [ "$NIGHTLY" = 1 ]; then
  TAG=$(github_api "https://api.github.com/repos/${REPO}/releases?per_page=100" \
    | extract_tag \
    | grep '^nightly-' \
    | head -1)
else
  TAG=$(github_api "https://api.github.com/repos/${REPO}/releases/latest" \
    | extract_tag)
fi

if [ -z "$TAG" ]; then
  echo "Error: could not determine release version"
  exit 1
fi

FILENAME="rustycode-${PLATFORM}-${ARCH_TAG}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${FILENAME}"

# --- Download ---
echo "Installing RustyCode ${TAG} for ${PLATFORM}-${ARCH_TAG}..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
curl -fsSL "$URL" -o "${TMPDIR}/${FILENAME}"

# --- Extract ---
mkdir -p "${TMPDIR}/archive"
tar xzf "${TMPDIR}/${FILENAME}" -C "${TMPDIR}/archive"

# Tarball layout varies by release:
#   Newer (build-release.sh): rustycode-${PLATFORM}-${ARCH_TAG}/<binaries>
#   Older: flat binaries at archive root
EXTRACTED_DIR="${TMPDIR}/archive/rustycode-${PLATFORM}-${ARCH_TAG}"
if [ ! -d "$EXTRACTED_DIR" ]; then
  # Flat archive — binaries are directly in archive subdirectory
  EXTRACTED_DIR="${TMPDIR}/archive"
fi

# Verify at least one binary exists
HAS_BIN=0
for f in "$EXTRACTED_DIR"/*; do
  [ -f "$f" ] && HAS_BIN=1 && break
done
if [ "$HAS_BIN" = 0 ]; then
  echo "Error: no binaries found in archive"
  exit 1
fi

# --- Install binaries ---
NEEDS_SUDO=""
if [ ! -w "$INSTALL_DIR" ]; then
  NEEDS_SUDO=1
  echo "Installing to ${INSTALL_DIR} (requires sudo)..."
fi

COUNT=0
for bin in "$EXTRACTED_DIR"/*; do
  [ -f "$bin" ] || continue
  name=$(basename "$bin")

  # If --bin was specified, skip non-matching
  if [ -n "$SELECTED_BIN" ] && [ "$name" != "$SELECTED_BIN" ]; then
    continue
  fi

  chmod +x "$bin"
  if [ -n "$NEEDS_SUDO" ]; then
    sudo cp "$bin" "${INSTALL_DIR}/${name}"
  else
    cp "$bin" "${INSTALL_DIR}/${name}"
  fi
  COUNT=$((COUNT + 1))
done

if [ "$COUNT" -eq 0 ]; then
  echo "Error: no binaries found in archive"
  exit 1
fi

# --- Verify ---
echo "Installed ${COUNT} binary(ies) to ${INSTALL_DIR}:"
for bin in "$INSTALL_DIR"/rustycode*; do
  [ -f "$bin" ] || continue
  name=$(basename "$bin")
  ver=$("$bin" --version 2>/dev/null || echo "?")
  echo "  ${name}  ${ver}"
done

if ! command -v rustycode >/dev/null 2>&1; then
  echo ""
  echo "Add ${INSTALL_DIR} to your PATH if it's not already there."
fi
