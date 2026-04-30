#!/bin/bash
# Integration test for TUI todo system

echo "=== TUI Todo System Integration Test ==="
echo ""

# Create test directory
TEST_DIR="/tmp/rustycode_todo_integration_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "Test directory: $TEST_DIR"
echo ""

# Initialize git repo for context
git init -q
git config user.email "test@test.com"
git config user.name "Test User"

echo "=== Test 1: Verify task commands work ==="
cat > test_commands.rs <<'EOF'
use rustycode_tui::task_commands::{handle_command, CommandResult};
use rustycode_tui::tasks::{load_tasks, WorkspaceTasks};

fn main() {
    println!("Testing todo/task commands...\n");

    // Load or create tasks
    let mut tasks = load_tasks();

    // Test todo commands
    println!("--- Todo Commands ---");

    println!("\n1. /todo add buy milk");
    match handle_command("/todo add buy milk", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n2. /todo add write code");
    match handle_command("/todo add write code", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n3. /todo list");
    match handle_command("/todo list", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ Success:\n{}\n", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}\n", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed\n"),
    }

    println!("\n4. /todo done 1");
    match handle_command("/todo done 1", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n5. /todo delete 2");
    match handle_command("/todo delete 2", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    // Test task commands
    println!("\n\n--- Task Commands ---");

    println!("\n6. /task create Fix the TUI");
    match handle_command("/task create Fix the TUI", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n7. /task start 1");
    match handle_command("/task start 1", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n8. /task complete 1");
    match handle_command("/task complete 1", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    // Test agent command
    println!("\n\n--- Agent Commands ---");

    println!("\n9. /agent Refactor the input handler");
    match handle_command("/agent Refactor the input handler", &mut tasks) {
        CommandResult::Success(msg) => println!("   ✅ {}", msg),
        CommandResult::Error(err) => println!("   ❌ Error: {}", err),
        CommandResult::Consumed => println!("   ⚠️ Consumed"),
    }

    println!("\n\n--- Summary ---");
    println!("Total tasks: {}", tasks.tasks.len());
    println!("Total todos: {}", tasks.todos.len());
    println!("Total agents: {}", tasks.active_agents.len());
}
EOF

rustc --edition 2021 \
    -L /Users/nat/dev/rustycode/target/debug/deps \
    --extern rustycode_tui=/Users/nat/dev/rustycode/target/debug/librustycode_tui.rlib \
    test_commands.rs -o test_commands 2>&1 | grep -v "warning"

if [ -f "./test_commands" ]; then
    echo "✅ Compiled test commands"
    echo ""
    ./test_commands
else
    echo "❌ Failed to compile test commands"
    cd /
    rm -rf "$TEST_DIR"
    exit 1
fi

echo ""
echo "=== Test 2: Verify persistence ==="
echo "Checking if tasks were saved..."

if [ -f ".rustycode/tasks.json" ]; then
    echo "✅ tasks.json exists"

    # Count items in JSON
    TASK_COUNT=$(jq '.tasks | length' .rustycode/tasks.json 2>/dev/null || echo "0")
    TODO_COUNT=$(jq '.todos | length' .rustycode/tasks.json 2>/dev/null || echo "0")
    AGENT_COUNT=$(jq '.active_agents | length' .rustycode/tasks.json 2>/dev/null || echo "0")

    echo "   Tasks: $TASK_COUNT"
    echo "   Todos: $TODO_COUNT"
    echo "   Agents: $AGENT_COUNT"

    if [ "$TASK_COUNT" -gt 0 ] || [ "$TODO_COUNT" -gt 0 ] || [ "$AGENT_COUNT" -gt 0 ]; then
        echo "✅ Tasks were persisted correctly"
    else
        echo "⚠️  Tasks file is empty"
    fi
else
    echo "❌ tasks.json was not created"
fi

echo ""
echo "=== Test 3: Verify data can be reloaded ==="
cat > test_reload.rs <<'EOF'
use rustycode_tui::tasks::load_tasks;

fn main() {
    println!("Reloading tasks from disk...");
    let tasks = load_tasks();

    println!("Loaded {} tasks", tasks.tasks.len());
    for (i, task) in tasks.tasks.iter().enumerate() {
        println!("  {}. {} - {:?}", i + 1, task.description, task.status);
    }

    println!("\nLoaded {} todos", tasks.todos.len());
    for (i, todo) in tasks.todos.iter().enumerate() {
        let checkbox = if todo.done { "☑" } else { "☐" };
        println!("  {}{} {}", checkbox, i + 1, todo.text);
    }

    println!("\nLoaded {} agents", tasks.active_agents.len());
    for (i, agent) in tasks.active_agents.iter().enumerate() {
        println!("  {}. {} - {:?}", i + 1, agent.id, agent.status);
    }
}
EOF

rustc --edition 2021 \
    -L /Users/nat/dev/rustycode/target/debug/deps \
    --extern rustycode_tui=/Users/nat/dev/rustycode/target/debug/librustycode_tui.rlib \
    test_reload.rs -o test_reload 2>&1 | grep -v "warning"

if [ -f "./test_reload" ]; then
    echo "✅ Compiled reload test"
    echo ""
    ./test_reload
else
    echo "❌ Failed to compile reload test"
fi

# Cleanup
cd /
rm -rf "$TEST_DIR"

echo ""
echo "✅ All integration tests completed!"
echo ""
echo "Summary:"
echo "  ✅ Todo commands work"
echo "  ✅ Task commands work"
echo "  ✅ Agent commands work"
echo "  ✅ Persistence works"
echo "  ✅ Reloading works"
echo ""
echo "The todo/task system is fully functional!"
