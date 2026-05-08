#!/usr/bin/env bash
# Generate release notes for RustyCode using git-cliff
#
# Usage:
#   ./scripts/release-notes.sh              # Preview unreleased notes
#   ./scripts/release-notes.sh v0.2.0       # Preview notes for a specific version
#   ./scripts/release-notes.sh --latest     # Show notes for latest tag
#   ./scripts/release-notes.sh --write      # Write CHANGELOG.md to repo root
#   ./scripts/release-notes.sh --github     # Output body only (for GitHub Release)

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CONFIG="cliff.toml"

if [ ! -f "$CONFIG" ]; then
    echo "Error: $CONFIG not found" >&2
    exit 1
fi

case "${1:-}" in
    --write)
        ~/.cargo/bin/git-cliff --config "$CONFIG" -o CHANGELOG.md
        echo "Wrote CHANGELOG.md"
        ;;
    --github)
        # Body only — no header/footer, for pasting into GitHub Release UI
        ~/.cargo/bin/git-cliff --config "$CONFIG" --unreleased --strip all
        ;;
    --latest)
        ~/.cargo/bin/git-cliff --config "$CONFIG" --latest
        ;;
    --tag)
        shift || true
        TAG="${1:?Usage: --tag v0.2.0}"
        ~/.cargo/bin/git-cliff --config "$CONFIG" --tag "$TAG" --unreleased
        ;;
    v*)
        # Preview a specific version tag
        ~/.cargo/bin/git-cliff --config "$CONFIG" --tag "$1" --unreleased
        ;;
    *)
        # Default: preview unreleased
        ~/.cargo/bin/git-cliff --config "$CONFIG" --unreleased
        ;;
esac
