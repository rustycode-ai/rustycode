//! Test the todo/task system
use rustycode_tui::task_commands::{CommandResult, handle_command};
use rustycode_tui::tasks::{WorkspaceTasks, load_tasks};

fn main() {
    println!("Testing Todo/Task System...\n");

    // Create a test workspace
    let mut tasks = WorkspaceTasks {
        tasks: vec![],
        todos: vec![],
        active_agents: vec![],
    };

    // Test 1: Add a todo
    println!("Test 1: Add a todo");
    let result = handle_command("/todo add buy milk", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Total todos: {}\n", tasks.todos.len());

    // Test 2: Add another todo
    println!("Test 2: Add another todo");
    let result = handle_command("/todo add write code", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Total todos: {}\n", tasks.todos.len());

    // Test 3: List todos
    println!("Test 3: List todos");
    let result = handle_command("/todo list", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success:\n{}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }

    // Test 4: Complete a todo
    println!("\nTest 4: Complete todo #1");
    let result = handle_command("/todo done 1", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Todo #1 done: {}", tasks.todos[0].done);

    // Test 5: List todos again
    println!("\nTest 5: List todos after completion");
    let result = handle_command("/todo list", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success:\n{}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }

    // Test 6: Create a task
    println!("\nTest 6: Create a task");
    let result = handle_command("/task create Fix the TUI", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Total tasks: {}\n", tasks.tasks.len());

    // Test 7: List tasks
    println!("Test 7: List tasks");
    let result = handle_command("/task list", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success:\n{}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }

    // Test 8: Start a task
    println!("\nTest 8: Start task #1");
    let result = handle_command("/task start 1", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Task #1 status: {:?}\n", tasks.tasks[0].status);

    // Test 9: Complete a task
    println!("Test 9: Complete task #1");
    let result = handle_command("/task complete 1", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Task #1 status: {:?}\n", tasks.tasks[0].status);

    // Test 10: Spawn an agent
    println!("Test 10: Spawn an agent");
    let result = handle_command("/agent Refactor the input handler", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Total agents: {}\n", tasks.active_agents.len());

    // Test 11: Delete a todo
    println!("Test 11: Delete todo #1");
    let result = handle_command("/todo delete 1", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success: {}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }
    println!("Total todos: {}\n", tasks.todos.len());

    // Test 12: List todos after deletion
    println!("Test 12: List todos after deletion");
    let result = handle_command("/todo list", &mut tasks);
    match &result {
        CommandResult::Success(msg) => println!("✅ Success:\n{}", msg),
        CommandResult::Error(err) => println!("❌ Error: {}", err),
        CommandResult::Consumed => println!("⚠️ Consumed"),
    }

    println!("\n✅ All tests completed!");
}
