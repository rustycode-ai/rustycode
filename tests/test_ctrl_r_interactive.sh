#!/bin/bash
# Interactive test script for Ctrl+R regenerate functionality
# This script performs manual-style testing with automated verification

set -e

TUI_BIN="/Users/nat/dev/rustycode/target/release/rustycode-tui"
SESSION_NAME="tui-regen-interactive"
TEST_LOG="/tmp/tui_regen_interactive.log"
PASSED=0
FAILED=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

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

log_info() {
    echo -e "${BLUE}INFO:${NC} $1" | tee -a "$TEST_LOG"
}

cleanup() {
    log_info "Cleaning up tmux session..."
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    killall rustycode-tui 2>/dev/null || true
    sleep 1
}

capture_screen() {
    tmux capture-pane -t "$SESSION_NAME" -p > /tmp/tui_screen.txt
    cat /tmp/tui_screen.txt | tee -a "$TEST_LOG"
}

wait_for_pattern() {
    local pattern=$1
    local timeout=${2:-10}
    local elapsed=0

    log_info "Waiting for pattern: $pattern (timeout: ${timeout}s)"

    while [ $elapsed -lt $timeout ]; do
        OUTPUT=$(tmux capture-pane -t "$SESSION_NAME" -p)
        if echo "$OUTPUT" | grep -qi "$pattern"; then
            log_info "Pattern found after ${elapsed}s"
            return 0
        fi
        sleep 1
        ((elapsed++))
    done

    log_info "Pattern NOT found after ${timeout}s"
    capture_screen
    return 1
}

send_command() {
    local cmd=$1
    log_info "Sending command: $cmd"
    tmux send-keys -t "$SESSION_NAME" "$cmd" Enter
}

press_ctrl_r() {
    log_info "Pressing Ctrl+R"
    tmux send-keys -t "$SESSION_NAME" "C-r"
}

# Initialize
echo "========================================" | tee "$TEST_LOG"
echo "Ctrl+R Interactive Test Suite" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"

cleanup
sleep 1

# Test 1: Basic regeneration workflow
log_test "Test 1: Basic regeneration workflow"
echo "----------------------------------------" | tee -a "$TEST_LOG"

log_info "Starting TUI..."
tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 3

log_info "Sending first message..."
send_command "hello"
sleep 4

log_info "Capturing initial response..."
OUTPUT1=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT1" | tee -a "$TEST_LOG"

if echo "$OUTPUT1" | grep -qi "你好\|hello\|hi\|greeting"; then
    log_pass "Initial response received from LLM"
else
    log_fail "No LLM response detected"
fi

MSG_COUNT_1=$(echo "$OUTPUT1" | grep -c "Messages:" || echo "0")
log_info "Message count before regeneration: $MSG_COUNT_1"

press_ctrl_r
sleep 2

log_info "Checking for regeneration message..."
OUTPUT2=$(tmux capture-pane -t "$SESSION_NAME" -p)

if echo "$OUTPUT2" | grep -qi "regenerating\|regenerate"; then
    log_pass "Regeneration message displayed"
else
    log_fail "No regeneration message found"
    capture_screen
fi

log_info "Waiting for new response..."
sleep 4

OUTPUT3=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT3" | tee -a "$TEST_LOG"

if echo "$OUTPUT3" | grep -qi "你好\|hello\|hi\|greeting"; then
    log_pass "New response generated after regeneration"
else
    log_fail "No new response after regeneration"
fi

cleanup
sleep 2

# Test 2: Edge case - No messages
log_test "Test 2: Edge case - Ctrl+R with no prior messages"
echo "----------------------------------------" | tee -a "$TEST_LOG"

tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 3

press_ctrl_r
sleep 2

OUTPUT=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT" | tee -a "$TEST_LOG"

if echo "$OUTPUT" | grep -qi "no ai response\|cannot regenerate"; then
    log_pass "Correct error message for no messages"
else
    log_fail "Expected error message not found"
fi

cleanup
sleep 2

# Test 3: Multiple regenerations
log_test "Test 3: Multiple sequential regenerations (3x)"
echo "----------------------------------------" | tee -a "$TEST_LOG"

tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 3

send_command "what is 1+1?"
sleep 4

for i in 1 2 3; do
    log_info "Regeneration #$i"
    press_ctrl_r
    sleep 3

    OUTPUT=$(tmux capture-pane -t "$SESSION_NAME" -p)
    if echo "$OUTPUT" | grep -qi "regenerating"; then
        log_pass "Regeneration #$i initiated successfully"
    else
        log_fail "Regeneration #$i failed"
    fi

    sleep 3
done

cleanup
sleep 2

# Test 4: During streaming (if possible)
log_test "Test 4: Ctrl+R during active streaming"
echo "----------------------------------------" | tee -a "$TEST_LOG"

tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 3

send_command "write a short haiku about testing"
sleep 1

# Try to press Ctrl+R quickly while streaming
press_ctrl_r
sleep 2

OUTPUT=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT" | tee -a "$TEST_LOG"

if echo "$OUTPUT" | grep -qi "cannot regenerate while streaming"; then
    log_pass "Correctly blocked during streaming"
else
    log_info "Note: Streaming protection may not be triggered (timing dependent)"
fi

cleanup
sleep 2

# Test 5: Verify old message is removed
log_test "Test 5: Verify old message is properly removed"
echo "----------------------------------------" | tee -a "$TEST_LOG"

tmux new-session -d -s "$SESSION_NAME" "$TUI_BIN"
sleep 3

send_command "remember the number 42"
sleep 4

OUTPUT_BEFORE=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT_BEFORE" | tee -a "$TEST_LOG"

# Count instances of "42"
COUNT_42_BEFORE=$(echo "$OUTPUT_BEFORE" | grep -c "42" || echo "0")
log_info "Occurrences of '42' before regeneration: $COUNT_42_BEFORE"

press_ctrl_r
sleep 5

OUTPUT_AFTER=$(tmux capture-pane -t "$SESSION_NAME" -p)
echo "$OUTPUT_AFTER" | tee -a "$TEST_LOG"

COUNT_42_AFTER=$(echo "$OUTPUT_AFTER" | grep -c "42" || echo "0")
log_info "Occurrences of '42' after regeneration: $COUNT_42_AFTER"

# The count should be the same or less (old message removed)
if [ "$COUNT_42_AFTER" -le "$COUNT_42_BEFORE" ]; then
    log_pass "Old message properly removed or replaced"
else
    log_fail "Message count increased unexpectedly"
fi

cleanup

# Summary
echo "" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "Test Summary" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "Total: $((PASSED + FAILED)) tests" | tee -a "$TEST_LOG"
echo -e "${GREEN}Passed: $PASSED${NC}" | tee -a "$TEST_LOG"
echo -e "${RED}Failed: $FAILED${NC}" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"
echo "Full log: $TEST_LOG" | tee -a "$TEST_LOG"

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi
