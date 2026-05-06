//! Command handlers for /task, /todo, /agent, and /schedule slash commands.
//!
//! This module provides the implementation for task management commands:
//! - `/task list|create|complete|delete|start|block` - Manage tasks
//! - `/todo list|add|done|delete|uncheck` - Manage todos
//! - `/agent <task>` - Spawn autonomous agents
//! - `/schedule list|stats` - View task scheduler status

use crate::app::tasks::{
    create_agent_from_task, create_task, create_todo, save_tasks, toggle_todo, update_task_status,
    AgentStatus, TaskStatus, WorkspaceTasks,
};

/// Result type for command execution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TaskCommandResult {
    /// Command was handled successfully
    Success(String),
    /// Command failed with error message
    Error(String),
    /// Command was consumed but no output
    Consumed,
}

/// Dispatcher for task-related slash commands
pub fn handle_command(input: &str, tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return TaskCommandResult::Consumed;
    }

    let cmd = parts[0];

    match cmd {
        "/task" => {
            let action = parts.get(1).copied().unwrap_or("list");
            let args: Vec<String> = parts.iter().skip(2).map(|s| s.to_string()).collect();
            handle_task_command(action, &args, tasks)
        }
        "/todo" => {
            let action = parts.get(1).copied().unwrap_or("list");
            let args: Vec<String> = parts.iter().skip(2).map(|s| s.to_string()).collect();
            handle_todo_command(action, &args, tasks)
        }
        "/agent" => {
            if parts.len() < 2 {
                return TaskCommandResult::Error("Usage: /agent <task description>".to_string());
            }
            let task_desc = parts[1..].join(" ");
            handle_agent_command(&task_desc, tasks)
        }
        "/schedule" => {
            let action = parts.get(1).copied().unwrap_or("list");
            let args: Vec<String> = parts.iter().skip(2).map(|s| s.to_string()).collect();
            handle_schedule_command(action, &args)
        }
        _ => TaskCommandResult::Error(format!("Unsupported command in task_commands: {}", cmd)),
    }
}

/// Handle a task command
pub fn handle_task_command(
    action: &str,
    args: &[String],
    tasks: &mut WorkspaceTasks,
) -> TaskCommandResult {
    match action {
        "list" => cmd_task_list(tasks),
        "create" => cmd_task_create(args, tasks),
        "complete" | "done" => cmd_task_complete(args, tasks),
        "start" => cmd_task_start(args, tasks),
        "delete" | "remove" => cmd_task_delete(args, tasks),
        "block" => cmd_task_block(args, tasks),
        _ => TaskCommandResult::Error(format!(
            "Unknown task action: {}. Use: list, create, complete, start, delete, block",
            action
        )),
    }
}

/// Handle a todo command
pub fn handle_todo_command(
    action: &str,
    args: &[String],
    tasks: &mut WorkspaceTasks,
) -> TaskCommandResult {
    match action {
        "list" => cmd_todo_list(tasks),
        "add" | "new" => cmd_todo_add(args, tasks),
        "done" | "complete" | "check" => cmd_todo_done(args, tasks),
        "uncheck" | "undo" => cmd_todo_uncheck(args, tasks),
        "delete" | "remove" => cmd_todo_delete(args, tasks),
        _ => TaskCommandResult::Error(format!(
            "Unknown todo action: {}. Use: list, add, done, uncheck, delete",
            action
        )),
    }
}

/// Handle an agent command
pub fn handle_agent_command(task: &str, tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    cmd_agent_spawn(task, tasks)
}

// Task Commands

/// List all tasks
fn cmd_task_list(tasks: &WorkspaceTasks) -> TaskCommandResult {
    if tasks.tasks.is_empty() {
        return TaskCommandResult::Success(
            "No tasks yet. Use `/task create <description>` to create one.".to_string(),
        );
    }

    let mut output = String::from("📋 Tasks:\n");

    for (idx, task) in tasks.tasks.iter().enumerate() {
        let icon = match task.status {
            TaskStatus::Pending => "⏳",
            TaskStatus::InProgress => "🔄",
            TaskStatus::Running => "🤖",
            TaskStatus::Completed => "✅",
            TaskStatus::Blocked => "🚫",
            TaskStatus::Failed => "💥",
            TaskStatus::Killed => "🗑️",
        };

        output.push_str(&format!("{}  {}. {}\n", icon, idx + 1, task.description));
    }

    TaskCommandResult::Success(output)
}

