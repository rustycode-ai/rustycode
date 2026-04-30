#!/bin/bash
# Side-by-Side AI Tools Testing in tmux
# Test if RustyCode works as expected alongside other tools

set -e

echo "=== Setting up Side-by-Side AI Tools Test ==="
echo ""

# Kill any existing session
tmux kill-session -t ai-tools-test 2>/dev/null || true

# Create new session with side-by-side layout
tmux new-session -d -s ai-tools-test -n "AI Tools Comparison" zsh

# Split into 3 panes: RustyCode | Aider | Continue/Other
tmux split-window -h -t ai-tools-test:0
tmux split-window -t ai-tools-test:0.1

# Setup pane 0.0 - RustyCode
tmux send-keys -t ai-tools-test:0.0 'clear' Enter
tmux send-keys -t ai-tools-test:0.0 'echo "=== PANE 1: RUSTYCODE ==="' Enter
tmux send-keys -t ai-tools-test:0.0 'cd /Users/nat/dev/rustycode' Enter
tmux send-keys -t ai-tools-test:0.0 'cargo build --release 2>&1 | tail -3' Enter
tmux send-keys -t ai-tools-test:0.0 'echo "Build complete. Testing: rustycode --version"' Enter
tmux send-keys -t ai-tools-test:0.0 './target/release/rustycode --version' Enter

# Setup pane 0.1 - Aider
tmux send-keys -t ai-tools-test:0.1 'clear' Enter
tmux send-keys -t ai-tools-test:0.1 'echo "=== PANE 2: AIDER ==="' Enter
if command -v aider &> /dev/null; then
    tmux send-keys -t ai-tools-test:0.1 'echo "Testing: aider --version"' Enter
    tmux send-keys -t ai-tools-test:0.1 'aider --version' Enter
    tmux send-keys -t ai-tools-test:0.1 'echo ""' Enter
    tmux send-keys -t ai-tools-test:0.1 'echo "To test Aider: cd /tmp && mkdir test-aider && cd test-aider && git init"' Enter
else
    tmux send-keys -t ai-tools-test:0.1 'echo "❌ Aider not installed"' Enter
    tmux send-keys -t ai-tools-test:0.1 'echo "Install: pip install aider-chat"' Enter
fi

# Setup pane 0.2 - Continue or other tool
tmux send-keys -t ai-tools-test:0.2 'clear' Enter
tmux send-keys -t ai-tools-test:0.2 'echo "=== PANE 3: CONTINUE / OTHER ==="' Enter
if command -v continue &> /dev/null; then
    tmux send-keys -t ai-tools-test:0.2 'echo "Testing: continue --version"' Enter
    tmux send-keys -t ai-tools-test:0.2 'continue --version' Enter
else
    tmux send-keys -t ai-tools-test:0.2 'echo "❌ Continue not installed"' Enter
    tmux send-keys -t ai-tools-test:0.2 'echo "Alternative: testing cursor-cli or other tools"' Enter
fi

# Create a test script
cat > /tmp/ai_tools_test.md <<'EOF'
# AI Tools Functionality Test

## Test Tasks (try these in each tool):

1. **Basic Setup**
   - Initialize a git repo
   - Create a simple Python/JavaScript file
   - Ask the tool to explain the code

2. **Code Generation**
   - Request: "Write a function that fetches data from an API"
   - Compare the quality and speed of responses

3. **Refactoring**
   - Create some messy code
   - Ask: "Refactor this for better readability"
   - Compare the suggestions

4. **Bug Fixing**
   - Intentionally add a bug
   - Ask: "Find and fix the bug"
   - Compare accuracy

5. **Memory/System Context**
   - Make multiple requests
   - Test if tool remembers previous context
   - Compare context retention

## Questions to Answer:
- ✅ Does the tool start without errors?
- ✅ Can it read and analyze code?
- ✅ Does it generate working code?
- ✅ How fast are the responses?
- ✅ Does it remember context?
- ✅ Error handling quality?

## Comparison Points:
- **RustyCode**: Rust-based, TUI, offline-capable
- **Aider**: Python-based, terminal UI, needs API keys
- **Continue**: Node-based, VSCode integration, needs API keys

## To Test Each Tool:

### RustyCode (Pane 1)
```bash
cd /tmp/test-rustycode
git init
echo 'print("hello")' > test.py
cargo run --release -- --help
```

### Aider (Pane 2)
```bash
cd /tmp/test-aider
git init
echo 'print("hello")' > test.py
aider --help
```

### Continue (Pane 3)
```bash
cd /tmp/test-continue
git init
continue --help
```
EOF

# Display instructions
echo "✅ tmux session created: 'ai-tools-test'"
echo ""
echo "📋 Test checklist created: /tmp/ai_tools_test.md"
echo ""
echo "🚀 To start testing:"
echo "   tmux attach-session -t ai-tools-test"
echo ""
echo "📊 Layout: [RustyCode | Aider | Continue/Other]"
echo ""
echo "📝 Test each tool with the same tasks and compare:"
echo "   - Startup speed"
echo "   - Code generation quality"
echo "   - Context understanding"
echo "   - Error handling"
echo "   - Memory efficiency"
echo ""
echo "💡 Tips:"
echo "   - Use the same coding task in each pane"
echo "   - Time the responses"
echo "   - Compare code quality"
echo "   - Test memory/context retention"
echo ""

# Show the test checklist
cat /tmp/ai_tools_test.md

echo ""
echo "==================================="
echo "Ready to test! Attach with:"
echo "  tmux attach-session -t ai-tools-test"
echo "==================================="
