#!/bin/bash
# Test clipboard functionality for RustyCode TUI

set -e

echo "=== Clipboard Test Script ==="
echo ""

# Check if we're on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo "This script is designed for macOS. Please adapt for Linux."
    exit 1
fi

echo "1. Testing text paste..."
echo "test text from clipboard" | pbcopy
CLIPBOARD_CONTENT=$(pbpaste)
if [[ "$CLIPBOARD_CONTENT" == "test text from clipboard" ]]; then
    echo "✓ Text clipboard works"
else
    echo "✗ Text clipboard failed"
    exit 1
fi

echo ""
echo "2. Testing pngpaste (for image paste fallback)..."
if command -v pngpaste &> /dev/null; then
    echo "✓ pngpaste is installed"
else
    echo "⚠ pngpaste not found. Install with: brew install pngpaste"
    echo "  Image paste will use arboard::Clipboard instead"
fi

echo ""
echo "3. Creating test image..."
# Create a simple 100x100 red PNG using ImageMagick or sips
if command -v sips &> /dev/null; then
    # Create a test image using macOS built-in tools
    python3 -c "
from PIL import Image
img = Image.new('RGB', (100, 100), color='red')
img.save('/tmp/test_clipboard.png')
print('Created /tmp/test_clipboard.png')
" 2>/dev/null || echo "PIL not available, skipping image creation"

    if [[ -f /tmp/test_clipboard.png ]]; then
        echo "✓ Test image created"

        # Copy to clipboard using osascript
        osascript -e 'set theData to read POSIX file "/tmp/test_clipboard.png" as «class PNGf»' \
                  -e 'set the clipboard to theData' 2>/dev/null && echo "✓ Image copied to clipboard"

        # Clean up
        rm -f /tmp/test_clipboard.png
    else
        echo "⚠ Could not create test image"
    fi
else
    echo "⚠ sips not available, skipping image test"
fi

echo ""
echo "4. Checking arboard crate..."
cargo tree -p rustycode-tui | grep arboard && echo "✓ arboard crate is included" || echo "⚠ arboard not found"

echo ""
echo "=== Test Complete ==="
echo ""
echo "To manually test image paste:"
echo "1. Open an image in Preview.app"
echo "2. Edit > Copy (⌘C)"
echo "3. In RustyCode TUI, press Ctrl+V"
echo "4. Should see image thumbnail in input area"
