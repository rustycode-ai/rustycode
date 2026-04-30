#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "scripts/release-phase1.sh is deprecated. Delegating to scripts/release-v2.sh."
exec "${SCRIPT_DIR}/release-v2.sh" "$@"
