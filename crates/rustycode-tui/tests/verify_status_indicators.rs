#![allow(clippy::if_not_else, clippy::uninlined_format_args)]

//! Quick verification test for status bar visual indicators
//!
//! This test verifies the logic for counting and formatting tasks and todos.

use rustycode_tui::tasks::{TaskStatus, Todo, WorkspaceTasks};
use std::time::SystemTime;

fn create_test_tasks() -> WorkspaceTasks {
    WorkspaceTasks {
        tasks: vec![
            rustycode_tui::tasks::Task {
                id: "1".to_string(),
                description: "Task 1".to_string(),
                status: TaskStatus::InProgress,
                created_at: SystemTime::now(),
                dependencies: vec![],
                owner: None,
            },
            rustycode_tui::tasks::Task {
                id: "2".to_string(),
                description: "Task 2".to_string(),
                status: TaskStatus::InProgress,
                created_at: SystemTime::now(),
                dependencies: vec![],
                owner: None,
            },
        ],
        todos: vec![
            Todo {
                id: "1".to_string(),
                text: "Todo 1".to_string(),
                done: false,
                created_at: SystemTime::now(),
            },
            Todo {
                id: "2".to_string(),
                text: "Todo 2".to_string(),
                done: false,
                created_at: SystemTime::now(),
            },
            Todo {
                id: "3".to_string(),
                text: "Todo 3".to_string(),
                done: true, // Completed
                created_at: SystemTime::now(),
            },
        ],
        active_agents: vec![],
    }
}

#[test]
fn test_task_status_counts() {
    let tasks = create_test_tasks();

    let in_progress_count = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InProgress))
        .count();

    let pending_todos = tasks.todos.iter().filter(|t| !t.done).count();

    println!("✓ In-progress tasks: {}", in_progress_count);
    println!("✓ Pending todos: {}", pending_todos);

    assert_eq!(in_progress_count, 2, "Should have 2 in-progress tasks");
    assert_eq!(pending_todos, 2, "Should have 2 pending todos");
}

#[test]
fn test_status_bar_formatting() {
    let tasks = create_test_tasks();

    // Simulate the status bar logic
    let in_progress_tasks = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InProgress))
        .count();

    let pending_todos = tasks.todos.iter().filter(|t| !t.done).count();

    // Build status string (simplified)
    let mut status_parts = Vec::new();

    if in_progress_tasks > 0 {
        status_parts.push(format!("🔄{}", in_progress_tasks));
    }

    if pending_todos > 0 {
        status_parts.push(format!("☐{}", pending_todos));
    }

    let status = if !status_parts.is_empty() {
        status_parts.join(" ")
    } else {
        "No activity".to_string()
    };

    println!("✓ Status bar would show: {}", status);

    // Verify format
    assert!(status.contains("🔄2"), "Should show 2 tasks");
    assert!(status.contains("☐2"), "Should show 2 todos");
}

#[test]
fn test_empty_state() {
    let tasks = WorkspaceTasks {
        tasks: vec![],
        todos: vec![],
        active_agents: vec![],
    };

    let in_progress_tasks = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InProgress))
        .count();

    let pending_todos = tasks.todos.iter().filter(|t| !t.done).count();

    println!(
        "✓ Empty state - tasks: {}, todos: {}",
        in_progress_tasks, pending_todos
    );

    assert_eq!(in_progress_tasks, 0, "Should have no in-progress tasks");
    assert_eq!(pending_todos, 0, "Should have no pending todos");
}

#[test]
fn test_task_status_variants() {
    let mut tasks = WorkspaceTasks {
        tasks: vec![],
        todos: vec![],
        active_agents: vec![],
    };

    // Test all status types
    for status in [
        TaskStatus::Pending,
        TaskStatus::InProgress,
        TaskStatus::Completed,
        TaskStatus::Blocked,
        TaskStatus::Running,
        TaskStatus::Failed,
        TaskStatus::Killed,
    ] {
        tasks.tasks.push(rustycode_tui::tasks::Task {
            id: format!("{:?}", status),
            description: format!("Task with {:?}", status),
            status: status.clone(),
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: None,
        });
    }

    let pending_count = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Pending))
        .count();

    let in_progress_count = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InProgress))
        .count();

    let completed_count = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Completed))
        .count();

    let blocked_count = tasks
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Blocked))
        .count();

    println!("✓ Task status breakdown:");
    println!("  - Pending: {}", pending_count);
    println!("  - In Progress: {}", in_progress_count);
    println!("  - Completed: {}", completed_count);
    println!("  - Blocked: {}", blocked_count);

    assert_eq!(pending_count, 1);
    assert_eq!(in_progress_count, 1);
    assert_eq!(completed_count, 1);
    assert_eq!(blocked_count, 1);
}

#[test]
fn test_todo_done_states() {
    let tasks = WorkspaceTasks {
        tasks: vec![],
        todos: vec![
            Todo {
                id: "1".to_string(),
                text: "Done todo".to_string(),
                done: true,
                created_at: SystemTime::now(),
            },
            Todo {
                id: "2".to_string(),
                text: "Pending todo".to_string(),
                done: false,
                created_at: SystemTime::now(),
            },
        ],
        active_agents: vec![],
    };

    let done_count = tasks.todos.iter().filter(|t| t.done).count();

    let pending_count = tasks.todos.iter().filter(|t| !t.done).count();

    println!(
        "✓ Todo states - done: {}, pending: {}",
        done_count, pending_count
    );

    assert_eq!(done_count, 1, "Should have 1 completed todo");
    assert_eq!(pending_count, 1, "Should have 1 pending todo");
}
