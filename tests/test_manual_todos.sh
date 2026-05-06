#!/bin/bash
# Manual test for todo system

echo "=== Manual Todo System Test ==="
echo ""

# Create test directory
TEST_DIR="/tmp/rustycode_todo_manual_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "Test directory: $TEST_DIR"
echo ""

# Test 1: Create a tasks.json file manually
echo "Test 1: Creating tasks.json file..."
mkdir -p .rustycode
cat > .rustycode/tasks.json <<'EOF'
{
  "tasks": [
    {
      "id": "01HKQ3PG0000000000000000",
      "description": "Fix the TUI",
      "status": "Pending",
      "created_at": [
        1699,
        1234567890,
        0
      ],
      "dependencies": []
    }
  ],
  "todos": [
    {
      "id": "01HKQ3PG0000000000000001",
      "text": "buy milk",
      "done": false,
      "created_at": [
        1699,
        1234567891,
        0
      ]
    },
    {
      "id": "01HKQ3PG0000000000000002",
      "text": "write code",
      "done": true,
      "created_at": [
        1699,
        1234567892,
        0
      ]
    }
  ],
  "active_agents": []
}
EOF

echo "✅ Created tasks.json"
echo ""

# Test 2: Read it back with the Rust code
echo "Test 2: Reading tasks with Rust code..."
cat > read_tasks.rs <<'EOF'
use rustycode_tui::tasks::load_tasks;

fn main() {
    let tasks = load_tasks();
    println!("Tasks: {}", tasks.tasks.len());
    for (i, task) in tasks.tasks.iter().enumerate() {
        println!("  {}. {} - {:?}", i + 1, task.description, task.status);
    }

    println!("\nTodos: {}", tasks.todos.len());
    for (i, todo) in tasks.todos.iter().enumerate() {
        let checkbox = if todo.done { "☑" } else { "☐" };
        println!("  {}{} {}", checkbox, i + 1, todo.text);
    }

    println!("\nActive Agents: {}", tasks.active_agents.len());
}
EOF

rustc --edition 2021 -L /Users/nat/dev/rustycode/target/debug/deps \
    --extern rustycode_tui=/Users/nat/dev/rustycode/target/debug/librustycode_tui.rlib \
    read_tasks.rs -o read_tasks 2>&1 | grep -v "warning"

if [ -f "./read_tasks" ]; then
    echo "✅ Compiled read_tasks program"
    echo ""
    echo "Output:"
    ./read_tasks
else
    echo "❌ Failed to compile read_tasks"
fi

echo ""
echo "Test 3: Updating tasks with Rust code..."
cat > update_tasks.rs <<'EOF'
use rustycode_tui::task_commands::{handle_command, CommandResult};
use rustycode_tui::tasks::{load_tasks, save_tasks};

fn main() {
    let mut tasks = load_tasks();

    println!("Initial state:");
    println!("  Tasks: {}", tasks.tasks.len());
    println!("  Todos: {}", tasks.todos.len());
    println!("  Agents: {}", tasks.active_agents.len());

    // Add a todo
    println!("\nExecuting: /todo add test todo");
    match handle_command("/todo add test todo", &mut tasks) {
        CommandResult::Success(msg) => println!("  ✅ {}", msg),
        CommandResult::Error(err) => println!("  ❌ Error: {}", err),
        CommandResult::Consumed => println!("  ⚠️ Consumed"),
    }

    // Complete a todo
    println!("\nExecuting: /todo done 2");
    match handle_command("/todo done 2", &mut tasks) {
        CommandResult::Success(msg) => println!("  ✅ {}", msg),
        CommandResult::Error(err) => println!("  ❌ Error: {}", err),
        CommandResult::Consumed => println!("  ⚠️ Consumed"),
    }

    // Start a task
    println!("\nExecuting: /task start 1");
    match handle_command("/task start 1", &mut tasks) {
        CommandResult::Success(msg) => println!("  ✅ {}", msg),
        CommandResult::Error(err) => println!("  ❌ Error: {}", err),
        CommandResult::Consumed => println!("  ⚠️ Consumed"),
    }

    // Save
    println!("\nSaving tasks...");
    match save_tasks(&tasks) {
        Ok(_) => println!("  ✅ Saved successfully"),
        Err(e) => println!("  ❌ Save failed: {}", e),
    }

    println!("\nFinal state:");
    println!("  Tasks: {}", tasks.tasks.len());
    println!("  Todos: {}", tasks.todos.len());
    println!("  Agents: {}", tasks.active_agents.len());
}
EOF

rustc --edition 2021 -L /Users/nat/dev/rustycode/target/debug/deps \
    --extern rustycode_tui=/Users/nat/dev/rustycode/target/debug/librustycode_tui.rlib \
    update_tasks.rs -o update_tasks 2>&1 | grep -v "warning"

if [ -f "./update_tasks" ]; then
    echo "✅ Compiled update_tasks program"
    echo ""
    echo "Output:"
    ./update_tasks
    echo ""
    echo "Updated tasks.json:"
    cat .rustycode/tasks.json | head -30
else
    echo "❌ Failed to compile update_tasks"
fi

# Cleanup
cd /
rm -rf "$TEST_DIR"

echo ""
echo "✅ All manual tests completed!"
