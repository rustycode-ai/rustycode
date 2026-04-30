#!/bin/bash
# Manual OpenAI TUI Test Script
# This script provides a guided manual testing experience for the OpenAI provider

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

BINARY_PATH="/Users/nat/dev/rustycode/target/release/rustycode-tui"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}OpenAI TUI Manual Test Guide${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check API key
if [ -z "$OPENAI_API_KEY" ]; then
    echo -e "${RED}ERROR: OPENAI_API_KEY not set${NC}"
    echo ""
    echo "To set up your API key:"
    echo "  1. Get key from https://platform.openai.com/api-keys"
    echo "  2. Run: export OPENAI_API_KEY='sk-your-key-here'"
    echo ""
    exit 1
fi

echo -e "${GREEN}✓ API Key is set${NC}"
echo ""

# Check binary
if [ ! -f "$BINARY_PATH" ]; then
    echo -e "${RED}ERROR: Binary not found at $BINARY_PATH${NC}"
    echo "Run: cargo build --release"
    exit 1
fi

echo -e "${GREEN}✓ Binary found${NC}"
echo ""

# Display test scenarios
echo -e "${YELLOW}Test Scenarios:${NC}"
echo ""

echo "1. ${GREEN}Basic Completion${NC}"
echo "   - Message: 'Say hello in one word'"
echo "   - Expected: Single word response like 'Hello'"
echo ""

echo "2. ${GREEN}Streaming Response${NC}"
echo "   - Message: 'Count from 1 to 10 slowly'"
echo "   - Expected: Numbers appear one by one"
echo ""

echo "3. ${GREEN}Code Generation${NC}"
echo "   - Message: 'Write a Rust function to add two numbers'"
echo "   - Expected: Rust code with function signature"
echo ""

echo "4. ${GREEN}Tool Calling - File List${NC}"
echo "   - Message: 'What files are in this directory?'"
echo "   - Expected: AI calls file listing tool and shows results"
echo ""

echo "5. ${GREEN}Tool Calling - Search${NC}"
echo "   - Message: 'Search for test in all .rs files'"
echo "   - Expected: AI calls grep/search tool"
echo ""

echo "6. ${GREEN}Multi-turn Conversation${NC}"
echo "   - Message 1: 'Remember the number 42'"
echo "   - Message 2: 'What number did I tell you to remember?'"
echo "   - Expected: AI responds with '42'"
echo ""

echo "7. ${GREEN}Error Handling - Invalid Key${NC}"
echo "   - Temporarily set: export OPENAI_API_KEY='sk-invalid'"
echo "   - Send any message"
echo "   - Expected: Clear error message"
echo "   - Restore: export OPENAI_API_KEY='your-real-key'"
echo ""

echo "8. ${GREEN}Long Response${NC}"
echo "   - Message: 'Explain what Rust is in 3 paragraphs'"
echo "   - Expected: Full response with proper formatting"
echo ""

echo "9. ${GREEN}Creative Writing${NC}"
echo "   - Message: 'Write a haiku about programming'"
echo "   - Expected: 5-7-5 syllable poem structure"
echo ""

echo "10. ${GREEN}Model Selection${NC}"
echo "   - Try different models: gpt-4o, gpt-4o-mini, gpt-3.5-turbo"
echo "   - Compare response quality and speed"
echo ""

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${YELLOW}Ready to launch TUI${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Press Enter to launch TUI (or Ctrl+C to cancel)..."
read

echo ""
echo -e "${GREEN}Launching TUI...${NC}"
echo ""
echo "TUI Tips:"
echo "  - Use Ctrl+C to exit"
echo "  - Use arrow keys for navigation"
echo "  - Use /help for available commands"
echo ""

# Launch TUI
"$BINARY_PATH"

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Test session complete!${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Please document your results:"
echo "  1. Which tests passed?"
echo "  2. Which tests failed?"
echo "  3. Any unexpected behavior?"
echo "  4. Response times for each model?"
echo ""
echo "Report your findings to improve the integration!"
echo ""
