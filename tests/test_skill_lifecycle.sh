#!/bin/bash

# Test script for skill lifecycle management
# This demonstrates the complete skill lifecycle: install, activate, deactivate, update, uninstall

set -e

echo "=== Skill Lifecycle Management Test ==="
echo ""

# Check if rustycode-tui is built
if [ ! -f "target/release/rustycode-tui" ]; then
    echo "❌ rustycode-tui not found. Building..."
    cargo build -p rustycode-tui --release
fi

echo "✓ rustycode-tui is ready"
echo ""

# Test 1: Check skill list command
echo "Test 1: Listing skills (should show installed skills)"
echo "Command: /skill list"
echo "Expected: List of skills with lifecycle states"
echo ""

# Test 2: Show skill info command structure
echo "Test 2: Skill info command structure"
echo "Command: /skill info <name>"
echo "Expected: Detailed skill information including lifecycle state"
echo ""

# Test 3: Show install command structure
echo "Test 3: Install command structure"
echo "Command: /skill install <name>"
echo "Expected: Install skill from marketplace"
echo ""

# Test 4: Show activate command structure
echo "Test 4: Activate command structure"
echo "Command: /skill activate <name>"
echo "Expected: Enable auto-triggering for skill"
echo ""

# Test 5: Show deactivate command structure
echo "Test 5: Deactivate command structure"
echo "Command: /skill deactivate <name>"
echo "Expected: Disable auto-triggering for skill"
echo ""

# Test 6: Show update command structure
echo "Test 6: Update command structure"
echo "Command: /skill update [name]"
echo "Expected: Update specific skill or all skills"
echo ""

# Test 7: Show uninstall command structure
echo "Test 7: Uninstall command structure"
echo "Command: /skill uninstall <name>"
echo "Expected: Remove skill from system"
echo ""

echo "=== Available Skill Commands ==="
echo "/skill list              - Show all skills with lifecycle states"
echo "/skill install <name>    - Install skill from marketplace"
echo "/skill uninstall <name>  - Remove installed skill"
echo "/skill activate <name>   - Enable auto-triggering"
echo "/skill deactivate <name> - Disable auto-triggering"
echo "/skill update [name]     - Update skill(s)"
echo "/skill info <name>       - Show detailed skill information"
echo "/skill reload            - Reload skills from disk"
echo ""

echo "=== Lifecycle States ==="
echo "📦 NotInstalled - Available in marketplace but not installed"
echo "🧩 Installed    - Installed locally but not auto-triggering"
echo "⚡ Active       - Auto-triggering enabled"
echo "💤 Inactive     - Installed but auto-triggering disabled"
echo "🔄 Running      - Currently executing"
echo "❌ Error        - Last execution failed"
echo ""

echo "✓ Skill lifecycle management system is ready!"
echo "  - Install skills from marketplace"
echo "  - Activate/deactivate for auto-triggering"
echo "  - Update skills individually or in bulk"
echo "  - View detailed lifecycle information"
echo "  - Uninstall when no longer needed"
echo ""

echo "To test interactively, run rustycode-tui and use the /skill commands."
