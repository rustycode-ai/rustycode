#!/bin/bash
# Test todo functionality in the TUI

echo "Testing TUI Todo/Task System..."
echo "This test will:"
echo "1. Start the TUI"
echo "2. Execute todo commands"
echo "3. Verify they work"
echo ""

# Check if tmux is available
if ! command -v tmux &> /dev/null; then
    echo "❌ tmux is not installed. Please install it first."
    exit 1
fi

# Create a temporary directory for testing
TEST_DIR="/tmp/rustycode_todo_test_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "Test directory: $TEST_DIR"

# Initialize git repo for context
git init
git config user.email "test@test.com"
git config user.name "Test User"

# Create test script for tmux
cat > test_todos.exp <<'EOF'
#!/usr/bin/expect -f
set timeout 10

# Spawn the TUI
spawn cargo run --bin rustycode

# Wait for TUI to start
expect ">*"

# Test 1: Add a todo
send "/todo add buy milk\r"
expect "buy milk"
sleep 1

# Test 2: Add another todo
send "/todo add write code\r"
expect "write code"
sleep 1

# Test 3: List todos
send "/todo list\r"
expect "Todos:"
sleep 1

# Test 4: Complete a todo
send "/todo done 1\r"
expect "Done:"
sleep 1

# Test 5: Create a task
send "/task create Fix the TUI\r"
expect "Task"
sleep 1

# Test 6: List tasks
send "/task list\r"
expect "Tasks:"
sleep 1

# Test 7: Start a task
send "/task start 1\r"
expect "Started:"
sleep 1

# Test 8: Complete a task
send "/task complete 1\r"
expect "Completed:"
sleep 1

# Test 9: Spawn an agent
send "/agent Test agent task\r"
expect "Agent"
sleep 1

# Test 10: Exit
send "\x03"  # Ctrl+C
sleep 1

# Exit expect
expect eof
EOF

chmod +x test_todos.exp

# Run the test in tmux
echo "Running TUI test..."
./test_todos.exp

# Check if tasks.json was created
if [ -f ".rustycode/tasks.json" ]; then
    echo ""
    echo "✅ Tasks file created successfully!"
    echo "Contents:"
    cat .rustycode/tasks.json | head -20
else
    echo ""
    echo "❌ Tasks file was not created"
fi

# Cleanup
cd /
rm -rf "$TEST_DIR"

echo ""
echo "✅ Test completed!"