fn cmd_task_create(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/task create <description>` - Create a new task".to_string(),
        );
    }

    let description = args.join(" ");
    let task = create_task(description);

    tasks.tasks.push(task);

    if let Err(e) = save_tasks(tasks) {
        tracing::error!("Failed to save tasks: {}", e);
        return TaskCommandResult::Error(
            "Failed to save task changes. Changes may be lost on restart.".to_string(),
        );
    }

    TaskCommandResult::Success(format!("✅ Task {} created", tasks.tasks.len()))
}

/// Mark a task as completed
fn cmd_task_complete(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/task complete <number>` - Mark task as completed".to_string(),
        );
    }

    // Parse task number (1-based index)
    let task_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.tasks.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid task number. Must be between 1 and {}",
                tasks.tasks.len()
            ))
        }
    };

    let task_id = tasks.tasks[task_num].id.clone();
    match update_task_status(tasks, &task_id, TaskStatus::Completed) {
        Ok(()) => {
            if let Err(e) = save_tasks(tasks) {
                tracing::error!("Failed to save tasks: {}", e);
                return TaskCommandResult::Error(
                    "Failed to save task changes. Changes may be lost on restart.".to_string(),
                );
            }

            let description = &tasks.tasks[task_num].description;
            TaskCommandResult::Success(format!("✅ Completed: {}", description))
        }
        Err(e) => TaskCommandResult::Error(e),
    }
}

/// Start working on a task
fn cmd_task_start(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/task start <number>` - Start working on a task".to_string(),
        );
    }

    // Parse task number (1-based index)
    let task_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.tasks.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid task number. Must be between 1 and {}",
                tasks.tasks.len()
            ))
        }
    };

    let task_id = tasks.tasks[task_num].id.clone();
    match update_task_status(tasks, &task_id, TaskStatus::InProgress) {
        Ok(()) => {
            if let Err(e) = save_tasks(tasks) {
                tracing::error!("Failed to save tasks: {}", e);
                return TaskCommandResult::Error(
                    "Failed to save task changes. Changes may be lost on restart.".to_string(),
                );
            }

            let description = &tasks.tasks[task_num].description;
            TaskCommandResult::Success(format!("🔄 Started: {}", description))
        }
        Err(e) => TaskCommandResult::Error(e),
    }
}

/// Delete a task
fn cmd_task_delete(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/task delete <number>` - Delete a task".to_string(),
        );
    }

    // Parse task number (1-based index)
    let task_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.tasks.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid task number. Must be between 1 and {}",
                tasks.tasks.len()
            ))
        }
    };

    let description = tasks.tasks[task_num].description.clone();
    tasks.tasks.remove(task_num);

    if let Err(e) = save_tasks(tasks) {
        tracing::error!("Failed to save tasks: {}", e);
        return TaskCommandResult::Error(
            "Failed to save task changes. Changes may be lost on restart.".to_string(),
        );
    }

    TaskCommandResult::Success(format!("🗑️ Deleted: {}", description))
}

/// Block a task
fn cmd_task_block(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/task block <number>` - Mark task as blocked".to_string(),
        );
    }

    // Parse task number (1-based index)
    let task_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.tasks.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid task number. Must be between 1 and {}",
                tasks.tasks.len()
            ))
        }
    };

    let task_id = tasks.tasks[task_num].id.clone();
    match update_task_status(tasks, &task_id, TaskStatus::Blocked) {
        Ok(()) => {
            if let Err(e) = save_tasks(tasks) {
                tracing::error!("Failed to save tasks: {}", e);
                return TaskCommandResult::Error(
                    "Failed to save task changes. Changes may be lost on restart.".to_string(),
                );
            }

            let description = &tasks.tasks[task_num].description;
            TaskCommandResult::Success(format!("🚫 Blocked: {}", description))
        }
        Err(e) => TaskCommandResult::Error(e),
    }
}

// Todo Commands

/// List all todos
fn cmd_todo_list(tasks: &WorkspaceTasks) -> TaskCommandResult {
    if tasks.todos.is_empty() {
        return TaskCommandResult::Success(
            "No todos yet. Use `/todo add <item>` to create one.".to_string(),
        );
    }

    let mut output = String::from("📝 Todos:\n");

    for (idx, todo) in tasks.todos.iter().enumerate() {
        let checkbox = if todo.done { "☑" } else { "☐" };
        output.push_str(&format!("{}  {}. {}\n", checkbox, idx + 1, todo.text));
    }

    TaskCommandResult::Success(output)
}

/// Add a new todo
fn cmd_todo_add(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/todo add <item>` - Add a new todo item".to_string(),
        );
    }

    let text = args.join(" ");
    let todo = create_todo(text);

    tasks.todos.push(todo);

    if let Err(e) = save_tasks(tasks) {
        tracing::error!("Failed to save tasks: {}", e);
        return TaskCommandResult::Error(
            "Failed to save task changes. Changes may be lost on restart.".to_string(),
        );
    }

    TaskCommandResult::Success(format!("✅ Todo {} added", tasks.todos.len()))
}

