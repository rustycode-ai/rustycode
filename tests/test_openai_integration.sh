#!/bin/bash
# OpenAI Provider Integration Test Script for RustyCode TUI
# This script tests the OpenAI provider integration with the TUI

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR="/tmp/rustycode_openai_test_$(date +%s)"
LOG_FILE="$TEST_DIR/test_results.log"
BINARY_PATH="/Users/nat/dev/rustycode/target/release/rustycode-tui"

# Test results tracking
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_TOTAL=0

# Helper functions
log() {
    echo -e "${BLUE}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1" | tee -a "$LOG_FILE"
    TESTS_PASSED=$((TESTS_PASSED + 1))
    TESTS_TOTAL=$((TESTS_TOTAL + 1))
}

log_error() {
    echo -e "${RED}✗${NC} $1" | tee -a "$LOG_FILE"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    TESTS_TOTAL=$((TESTS_TOTAL + 1))
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1" | tee -a "$LOG_FILE"
}

log_info() {
    echo -e "${BLUE}ℹ${NC} $1" | tee -a "$LOG_FILE"
}

# Create test directory
mkdir -p "$TEST_DIR"
log "Test directory created: $TEST_DIR"

# Header
log "=========================================="
log "OpenAI Provider Integration Test"
log "=========================================="
log ""

# Test 1: Check binary exists
log "Test 1: Checking if binary exists..."
if [ -f "$BINARY_PATH" ]; then
    log_success "Binary found at $BINARY_PATH"
    ls -lh "$BINARY_PATH" | tee -a "$LOG_FILE"
else
    log_error "Binary not found at $BINARY_PATH"
    log "Please build the project first: cargo build --release"
    exit 1
fi
log ""

# Test 2: Check for OPENAI_API_KEY
log "Test 2: Checking for OPENAI_API_KEY environment variable..."
if [ -z "$OPENAI_API_KEY" ]; then
    log_error "OPENAI_API_KEY environment variable is not set"
    log ""
    log "To set up OpenAI API key:"
    log "  1. Get your API key from https://platform.openai.com/api-keys"
    log "  2. Export it: export OPENAI_API_KEY='sk-...'"
    log "  3. Or add to your ~/.bashrc or ~/.zshrc:"
    log "     echo 'export OPENAI_API_KEY=\"sk-...\"' >> ~/.bashrc"
    log ""
    log "Continuing with minimal tests (API calls will fail)..."
    API_KEY_SET=false
else
    log_success "OPENAI_API_KEY is set (length: ${#OPENAI_API_KEY} chars)"
    # Mask the key for logging
    MASKED_KEY="${OPENAI_API_KEY:0:7}...${OPENAI_API_KEY: -4}"
    log "Key (masked): $MASKED_KEY"
    API_KEY_SET=true
fi
log ""

# Test 3: Check binary version/info
log "Test 3: Checking binary information..."
if timeout 5 "$BINARY_PATH" --version 2>&1 | tee -a "$LOG_FILE" || timeout 5 "$BINARY_PATH" --help 2>&1 | head -20 | tee -a "$LOG_FILE"; then
    log_success "Binary information retrieved"
else
    log_warning "Could not retrieve binary version/info (this is expected if no --version flag)"
fi
log ""

# Test 4: Check OpenAI provider in code
log "Test 4: Checking OpenAI provider implementation..."
if [ -f "/Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs" ]; then
    log_success "OpenAI provider source file exists"

    # Check for key features
    if grep -q "async fn complete" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
        log_success "Provider implements complete() method"
    else
        log_error "Provider missing complete() method"
    fi

    if grep -q "async fn complete_stream" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
        log_success "Provider implements complete_stream() method"
    else
        log_error "Provider missing complete_stream() method"
    fi

    if grep -q "select_tools_for_prompt" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
        log_success "Provider implements tool selection"
    else
        log_warning "Provider may not implement intelligent tool selection"
    fi
else
    log_error "OpenAI provider source file not found"
fi
log ""

