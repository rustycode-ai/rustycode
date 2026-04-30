#!/bin/bash
# OpenRouter Test Runner
# Quick setup validation and testing script

set -e

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   OpenRouter Setup & Test Runner                          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check 1: API Key
echo -e "${BLUE}Checking API key...${NC}"
if [ -z "$OPENROUTER_API_KEY" ]; then
    echo -e "${RED}✗ OPENROUTER_API_KEY not set${NC}"
    echo ""
    echo "To set up your API key:"
    echo "  1. Visit https://openrouter.ai/keys"
    echo "  2. Sign up for free account"
    echo "  3. Generate an API key"
    echo "  4. Run: export OPENROUTER_API_KEY=sk-or-your-key"
    echo ""
    exit 1
fi

if [[ ! "$OPENROUTER_API_KEY" =~ ^sk-or- ]]; then
    echo -e "${RED}✗ Invalid API key format (must start with 'sk-or-')${NC}"
    exit 1
fi

KEY_LENGTH=${#OPENROUTER_API_KEY}
echo -e "${GREEN}✓ API key found${NC} (length: $KEY_LENGTH chars)"
echo ""

# Check 2: Cargo
echo -e "${BLUE}Checking Rust toolchain...${NC}"
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}✗ Cargo not found${NC}"
    echo "Install Rust from https://rustup.rs/"
    exit 1
fi
echo -e "${GREEN}✓ Cargo installed${NC}"
echo ""

# Check 3: Project structure
echo -e "${BLUE}Checking project structure...${NC}"
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}✗ Not in rustycode project root${NC}"
    echo "Run this script from /Users/nat/dev/rustycode"
    exit 1
fi
echo -e "${GREEN}✓ Project structure OK${NC}"
echo ""

# Choose test type
echo "Choose test type:"
echo "  1) Quick test (all free models, ~30 seconds)"
echo "  2) Comprehensive test (performance benchmarks, ~2 minutes)"
echo "  3) TUI integration test (interactive)"
echo ""
read -p "Enter choice [1-3]: " choice

case $choice in
    1)
        echo ""
        echo -e "${BLUE}Running quick test...${NC}"
        echo ""
        cargo run --example test_openrouter
        ;;
    2)
        echo ""
        echo -e "${BLUE}Running comprehensive test suite...${NC}"
        echo ""
        cargo run --example test_openrouter_comprehensive
        ;;
    3)
        echo ""
        echo -e "${BLUE}Launching TUI...${NC}"
        echo ""
        echo "In the TUI:"
        echo "  - Press Ctrl+M to open model selector"
        echo "  - Select 'OpenRouter (Free Models)'"
        echo "  - Choose a free model"
        echo "  - Start chatting!"
        echo ""
        read -p "Press Enter to launch TUI..."
        cargo run -p rustycode-tui
        ;;
    *)
        echo -e "${RED}Invalid choice${NC}"
        exit 1
        ;;
esac

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Test Complete                                            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo -e "${GREEN}✓ All checks passed!${NC}"
echo ""
echo "Next steps:"
echo "  • Use OpenRouter in TUI: cargo run -p rustycode-tui"
echo "  • Compare models: Press Ctrl+M in TUI"
echo "  • View docs: cat OPENROUTER_TEST_GUIDE.md"
echo ""
