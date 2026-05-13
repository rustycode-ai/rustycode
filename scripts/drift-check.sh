#!/bin/bash
# Structural Drift Check - Part of the Unified Symbol Engine
# This script identifies breaking API changes by comparing the current code structure
# against a cached baseline (or git HEAD).

set -e

# Path to the rustycode CLI (assume it's in the path or build it)
CLI="cargo run -p rustycode-cli --quiet --"

echo "🔍 Checking for structural drift..."

# We use the internal 'check-drift' tool logic exposed via the CLI
# (In a real implementation, we would have a 'rustycode check-drift' command)
$CLI tools call check_drift "{\"path\": \"$1\"}"

if [ $? -eq 0 ]; then
    echo "✅ No structural drift detected."
else
    echo "⚠️ Structural drift detected! Please review public API changes."
    exit 1
fi