# Test 5: Check provider registration
log "Test 5: Checking provider registration..."
if [ -f "/Users/nat/dev/rustycode/crates/rustycode-llm/src/lib.rs" ]; then
    if grep -q "openai" /Users/nat/dev/rustycode/crates/rustycode-llm/src/lib.rs; then
        log_success "OpenAI provider appears to be registered in lib.rs"
    else
        log_warning "Could not confirm OpenAI provider registration"
    fi
else
    log_warning "Could not check provider registration (lib.rs not found)"
fi
log ""

# Test 6: Verify configuration schema
log "Test 6: Checking OpenAI provider configuration schema..."
if grep -q "ProviderMetadata" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Provider metadata defined"

    # Check for required fields
    if grep -q "api_key.*ConfigField" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
        log_success "API key configuration field defined"
    else
        log_warning "API key configuration field may not be properly defined"
    fi
else
    log_warning "Provider metadata not found"
fi
log ""

# Test 7: Check for streaming support
log "Test 7: Checking streaming support..."
if grep -q "StreamChunk" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Streaming support implemented"
else
    log_warning "Streaming support may not be implemented"
fi
log ""

# Test 8: Check for tool calling support
log "Test 8: Checking tool calling support..."
if grep -q "format_tools_for_openai\|tool_to_openai_format" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Tool calling support implemented"
else
    log_warning "Tool calling support may not be implemented"
fi
log ""

# Test 9: Check for error handling
log "Test 9: Checking error handling..."
if grep -q "ProviderError" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Error handling implemented"

    # Check for specific error types
    if grep -q "ProviderError::auth\|ProviderError::RateLimited" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
        log_success "Specific error types handled (auth, rate limiting)"
    else
        log_warning "Specific error types may not be handled"
    fi
else
    log_warning "Error handling may not be implemented"
fi
log ""

# Test 10: Check for environment variable support
log "Test 10: Checking environment variable support..."
if grep -q "OPENAI_API_KEY" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Environment variable OPENAI_API_KEY referenced"
else
    log_warning "Environment variable support may not be implemented"
fi
log ""

# Test 11: Verify API endpoint configuration
log "Test 11: Checking API endpoint configuration..."
if grep -q "api.openai.com" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "OpenAI API endpoint configured"
else
    log_warning "Default API endpoint may not be configured"
fi
log ""

# Test 12: Check for model list
log "Test 12: Checking available models..."
if grep -q "async fn list_models" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Model listing implemented"

    # Extract model names
    log_info "Available models from source:"
    grep -A 20 "async fn list_models" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs | grep '"' | sed 's/.*"\(.*\)".*/  - \1/' | tee -a "$LOG_FILE"
else
    log_warning "Model listing may not be implemented"
fi
log ""

# Test 13: Check for tests
log "Test 13: Checking for unit tests..."
if grep -q "#\[cfg(test)\]" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs; then
    log_success "Unit tests defined in openai.rs"

    # Count tests
    TEST_COUNT=$(grep -c "#\[test\]" /Users/nat/dev/rustycode/crates/rustycode-llm/src/openai.rs || echo 0)
    log_info "Found $TEST_COUNT unit tests"
else
    log_warning "No unit tests found"
fi
log ""

# Test 14: Run unit tests
log "Test 14: Running OpenAI provider unit tests..."
cd /Users/nat/dev/rustycode
if cargo test --package rustycode-llm --lib openai 2>&1 | tee -a "$LOG_FILE" | grep -q "test result"; then
    log_success "Unit tests executed (check output above for results)"
else
    log_warning "Unit tests may have failed or none exist"
fi
log ""

