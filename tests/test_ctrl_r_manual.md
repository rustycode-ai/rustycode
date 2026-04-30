# Ctrl+R Regenerate Response - Manual Test Procedure

## Test Environment Setup

1. Ensure RustyCode TUI is built: `cargo build --release -p rustycode-tui`
2. Configure LLM provider (check `~/.rustycode/preferences.json`)
3. Have a working LLM API key available

## Test Cases

### Test 1: Basic Regeneration (Happy Path)
**Steps:**
1. Launch TUI: `./target/release/rustycode-tui`
2. Type: `hello`
3. Press Enter
4. Wait for LLM response
5. Press `Ctrl+R`
6. Wait for new response

**Expected Results:**
- [ ] "Regenerating response..." message appears
- [ ] Old AI message is removed
- [ ] New AI message is generated
- [ ] Total message count stays the same (old replaced by new)
- [ ] Response is different from the original

**Actual Results:**
```
(Record observations here)
```

---

### Test 2: Edge Case - No Messages
**Steps:**
1. Launch TUI fresh
2. Immediately press `Ctrl+R` (without sending any message)

**Expected Results:**
- [ ] Error message: "No AI response to regenerate. Send a message first."
- [ ] No crash or panic
- [ ] UI remains responsive

**Actual Results:**
```
(Record observations here)
```

---

### Test 3: Edge Case - During Streaming
**Steps:**
1. Launch TUI
2. Send a long prompt that will stream: `write a detailed explanation of quantum computing`
3. Immediately press `Ctrl+R` while response is streaming

**Expected Results:**
- [ ] Error message: "Cannot regenerate while streaming. Please wait."
- [ ] Streaming continues uninterrupted
- [ ] No race condition or crash

**Actual Results:**
```
(Record observations here)
```

---

### Test 4: Multiple Sequential Regenerations
**Steps:**
1. Launch TUI
2. Send: `what is 2+2?`
3. Wait for response
4. Press `Ctrl+R` (1st regeneration)
5. Wait for new response
6. Press `Ctrl+R` (2nd regeneration)
7. Wait for new response
8. Press `Ctrl+R` (3rd regeneration)
9. Wait for new response

**Expected Results:**
- [ ] Each regeneration shows "Regenerating response..." message
- [ ] Only last message is regenerated each time
- [ ] Message count stays constant
- [ ] No memory leaks or performance degradation
- [ ] Each response can be different

**Actual Results:**
```
(Record observations here)
```

---

### Test 5: Multi-turn Conversation
**Steps:**
1. Launch TUI
2. Send: `my name is Alice`
3. Wait for response
4. Send: `what is my name?`
5. Wait for response
6. Press `Ctrl+R`

**Expected Results:**
- [ ] Only the last response (to "what is my name?") is regenerated
- [ ] Earlier messages ("my name is Alice" and its response) remain intact
- [ ] Context is preserved for the new response

**Actual Results:**
```
(Record observations here)
```

---

### Test 6: Verify Old Message Removal
**Steps:**
1. Launch TUI
2. Send: `remember the number 12345`
3. Wait for response (which should include "12345")
4. Count occurrences of "12345" in visible messages
5. Press `Ctrl+R`
6. Wait for new response
7. Count occurrences of "12345" again

**Expected Results:**
- [ ] Number of "12345" occurrences is the same or less after regeneration
- [ ] Old response containing "12345" is removed
- [ ] New response appears without duplicate old content

**Actual Results:**
```
(Record observations here)
```

---

### Test 7: Rapid Successive Ctrl+R Presses
**Steps:**
1. Launch TUI
2. Send: `test message`
3. Wait for response
4. Quickly press `Ctrl+R` 5 times in succession (with 0.5s intervals)

**Expected Results:**
- [ ] No crash or panic
- [ ] TUI remains responsive
- [ ] Eventually settles on a final response
- [ ] No error messages (or only appropriate rate limiting)

**Actual Results:**
```
(Record observations here)
```

---

### Test 8: Code Responses
**Steps:**
1. Launch TUI
2. Send: `write a hello world in rust`
3. Wait for code response
4. Press `Ctrl+R`
5. Wait for new response

**Expected Results:**
- [ ] Code formatting is preserved in new response
- [ ] New response also contains code
- [ ] No syntax errors or rendering issues
- [ ] Code blocks display correctly

**Actual Results:**
```
(Record observations here)
```

---

### Test 9: System Message Verification
**Steps:**
1. Launch TUI
2. Send: `test`
3. Wait for response
4. Press `Ctrl+R`
5. Watch for system message

**Expected Results:**
- [ ] System message appears: "🔄 Regenerating response..."
- [ ] Emoji (🔄) displays correctly
- [ ] Message is styled differently from user/AI messages
- [ ] System message appears before new response

**Actual Results:**
```
(Record observations here)
```

---

### Test 10: Integration with Other Features
**Steps:**
1. Launch TUI
2. Send a message
3. Wait for response
4. Press `Ctrl+R` to regenerate
5. After new response, try:
   - Press `/` to open command palette
   - Press arrow keys to navigate history
   - Press `Ctrl+K` to copy message

**Expected Results:**
- [ ] Other features still work after regeneration
- [ ] No state corruption
- [ ] UI remains fully functional

**Actual Results:**
```
(Record observations here)
```

---

## Summary Checklist

After completing all tests, fill out this summary:

- **Total Tests Run**: ___/10
- **Tests Passed**: ___/10
- **Tests Failed**: ___/10
- **Critical Issues Found**: ___
- **Non-Critical Issues Found**: ___

### Issues Found

| Test # | Issue | Severity | Notes |
|--------|-------|----------|-------|
| 1 | | | |
| 2 | | | |
| ... | | | |

### Recommendations

```
(Record any improvement suggestions here)
```

---

## Testing Notes

- **LLM Provider**: _________________
- **Model Used**: _________________
- **TUI Version**: _________________
- **Test Date**: _________________
- **Tester**: _________________

### Known Limitations

1. Rate limiting may prevent rapid testing
2. LLM response variability makes exact comparison difficult
3. Timing-dependent tests (streaming) may need multiple attempts

### Test Environment

- OS: macOS Darwin 25.3.0
- Shell: zsh
- Terminal: tmux for automated testing

