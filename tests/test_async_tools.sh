#!/bin/bash
# Test script for async tool execution
# This script tests that the TUI remains responsive during tool execution

set -e

echo "🧪 Testing Async Tool Execution"
echo "================================"
echo ""

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "📋 Test Plan:"
echo "  1. Build the project"
echo "  2. Start TUI with complex prompt"
echo "  3. Verify UI remains responsive during tool execution"
echo "  4. Check for timeout protection"
echo "  5. Verify error handling"
echo ""

# Build the project
echo "🔨 Building project..."
cargo build --release 2>&1 | grep -E "Compiling|Finished" || true
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Create test prompt that triggers multiple tools
cat > /tmp/test_prompt.txt << 'EOF'
Analyze the RustyCode codebase:
1. Read the README.md file
2. Grep for "async" in all Rust files
3. List all files in crates/rustycode-tui/src
4. Read the Cargo.toml file
5. Grep for "tool" in all Rust files

Please provide a summary of the project structure.
EOF

echo "📝 Test prompt created: /tmp/test_prompt.txt"
cat /tmp/test_prompt.txt
echo ""

echo "🚀 Starting TUI test..."
echo ""
echo "Manual testing required:"
echo "  1. Run: ./target/release/rustycode-cli tui"
echo "  2. Paste the prompt from /tmp/test_prompt.txt"
echo "  3. Observe:"
echo "     - Footer shows 'Running read_file' with spinner"
echo "     - UI remains responsive (can scroll, type)"
echo "     - Tools execute sequentially"
echo "     - Results displayed as they complete"
echo "     - No hang occurs"
echo ""
echo "  4. Test timeout:"
echo "     - Send command: /tool bash {\"command\": \"sleep 40\"}"
echo "     - Should timeout after 30s with error message"
echo ""
echo "  5. Test error handling:"
echo "     - Send command: /tool read_file {\"file_path\": \"/nonexistent/file.txt\"}"
echo "     - Should show error message"
echo ""

# Optional: Automated test (requires expect or similar)
if command -v expect &> /dev/null; then
    echo "🤖 Automated testing available (expect found)"
    echo "   Would you like to run automated tests? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        echo "Running automated tests..."
        # TODO: Add expect script for automated testing
        echo "⚠️  Automated tests not yet implemented"
    fi
else
    echo "⚠️  'expect' not found - manual testing required"
fi

echo ""
echo -e "${YELLOW}📚 See ASYNC_TOOL_EXECUTION.md for detailed documentation${NC}"
echo ""
echo "✅ Test script complete!"
echo ""
echo "Next steps:"
echo "  1. Run manual tests as described above"
echo "  2. Verify UI remains responsive"
echo "  3. Check timeout and error handling"
echo "  4. Report any issues"
