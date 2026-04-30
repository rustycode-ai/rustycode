# rustycode-tasks

Task management and lifecycle for RustyCode development workflows.

## Purpose

Manages task creation, tracking, and lifecycle during development sessions. Allows breaking down complex development work into subtasks, tracking progress, managing dependencies between tasks, and persisting task state across sessions.

## Intended Features

- **Task Creation** — Define tasks with descriptions, estimates, and dependencies
- **Progress Tracking** — Mark tasks as pending, in_progress, or completed
- **Dependency Management** — Define which tasks block other tasks
- **Persistence** — Save task state to enable multi-session continuity
- **Filtering** — Query tasks by status, owner, priority, or dependency state
- **Notifications** — Alerts when tasks are unblocked or due

## Key Types

- `Task` — Individual task with description, status, owner, estimate
- `TaskManager` — Central manager for task lifecycle
- `TaskFilter` — Query builder for finding tasks by criteria
- `TaskDependency` — Dependency relationship between tasks
- `TaskStatus` — Task state (Pending, InProgress, Completed, Blocked)

## Intended Public API

```rust
use rustycode_tasks::{Task, TaskManager, TaskStatus};

// Create task manager
let manager = TaskManager::new()?;

// Create a task
let task = Task::new("Implement authentication")
    .with_estimate("2 hours")
    .with_owner("alice");

manager.create_task(task)?;

// Track progress
manager.update_status("task-123", TaskStatus::InProgress)?;

// Query tasks
let my_tasks = manager.find_tasks()
    .with_owner("alice")
    .with_status(TaskStatus::Pending)
    .execute()?;

// Mark complete
manager.complete_task("task-123")?;
```

## Dependencies

Currently minimal. When implemented will likely depend on:
- `tokio` — Async runtime
- `serde` — Serialization for persistence
- `sqlx` or equivalent — Task storage
- `anyhow` — Error handling

## Architecture Notes

Tasks should integrate with the RustyCode session system. Task state is persisted to enable resuming work across sessions. Dependency resolution ensures blocking relationships are enforced.

Tasks can be created programmatically (e.g., from a plan) or manually. Notifications alert when dependencies are satisfied and tasks become available.

## Status

This crate is currently a skeleton with intended architecture documented. Implementation is pending based on workflow requirements.

## See Also

- `rustycode-core` — Session lifecycle (tasks live within sessions)
- `rustycode-execution` — Plan execution (plans consist of tasks)
- `rustycode-observability` — Task tracking and metrics
