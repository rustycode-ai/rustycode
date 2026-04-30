#!/bin/bash
# Verification script for the multi-line input handler module

echo "=========================================="
echo "Multi-Line Input Handler Verification"
echo "=========================================="
echo ""

# Check file exists
echo "1. Checking if input.rs exists..."
if [ -f "crates/rustycode-tui/src/ui/input.rs" ]; then
    echo "   ✓ input.rs found"
    echo "   Lines: $(wc -l < crates/rustycode-tui/src/ui/input.rs)"
else
    echo "   ✗ input.rs not found"
    exit 1
fi

# Check exports in mod.rs
echo ""
echo "2. Checking if module is exported..."
if grep -q "pub use input" crates/rustycode-tui/src/ui/mod.rs; then
    echo "   ✓ Module exported in ui/mod.rs"
else
    echo "   ✗ Module not exported"
    exit 1
fi

# Check dependencies
echo ""
echo "3. Checking dependencies..."
if grep -q "ulid" crates/rustycode-tui/Cargo.toml; then
    echo "   ✓ ulid dependency added"
else
    echo "   ✗ ulid dependency missing"
    exit 1
fi

if grep -q "arboard" crates/rustycode-tui/Cargo.toml; then
    echo "   ✓ arboard dependency present"
else
    echo "   ✗ arboard dependency missing"
    exit 1
fi

if grep -q "image" crates/rustycode-tui/Cargo.toml; then
    echo "   ✓ image dependency present"
else
    echo "   ✗ image dependency missing"
    exit 1
fi

# Check for key components in source
echo ""
echo "4. Checking key components..."

if grep -q "pub enum InputMode" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ InputMode enum defined"
else
    echo "   ✗ InputMode enum missing"
    exit 1
fi

if grep -q "pub struct InputState" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ InputState struct defined"
else
    echo "   ✗ InputState struct missing"
    exit 1
fi

if grep -q "pub struct InputHandler" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ InputHandler struct defined"
else
    echo "   ✗ InputHandler struct missing"
    exit 1
fi

if grep -q "pub struct PasteHandler" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ PasteHandler struct defined"
else
    echo "   ✗ PasteHandler struct missing"
    exit 1
fi

if grep -q "pub enum InputAction" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ InputAction enum defined"
else
    echo "   ✗ InputAction enum missing"
    exit 1
fi

# Check for key methods
echo ""
echo "5. Checking key methods..."

if grep -q "pub fn handle_key_event" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ handle_key_event method defined"
else
    echo "   ✗ handle_key_event method missing"
    exit 1
fi

if grep -q "pub fn insert_newline" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ insert_newline method defined"
else
    echo "   ✗ insert_newline method missing"
    exit 1
fi

if grep -q "pub fn generate_image_preview" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ generate_image_preview function defined"
else
    echo "   ✗ generate_image_preview function missing"
    exit 1
fi

# Check for tests
echo ""
echo "6. Checking test coverage..."

test_count=$(grep -c "#\[test\]" crates/rustycode-tui/src/ui/input.rs || echo "0")
if [ "$test_count" -gt 0 ]; then
    echo "   ✓ Found $test_count unit tests"
else
    echo "   ✗ No tests found"
    exit 1
fi

# Check for specific test categories
if grep -q "test_multiline" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Multi-line tests present"
else
    echo "   ✗ Multi-line tests missing"
    exit 1
fi

if grep -q "test_handler" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Handler tests present"
else
    echo "   ✗ Handler tests missing"
    exit 1
fi

if grep -q "test_insert" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Input manipulation tests present"
else
    echo "   ✗ Input manipulation tests missing"
    exit 1
fi

# Check for key features
echo ""
echo "7. Checking key features..."

if grep -q "Option+Enter" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Option+Enter handling documented"
else
    echo "   ✗ Option+Enter handling missing"
    exit 1
fi

if grep -q "clipboard" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Clipboard integration present"
else
    echo "   ✗ Clipboard integration missing"
    exit 1
fi

if grep -q "image" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ Image handling present"
else
    echo "   ✗ Image handling missing"
    exit 1
fi

if grep -q "ASCII" crates/rustycode-tui/src/ui/input.rs; then
    echo "   ✓ ASCII preview generation present"
else
    echo "   ✗ ASCII preview generation missing"
    exit 1
fi

# Summary
echo ""
echo "=========================================="
echo "✓ All verification checks passed!"
echo "=========================================="
echo ""
echo "Module Statistics:"
echo "  - Total lines: $(wc -l < crates/rustycode-tui/src/ui/input.rs)"
echo "  - Unit tests: $test_count"
echo "  - Public structs: $(grep -c "pub struct" crates/rustycode-tui/src/ui/input.rs)"
echo "  - Public enums: $(grep -c "pub enum" crates/rustycode-tui/src/ui/input.rs)"
echo "  - Public functions: $(grep -c "pub fn" crates/rustycode-tui/src/ui/input.rs)"
echo ""
echo "Next steps:"
echo "  1. Review INPUT_MODULE_SUMMARY.md for usage guide"
echo "  2. Check examples/input_handler_example.rs for integration patterns"
echo "  3. Run: cargo test --package rustycode-tui --lib ui::input::tests"
echo "  4. Integrate into TUI event loop"
echo ""