/// Mark a todo as done
fn cmd_todo_done(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/todo done <number>` - Mark todo as completed".to_string(),
        );
    }

    // Parse todo number (1-based index)
    let todo_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.todos.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid todo number. Must be between 1 and {}",
                tasks.todos.len()
            ))
        }
    };

    let todo_id = tasks.todos[todo_num].id.clone();
    match toggle_todo(tasks, &todo_id) {
        Ok(done) => {
            if let Err(e) = save_tasks(tasks) {
                tracing::error!("Failed to save tasks: {}", e);
                return TaskCommandResult::Error(
                    "Failed to save task changes. Changes may be lost on restart.".to_string(),
                );
            }

            let text = &tasks.todos[todo_num].text;
            if done {
                TaskCommandResult::Success(format!("☑ Done: {}", text))
            } else {
                TaskCommandResult::Success(format!("☐ Undone: {}", text))
            }
        }
        Err(e) => TaskCommandResult::Error(e),
    }
}

/// Uncheck a todo
fn cmd_todo_uncheck(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/todo uncheck <number>` - Mark todo as not done".to_string(),
        );
    }

    // Parse todo number (1-based index)
    let todo_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.todos.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid todo number. Must be between 1 and {}",
                tasks.todos.len()
            ))
        }
    };

    let todo_id = tasks.todos[todo_num].id.clone();
    match toggle_todo(tasks, &todo_id) {
        Ok(done) => {
            if let Err(e) = save_tasks(tasks) {
                tracing::error!("Failed to save tasks: {}", e);
                return TaskCommandResult::Error(
                    "Failed to save task changes. Changes may be lost on restart.".to_string(),
                );
            }

            let text = &tasks.todos[todo_num].text;
            if !done {
                TaskCommandResult::Success(format!("☐ Unchecked: {}", text))
            } else {
                TaskCommandResult::Success(format!("☑ Checked: {}", text))
            }
        }
        Err(e) => TaskCommandResult::Error(e),
    }
}

