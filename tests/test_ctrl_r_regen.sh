#!/bin/bash
# Comprehensive test suite for Ctrl+R regenerate response functionality
# Tests the regenerate_last_response() implementation in event_loop.rs

set -e

TUI_BIN="/Users/nat/dev/rustycode/target/release/rustycode-tui"
SESSION_NAME="tui-regen-test"
TEST_LOG="/tmp/tui_regen_test.log"
PASSED=0
FAILED=0

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
log_test() {
    echo -e "${YELLOW}TEST:${NC} $1" | tee -a "$TEST_LOG"
}

log_pass() {
    echo -e "${GREEN}✓ PASS:${NC} $1" | tee -a "$TEST_LOG"
    ((PASSED++))
}

log_fail() {
    echo -e "${RED}✗ FAIL:${NC} $1" | tee -a "$TEST_LOG"
    ((FAILED++))
}

cleanup() {
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    killall rustycode-tui 2>/dev/null || true
    sleep 0.5
}

wait_for_response() {
    local timeout=$1
    local elapsed=0
    while [ $elapsed -lt $timeout ]; do
        local output=$(tmux capture-pane -t "$SESSION_NAME" -p)
        if echo "$output" | grep -q "Assistant\|▏"; then
            return 0
        fi
        sleep 0.5
        ((elapsed+=1))
    done
    return 1
}

get_message_count() {
    local output=$(tmux capture-pane -t "$SESSION_NAME" -p)
    echo "$output" | grep -c "^.*Assistant.*$" || echo "0"
}

capture_output() {
    tmux capture-pane -t "$SESSION_NAME" -p | tee -a "$TEST_LOG"
}

# Initial setup
echo "======================================"
echo "Ctrl+R Regenerate Response Test Suite"
echo "======================================"
echo "" > "$TEST_LOG"

# Cleanup any previous sessions
cleanup

# Test 1: Basic regeneration - single message exchange
log_test "Test 1: Basic regeneration (send message, receive response, Ctrl+R)"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

# Send a simple message
tmux send-keys -t "$SESSION_NAME" "hello" Enter
sleep 3

# Get message count before regeneration
MSG_COUNT_BEFORE=$(get_message_count)
log_test "  Message count before Ctrl+R: $MSG_COUNT_BEFORE"

# Press Ctrl+R to regenerate
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 3

# Check for regeneration message
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "Regenerating response"; then
    log_pass "  'Regenerating response...' message displayed"
else
    log_fail "  'Regenerating response...' message NOT displayed"
fi

# Wait for new response
if wait_for_response 10; then
    log_pass "  New response generated after regeneration"
else
    log_fail "  Timeout waiting for new response"
fi

MSG_COUNT_AFTER=$(get_message_count)
log_test "  Message count after Ctrl+R: $MSG_COUNT_AFTER"

# Verify message count remains the same (old message replaced)
if [ "$MSG_COUNT_BEFORE" -eq "$MSG_COUNT_AFTER" ]; then
    log_pass "  Old message properly removed and replaced"
else
    log_fail "  Message count changed unexpectedly (was $MSG_COUNT_BEFORE, now $MSG_COUNT_AFTER)"
fi

cleanup
sleep 1

# Test 2: Edge case - Ctrl+R with no messages
log_test "Test 2: Edge case - Ctrl+R with no messages in history"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

# Press Ctrl+R immediately
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 1

OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "No AI response to regenerate"; then
    log_pass "  Correct error message: 'No AI response to regenerate'"
else
    log_fail "  Expected error message not found"
    echo "  Output:" >> "$TEST_LOG"
    echo "$OUTPUT" >> "$TEST_LOG"
fi

cleanup
sleep 1

# Test 3: Edge case - Ctrl+R during streaming
log_test "Test 3: Edge case - Ctrl+R during active streaming"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

# Send a longer message that will take time to stream
tmux send-keys -t "$SESSION_NAME" "write a short poem about testing" Enter
sleep 0.5

# Press Ctrl+R immediately while streaming
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 2

OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "Cannot regenerate while streaming"; then
    log_pass "  Correct error message: 'Cannot regenerate while streaming'"
else
    log_fail "  Expected streaming error message not found"
fi

cleanup
sleep 1

# Test 4: Multiple sequential regenerations
log_test "Test 4: Multiple sequential regenerations (Ctrl+R 3 times)"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

# Send initial message
tmux send-keys -t "$SESSION_NAME" "what is 2+2?" Enter
sleep 3

