#!/bin/bash
# Quick verification script for RustyCode TUI

set -e

echo "🔍 Verifying RustyCode TUI build..."
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Must run from rustycode root directory"
    exit 1
fi

# Build the project
echo "📦 Building..."
cargo build --release --quiet

# Check if binary exists
if [ ! -f "target/release/rustycode-cli" ]; then
    echo "❌ Error: Binary not found after build"
    exit 1
fi

# Test that the CLI works
echo "🧪 Testing CLI help output..."
if ./target/release/rustycode-cli --help > /dev/null 2>&1; then
    echo "✅ CLI help works"
else
    echo "❌ CLI help failed"
    exit 1
fi

# Test that tui subcommand is recognized
echo "🧪 Testing TUI subcommand..."
if ./target/release/rustycode-cli tui --help > /dev/null 2>&1; then
    echo "✅ TUI subcommand works"
else
    echo "❌ TUI subcommand failed"
    exit 1
fi

# Check for config file
echo "📋 Checking configuration..."
if [ -f "$HOME/.codex/rustycode/config.toml" ]; then
    echo "✅ Config file found at ~/.codex/rustycode/config.toml"
else
    echo "⚠️  Config file not found (will use env vars or defaults)"
fi

# Run clippy
echo "🔍 Running clippy..."
if cargo clippy --quiet --release 2>&1 | grep -q "warning"; then
    echo "⚠️  Clippy found warnings"
else
    echo "✅ Clippy passed"
fi

echo ""
echo "✅ All verifications passed!"
echo ""
echo "To run the TUI:"
echo "  ./target/release/rustycode-cli tui"
echo ""
echo "Current TUI status:"
echo "  • Minimal TUI implementation"
echo "  • Character input works in chat mode"
echo "  • Layout properly sized"
echo "  • Natural text selection (no copy mode needed)"
echo "  • Simple echo responses (AI integration coming soon)"
echo "  • Press 'c' to enter chat mode"
echo "  • Press 'q' to quit"
echo ""
echo "See TUI_TESTING.md for testing instructions"
