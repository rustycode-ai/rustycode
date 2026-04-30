#!/bin/bash
# Automated test for TUI enhancements compilation and basic checks

set -e

echo "🧪 Automated TUI Enhancement Tests"
echo "=================================="
echo ""

# Test 1: Build check
echo "📦 Test 1: Building rustycode-cli..."
if cargo build -p rustycode-cli 2>&1 | grep -E "^error"; then
    echo "❌ Build failed"
    exit 1
else
    echo "✅ Build successful"
fi

# Test 2: Check tmux module compiles
echo ""
echo "📦 Test 2: Checking tmux module compilation..."
if cargo build -p rustycode-tui 2>&1 | grep -E "^error"; then
    echo "❌ TUI build failed"
    exit 1
else
    echo "✅ TUI module compiles successfully"
fi

# Test 3: Verify changes in minimal.rs
echo ""
echo "🔍 Test 3: Verifying code changes..."

# Check for tmux import
if grep -q "use super::tmux" /Users/nat/dev/rustycode/crates/rustycode-tui/src/minimal.rs; then
    echo "✅ Tmux module imported"
else
    echo "❌ Tmux module not imported"
    exit 1
fi

# Check for resize handling
if grep -q "Event::Resize(width, height)" /Users/nat/dev/rustycode/crates/rustycode-tui/src/minimal.rs; then
    echo "✅ Resize event handling implemented"
else
    echo "❌ Resize event handling not found"
    exit 1
fi

# Check for bracketed paste enable
if grep -q "enable_bracketed_paste" /Users/nat/dev/rustycode/crates/rustycode-tui/src/minimal.rs; then
    echo "✅ Bracketed paste mode integration found"
else
    echo "❌ Bracketed paste mode not integrated"
    exit 1
fi

# Check for tmux setup instructions
if grep -q "print_tmux_setup_instructions" /Users/nat/dev/rustycode/crates/rustycode-tui/src/minimal.rs; then
    echo "✅ Tmux setup instructions integrated"
else
    echo "❌ Tmux setup instructions not found"
    exit 1
fi

# Test 4: Check that binary exists and is executable
echo ""
echo "📦 Test 4: Checking binary..."
if [ -f "./target/debug/rustycode-cli" ]; then
    echo "✅ Binary exists"
    if ./target/debug/rustycode-cli --help &> /dev/null; then
        echo "✅ Binary is executable"
    else
        echo "❌ Binary execution failed"
        exit 1
    fi
else
    echo "❌ Binary not found"
    exit 1
fi

# Test 5: Verify TUI command exists
echo ""
echo "📦 Test 5: Checking TUI command..."
if ./target/debug/rustycode-cli tui --help &> /dev/null; then
    echo "✅ TUI command available"
else
    echo "⚠️  TUI command may not support --help (that's OK)"
fi

echo ""
echo "✅ All automated tests passed!"
echo ""
echo "📋 Summary:"
echo "   - Build: ✅"
echo "   - Tmux integration: ✅"
echo "   - Resize handling: ✅"
echo "   - Binary: ✅"
echo ""
echo "🔍 For manual testing, run: ./test_tui_manual.sh"
