#!/bin/bash
# Quick build and test script for RustyCode

set -e

echo "🔨 Building RustyCode..."
cargo build --package rustycode-cli

echo ""
echo "🧪 Running tests..."
cargo test --package rustycode-llm --quiet
cargo test --package rustycode-tui --quiet

echo ""
echo "✅ Build and tests passed!"
echo ""
echo "🚀 To run RustyCode:"
echo "   cd /Users/nat/dev/rustycode"
echo "   cargo run --package rustycode-cli"
echo ""
echo "📝 See TESTING_CHECKLIST.md for what to test"