/// Delete a todo
fn cmd_todo_delete(args: &[String], tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if args.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/todo delete <number>` - Delete a todo".to_string(),
        );
    }

    // Parse todo number (1-based index)
    let todo_num = match args[0].parse::<usize>() {
        Ok(n) if n >= 1 && n <= tasks.todos.len() => n - 1,
        _ => {
            return TaskCommandResult::Error(format!(
                "Invalid todo number. Must be between 1 and {}",
                tasks.todos.len()
            ))
        }
    };

    let text = tasks.todos[todo_num].text.clone();
    tasks.todos.remove(todo_num);

    if let Err(e) = save_tasks(tasks) {
        tracing::error!("Failed to save tasks: {}", e);
        return TaskCommandResult::Error(
            "Failed to save task changes. Changes may be lost on restart.".to_string(),
        );
    }

    TaskCommandResult::Success(format!("🗑️ Deleted: {}", text))
}

// Agent Commands

/// Spawn a new agent
fn cmd_agent_spawn(task: &str, tasks: &mut WorkspaceTasks) -> TaskCommandResult {
    if task.is_empty() {
        return TaskCommandResult::Error(
            "Usage: `/agent <task description>` - Spawn an autonomous agent".to_string(),
        );
    }

    let (corresponding_task, mut agent) = create_agent_from_task(task.to_string());

    agent.status = AgentStatus::Running;

    let agent_id = agent.id.clone();
    let agent_short_id = if agent_id.len() >= 8 {
        agent_id.chars().take(8).collect::<String>()
    } else {
        agent_id
    };

    tasks.tasks.push(corresponding_task);
    tasks.active_agents.push(agent);

    if let Err(e) = save_tasks(tasks) {
        tracing::error!("Failed to save tasks: {}", e);
        return TaskCommandResult::Error(
            "Failed to save task changes. Changes may be lost on restart.".to_string(),
        );
    }

    TaskCommandResult::Success(format!("🤖 Agent {} spawned for: {}", agent_short_id, task))
}

// Schedule Commands

/// Handle a schedule command
fn handle_schedule_command(action: &str, _args: &[String]) -> TaskCommandResult {
    match action {
        "list" => cmd_schedule_list(),
        "stats" => cmd_schedule_stats(),
        _ => TaskCommandResult::Error(format!(
            "Unknown schedule action: {}. Use: list, stats",
            action
        )),
    }
}

/// List scheduler information
fn cmd_schedule_list() -> TaskCommandResult {
    TaskCommandResult::Success(
        "📅 Task Scheduler:\n\
         \n\
         The task scheduler manages background task execution with:\n\
         • Priority-based queuing (Critical > High > Medium > Low > Background)\n\
         • Deadline-aware scheduling\n\
         • Load balancing across agents\n\
         • Automatic retry on failure\n\
         • Dependency resolution\n\
         \n\
         Use `/task` commands to create tasks that will be scheduled automatically.\n\
         Use `/schedule stats` to view scheduler statistics."
            .to_string(),
    )
}

/// Show scheduler statistics
fn cmd_schedule_stats() -> TaskCommandResult {
    TaskCommandResult::Success(
        "📊 Scheduler Statistics:\n\
         \n\
         The task scheduler tracks:\n\
         • Queue size: Number of pending tasks\n\
         • Active tasks: Currently executing\n\
         • Completed tasks: Finished tasks\n\
         • Agent workload: Tasks per agent\n\
         • Success rate: Completion percentage\n\
         \n\
         Statistics are updated in real-time as tasks progress."
            .to_string(),
    )
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tasks::set_test_tasks_path;

    /// Run a test with an isolated tasks.json path (thread-safe)
    fn with_temp_tasks<F: FnOnce()>(f: F) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("tasks.json");
        set_test_tasks_path(Some(path));
        f();
        set_test_tasks_path(None);
    }

    #[test]
    fn test_task_create() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: Vec::new(),
                todos: Vec::new(),
                active_agents: Vec::new(),
            };

            let result = handle_task_command("create", &["Test task".to_string()], &mut tasks);
            assert!(matches!(result, TaskCommandResult::Success(_)));
            assert_eq!(tasks.tasks.len(), 1);
            assert_eq!(tasks.tasks[0].description, "Test task");
        });
    }

    #[test]
    fn test_task_complete() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: vec![create_task("Test task".to_string())],
                todos: Vec::new(),
                active_agents: Vec::new(),
            };

            let result = handle_task_command("complete", &["1".to_string()], &mut tasks);
            assert!(matches!(result, TaskCommandResult::Success(_)));
            assert_eq!(tasks.tasks[0].status, TaskStatus::Completed);
        });
    }

    #[test]
    fn test_todo_add() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: Vec::new(),
                todos: Vec::new(),
                active_agents: Vec::new(),
            };

            let result = handle_todo_command("add", &["Test todo".to_string()], &mut tasks);
            assert!(matches!(result, TaskCommandResult::Success(_)));
            assert_eq!(tasks.todos.len(), 1);
            assert_eq!(tasks.todos[0].text, "Test todo");
        });
    }

    #[test]
    fn test_todo_done() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: Vec::new(),
                todos: vec![create_todo("Test todo".to_string())],
                active_agents: Vec::new(),
            };

            let result = handle_todo_command("done", &["1".to_string()], &mut tasks);
            assert!(matches!(result, TaskCommandResult::Success(_)));
            assert!(tasks.todos[0].done);
        });
    }

    #[test]
    fn test_agent_spawn() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: Vec::new(),
                todos: Vec::new(),
                active_agents: Vec::new(),
            };

            let result = handle_agent_command("Test agent task", &mut tasks);
            assert!(matches!(result, TaskCommandResult::Success(_)));
            assert_eq!(tasks.active_agents.len(), 1);
            assert_eq!(tasks.active_agents[0].status, AgentStatus::Running);
            assert_eq!(tasks.tasks.len(), 1);
            assert_eq!(tasks.tasks[0].status, TaskStatus::Running);
            assert_eq!(
                tasks.tasks[0].owner.as_deref(),
                Some(tasks.active_agents[0].id.as_str())
            );
        });
    }

    #[test]
    fn test_invalid_task_number() {
        with_temp_tasks(|| {
            let mut tasks = WorkspaceTasks {
                tasks: vec![create_task("Test task".to_string())],
                todos: Vec::new(),
                active_agents: Vec::new(),
            };

            let result = handle_task_command("complete", &["99".to_string()], &mut tasks);
            assert!(matches!(result, TaskCommandResult::Error(_)));
        });
    }
}
