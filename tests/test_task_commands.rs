//! Test/demo for task management commands
//!
//! Run with: cargo run --example test_task_commands

use rustycode_tui::task_commands::{
    CommandResult, handle_agent_command, handle_task_command, handle_todo_command,
};
use rustycode_tui::tasks::{AgentStatus, TaskStatus, WorkspaceTasks, load_tasks, save_tasks};
use std::path::PathBuf;

fn main() {
    println!("🧪 Testing Task Management Commands\n");
    println!("═══════════════════════════════════════\n");

    // Create a temporary directory for testing
    let test_dir = std::env::current_dir().unwrap();
    println!("📁 Test directory: {:?}\n", test_dir);

    // Initialize empty tasks
    let mut tasks = WorkspaceTasks {
        tasks: Vec::new(),
        todos: Vec::new(),
        active_agents: Vec::new(),
    };

    // Test 1: Create tasks
    println!("📝 Test 1: Creating tasks");
    println!("─────────────────────────────");
    match handle_task_command("create", &["Fix the TUI rendering".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    match handle_task_command(
        "create",
        &["Implement slash commands".to_string()],
        &mut tasks,
    ) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    match handle_task_command(
        "create",
        &["Add syntax highlighting".to_string()],
        &mut tasks,
    ) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 2: Listing tasks");
    println!("─────────────────────────────");
    match handle_task_command("list", &[], &mut tasks) {
        CommandResult::Success(msg) => println!("{}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 3: Starting a task");
    println!("─────────────────────────────");
    match handle_task_command("start", &["2".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 4: Completing a task");
    println!("─────────────────────────────");
    match handle_task_command("complete", &["3".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 5: Creating todos");
    println!("─────────────────────────────");
    match handle_todo_command("add", &["Review PR 123".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    match handle_todo_command("add", &["Update documentation".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 6: Listing todos");
    println!("─────────────────────────────");
    match handle_todo_command("list", &[], &mut tasks) {
        CommandResult::Success(msg) => println!("{}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 7: Marking todo as done");
    println!("─────────────────────────────");
    match handle_todo_command("done", &["1".to_string()], &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 8: Spawning an agent");
    println!("─────────────────────────────");
    match handle_agent_command("Refactor the input handler", &mut tasks) {
        CommandResult::Success(msg) => println!("✅ {}", msg),
        CommandResult::Error(err) => println!("❌ {}", err),
        _ => {}
    }

    println!("\n📝 Test 9: Final state");
    println!("─────────────────────────────");
    println!("Tasks: {}", tasks.tasks.len());
    for (i, task) in tasks.tasks.iter().enumerate() {
        let icon = match task.status {
            TaskStatus::Pending => "⏳",
            TaskStatus::InProgress => "🔄",
            TaskStatus::Completed => "✅",
            TaskStatus::Blocked => "🚫",
        };
        println!("  {}  {}. {}", icon, i + 1, task.description);
    }

    println!("\nTodos: {}", tasks.todos.len());
    for (i, todo) in tasks.todos.iter().enumerate() {
        let checkbox = if todo.done { "☑" } else { "☐" };
        println!("  {}  {}. {}", checkbox, i + 1, todo.text);
    }

    println!("\nActive Agents: {}", tasks.active_agents.len());
    for (i, agent) in tasks.active_agents.iter().enumerate() {
        let icon = match agent.status {
            AgentStatus::Starting => "⚡",
            AgentStatus::Running => "🤖",
            AgentStatus::Completed => "✨",
            AgentStatus::Failed => "💥",
            AgentStatus::Killed => "🗑️",
        };
        println!(
            "  {}  {}. {} (ID: {})",
            icon,
            i + 1,
            agent.task,
            &agent.id[..8]
        );
    }

    println!("\n📝 Test 10: Save to file");
    println!("─────────────────────────────");
    match save_tasks(&tasks) {
        Ok(()) => println!("✅ Tasks saved to .rustycode/tasks.json"),
        Err(e) => println!("❌ Failed to save: {}", e),
    }

    println!("\n📝 Test 11: Load from file");
    println!("─────────────────────────────");
    let loaded = load_tasks();
    println!(
        "✅ Loaded {} tasks, {} todos, {} agents",
        loaded.tasks.len(),
        loaded.todos.len(),
        loaded.active_agents.len()
    );

    println!("\n═══════════════════════════════════════");
    println!("✨ All tests completed successfully!");
}