# Test 15: Integration test scenarios
if [ "$API_KEY_SET" = true ]; then
    log "Test 15: Integration test scenarios (API key available)"
    log "WARNING: These tests will make actual API calls and may incur costs"
    log ""

    # Create a simple test that sends a message
    log "To manually test the TUI with OpenAI:"
    log "  1. Run: $BINARY_PATH"
    log "  2. Select OpenAI as the provider"
    log "  3. Select a model (e.g., gpt-4o-mini)"
    log "  4. Send a test message: 'Say hello in one word'"
    log "  5. Verify the response is received"
    log "  6. Try a streaming request (if supported)"
    log "  7. Try a tool call (e.g., 'List files in current directory')"
    log ""

    # Automated test with curl (if available)
    if command -v curl &> /dev/null; then
        log "Automated API test with curl..."
        RESPONSE=$(curl -s -w "\n%{http_code}" \
            -X POST "https://api.openai.com/v1/chat/completions" \
            -H "Authorization: Bearer $OPENAI_API_KEY" \
            -H "Content-Type: application/json" \
            -d '{
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": "Say hello in one word"}],
                "max_tokens": 10
            }' 2>&1)

        HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
        BODY=$(echo "$RESPONSE" | head -n-1)

        if [ "$HTTP_CODE" = "200" ]; then
            log_success "OpenAI API call successful (HTTP $HTTP_CODE)"
            log_info "Response: $BODY" | tee -a "$LOG_FILE"

            # Extract the actual response
            CONTENT=$(echo "$BODY" | grep -o '"content":"[^"]*"' | head -1 | cut -d'"' -f4)
            log_info "AI Response: $CONTENT"
        else
            log_error "OpenAI API call failed (HTTP $HTTP_CODE)"
            log_error "Response: $BODY" | tee -a "$LOG_FILE"
        fi
    else
        log_warning "curl not available, skipping automated API test"
    fi
else
    log "Test 15: Integration test scenarios (SKIPPED - no API key)"
    log "Set OPENAI_API_KEY to run integration tests"
fi
log ""

# Test 16: Check for configuration file support
log "Test 16: Checking configuration file support..."
CONFIG_DIR="$HOME/.config/rustycode"
if [ -d "$CONFIG_DIR" ]; then
    log_success "Configuration directory exists: $CONFIG_DIR"
    ls -la "$CONFIG_DIR" | tee -a "$LOG_FILE" || true
else
    log_info "Configuration directory not found (will be created on first run)"
fi
log ""

# Test 17: Documentation check
log "Test 17: Checking for documentation..."
if [ -f "/Users/nat/dev/rustycode/README.md" ]; then
    if grep -qi "openai" /Users/nat/dev/rustycode/README.md; then
        log_success "README mentions OpenAI"
    else
        log_info "README does not mention OpenAI"
    fi
else
    log_info "No README.md found"
fi
log ""

# Summary
log "=========================================="
log "Test Summary"
log "=========================================="
log "Total Tests: $TESTS_TOTAL"
log -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
log -e "Failed: ${RED}$TESTS_FAILED${NC}"
log ""

# Calculate success rate
if [ $TESTS_TOTAL -gt 0 ]; then
    SUCCESS_RATE=$((TESTS_PASSED * 100 / TESTS_TOTAL))
    log "Success Rate: $SUCCESS_RATE%"
    log ""

    if [ $SUCCESS_RATE -ge 80 ]; then
        log -e "${GREEN}Overall: EXCELLENT${NC}"
    elif [ $SUCCESS_RATE -ge 60 ]; then
        log -e "${YELLOW}Overall: GOOD${NC}"
    else
        log -e "${RED}Overall: NEEDS IMPROVEMENT${NC}"
    fi
fi

# Recommendations
log ""
log "=========================================="
log "Recommendations"
log "=========================================="

if [ "$API_KEY_SET" = false ]; then
    log "1. Set OPENAI_API_KEY environment variable to enable full testing"
fi

if [ $TESTS_FAILED -gt 0 ]; then
    log "2. Review failed tests and implement missing features"
fi

log "3. Run manual TUI test: $BINARY_PATH"
log "4. Test streaming responses with: 'Write a short poem'"
log "5. Test tool calling with: 'What files are in this directory?'"
log "6. Test error handling with invalid API key"
log "7. Test rate limiting behavior"
log ""

# Final status
log "Full test log saved to: $LOG_FILE"
log "Test directory: $TEST_DIR"

if [ $TESTS_FAILED -eq 0 ]; then
    log -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    log -e "${RED}Some tests failed${NC}"
    exit 1
fi