# First regeneration
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 3
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "Regenerating response"; then
    log_pass "  First regeneration initiated"
else
    log_fail "  First regeneration failed"
fi

# Second regeneration
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 3
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "Regenerating response"; then
    log_pass "  Second regeneration initiated"
else
    log_fail "  Second regeneration failed"
fi

# Third regeneration
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 3
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "Regenerating response"; then
    log_pass "  Third regeneration initiated"
else
    log_fail "  Third regeneration failed"
fi

cleanup
sleep 1

# Test 5: Regeneration after multiple message exchanges
log_test "Test 5: Regeneration after multiple message exchanges"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

# Send multiple messages
tmux send-keys -t "$SESSION_NAME" "first message" Enter
sleep 2

tmux send-keys -t "$SESSION_NAME" "second message" Enter
sleep 2

tmux send-keys -t "$SESSION_NAME" "third message" Enter
sleep 2

# Get current message count
MSG_COUNT_BEFORE=$(get_message_count)

# Press Ctrl+R - should only regenerate the last response
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 3

MSG_COUNT_AFTER=$(get_message_count)

# Message count should be the same (only last message regenerated)
if [ "$MSG_COUNT_BEFORE" -eq "$MSG_COUNT_AFTER" ]; then
    log_pass "  Only last message regenerated, earlier messages preserved"
else
    log_fail "  Message count changed unexpectedly"
fi

# Verify earlier messages are still present
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "first message"; then
    log_pass "  Earlier messages still present in history"
else
    log_fail "  Earlier messages may have been removed"
fi

cleanup
sleep 1

# Test 6: Verify status message appearance
log_test "Test 6: Verify system message appears during regeneration"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

tmux send-keys -t "$SESSION_NAME" "test status message" Enter
sleep 3

# Clear any previous output
tmux send-keys -t "$SESSION_NAME" "C-l"
sleep 0.5

# Press Ctrl+R
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 1

# Check for system message with emoji
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "🔄.*Regenerating"; then
    log_pass "  System message with spinner emoji displayed"
else
    log_fail "  Expected system message not found"
fi

cleanup
sleep 1

# Test 7: Rapid successive Ctrl+R presses
log_test "Test 7: Rapid successive Ctrl+R presses (stress test)"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

tmux send-keys -t "$SESSION_NAME" "stress test" Enter
sleep 3

# Press Ctrl+R rapidly 5 times
for i in {1..5}; do
    tmux send-keys -t "$SESSION_NAME" "C-r"
    sleep 0.2
done

sleep 5

# Verify no crashes or errors
OUTPUT=$(capture_output)
if ! echo "$OUTPUT" | grep -i "error\|panic\|crash"; then
    log_pass "  No crashes or errors with rapid Ctrl+R presses"
else
    log_fail "  Errors detected in output"
fi

cleanup
sleep 1

# Test 8: Regeneration with complex content
log_test "Test 8: Regeneration with code/complex content"
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 2

tmux send-keys -t "$SESSION_NAME" "write a hello world in rust" Enter
sleep 4

MSG_COUNT_BEFORE=$(get_message_count)

# Regenerate
tmux send-keys -t "$SESSION_NAME" "C-r"
sleep 4

MSG_COUNT_AFTER=$(get_message_count)

if [ "$MSG_COUNT_BEFORE" -eq "$MSG_COUNT_AFTER" ]; then
    log_pass "  Code response properly regenerated"
else
    log_fail "  Code regeneration changed message count"
fi

# Verify new response also contains code
OUTPUT=$(capture_output)
if echo "$OUTPUT" | grep -q "fn main\|println\|pub fn"; then
    log_pass "  New response contains expected code"
else
    log_fail "  New response may be missing code content"
fi

cleanup

# Summary
echo "" | tee -a "$TEST_LOG"
echo "======================================" | tee -a "$TEST_LOG"
echo "Test Suite Summary" | tee -a "$TEST_LOG"
echo "======================================" | tee -a "$TEST_LOG"
echo -e "Total Tests: $((PASSED + FAILED))" | tee -a "$TEST_LOG"
echo -e "${GREEN}Passed: $PASSED${NC}" | tee -a "$TEST_LOG"
echo -e "${RED}Failed: $FAILED${NC}" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}" | tee -a "$TEST_LOG"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}" | tee -a "$TEST_LOG"
    echo "Full log saved to: $TEST_LOG"
    exit 1
fi
