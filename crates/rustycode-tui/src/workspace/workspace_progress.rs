//! Unified workspace progress snapshot and renderer.

use crate::agents::{AgentStatus, AgentTask};
use crate::tasks::{TaskStatus, Todo, WorkspaceTasks};
use chrono::{DateTime, Utc};
use rustycode_orchestration::state_derivation::StateDeriver;
use serde_json::Value;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CountSummary {
    total: usize,
    pending: usize,
    in_progress: usize,
    completed: usize,
    blocked: usize,
    failed: usize,
    running: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Section {
    title: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct WorkspaceProgressSnapshot {
    generated_at: DateTime<Utc>,
    task_summary: CountSummary,
    todo_summary: CountSummary,
    agent_summary: CountSummary,
    mcp_summary: Option<CountSummary>,
    harness_summary: Option<CountSummary>,
    orchestra_summary: Option<Vec<String>>,
    sections: Vec<Section>,
}

impl WorkspaceProgressSnapshot {
    fn render(self) -> String {
        let mut output = String::new();

        let _ = writeln!(output, "Workspace Progress");
        let _ = writeln!(
            output,
            "Updated: {}",
            self.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        output.push('\n');

        let _ = writeln!(
            output,
            "Summary: {} tasks | {} todos | {} agents{}{}",
            self.task_summary.total,
            self.todo_summary.total,
            self.agent_summary.total,
            self.mcp_summary
                .as_ref()
                .map(|m| format!(" | {} MCP servers", m.total))
                .unwrap_or_default(),
            self.harness_summary
                .as_ref()
                .map(|h| format!(" | {} harness tasks", h.total))
                .unwrap_or_default()
        );

        let _ = writeln!(
            output,
            "Open work: {} pending tasks, {} open todos, {} running agents",
            self.task_summary.pending + self.task_summary.in_progress,
            self.todo_summary.pending,
            self.agent_summary.running
        );

        if let Some(summary) = &self.mcp_summary {
            let _ = writeln!(
                output,
                "MCP: {} configured | {} enabled | {} disabled",
                summary.total, summary.completed, summary.failed
            );
        }

        if let Some(summary) = &self.harness_summary {
            let _ = writeln!(
                output,
                "Harness: {} pending | {} in progress | {} done | {} failed",
                summary.pending, summary.in_progress, summary.completed, summary.failed
            );
        }

        if let Some(lines) = &self.orchestra_summary {
            if !lines.is_empty() {
                let _ = writeln!(output, "Orchestra: {}", lines.join(" | "));
            }
        }

        output.push('\n');

        for section in &self.sections {
            let _ = writeln!(output, "{}:", section.title);
            for line in &section.lines {
                let _ = writeln!(output, "  {}", line);
            }
            output.push('\n');
        }

        let _ = writeln!(
            output,
            "Shortcuts: /task list | /todo list | /mcp status | /orchestra progress"
        );

        output.trim_end().to_string()
    }

    fn render_compact(self) -> String {
        let mut output = String::new();

        let _ = writeln!(output, "Workspace");
        let _ = writeln!(
            output,
            "Tasks {} | Todos {} | Agents {}",
            self.task_summary.total, self.todo_summary.total, self.agent_summary.total
        );

        let _ = writeln!(
            output,
            "Open {} task{} | {} todo{} | {} running",
            self.task_summary.pending + self.task_summary.in_progress,
            if self.task_summary.pending + self.task_summary.in_progress == 1 {
                ""
            } else {
                "s"
            },
            self.todo_summary.pending,
            if self.todo_summary.pending == 1 {
                ""
            } else {
                "s"
            },
            self.agent_summary.running
        );

        if let Some(summary) = &self.mcp_summary {
            let _ = writeln!(
                output,
                "MCP {} enabled / {} disabled",
                summary.completed, summary.failed
            );
        }

        if let Some(summary) = &self.harness_summary {
            let _ = writeln!(
                output,
                "Harness {} pending | {} running | {} done",
                summary.pending, summary.in_progress, summary.completed
            );
        }

        if let Some(lines) = &self.orchestra_summary {
            if let Some(first) = lines.first() {
                let _ = writeln!(output, "Orchestra {}", first);
            }
        }

        let top_task = self
            .sections
            .iter()
            .find(|section| section.title == "Tasks")
            .and_then(|section| section.lines.first());
        let top_todo = self
            .sections
            .iter()
            .find(|section| section.title == "Todos")
            .and_then(|section| section.lines.first());
        let top_agent = self
            .sections
            .iter()
            .find(|section| section.title == "Agents")
            .and_then(|section| section.lines.first());

        if top_task.is_some() || top_todo.is_some() || top_agent.is_some() {
            output.push('\n');
        }

        if let Some(line) = top_task {
            let _ = writeln!(output, "Task {}", line);
        }
        if let Some(line) = top_todo {
            let _ = writeln!(output, "Todo {}", line);
        }
        if let Some(line) = top_agent {
            let _ = writeln!(output, "Agent {}", line);
        }

        let _ = writeln!(output, "Use /track full for details");

        output.trim_end().to_string()
    }
}

pub fn render_workspace_progress(
    cwd: &Path,
    tasks: &WorkspaceTasks,
    agents: &[AgentTask],
) -> String {
    collect_workspace_progress(cwd, tasks, agents).render()
}

pub fn render_workspace_progress_compact(
    cwd: &Path,
    tasks: &WorkspaceTasks,
    agents: &[AgentTask],
) -> String {
    collect_workspace_progress(cwd, tasks, agents).render_compact()
}

fn collect_workspace_progress(
    cwd: &Path,
    tasks: &WorkspaceTasks,
    agents: &[AgentTask],
) -> WorkspaceProgressSnapshot {
    let generated_at = Utc::now();

    let task_summary = summarize_tasks(tasks);
    let todo_summary = summarize_todos(&tasks.todos);
    let agent_summary = summarize_agents(agents);
    let mcp_summary = summarize_mcp(cwd);
    let harness_summary = summarize_harness(cwd);
    let orchestra_summary = summarize_orchestra(cwd);

    let mut sections = Vec::new();

    if !tasks.tasks.is_empty() {
        sections.push(Section {
            title: "Tasks".to_string(),
            lines: format_task_lines(&tasks.tasks),
        });
    }

    if !tasks.todos.is_empty() {
        sections.push(Section {
            title: "Todos".to_string(),
            lines: format_todo_lines(&tasks.todos),
        });
    }

    if !agents.is_empty() {
        sections.push(Section {
            title: "Agents".to_string(),
            lines: format_agent_lines(agents),
        });
    }

    if let Some(summary) = &mcp_summary {
        if summary.total > 0 {
            sections.push(Section {
                title: "MCP".to_string(),
                lines: format_mcp_lines(cwd),
            });
        }
    }

    if let Some(summary) = &harness_summary {
        if summary.total > 0 {
            sections.push(Section {
                title: "Harness".to_string(),
                lines: format_harness_lines(cwd, summary),
            });
        }
    }

    if let Some(lines) = &orchestra_summary {
        if !lines.is_empty() {
            sections.push(Section {
                title: "Orchestra".to_string(),
                lines: lines.clone(),
            });
        }
    }

    WorkspaceProgressSnapshot {
        generated_at,
        task_summary,
        todo_summary,
        agent_summary,
        mcp_summary,
        harness_summary,
        orchestra_summary,
        sections,
    }
}

fn summarize_tasks(tasks: &WorkspaceTasks) -> CountSummary {
    let mut summary = CountSummary {
        total: tasks.tasks.len(),
        ..Default::default()
    };

    for task in &tasks.tasks {
        match &task.status {
            TaskStatus::Pending => summary.pending += 1,
            TaskStatus::InProgress => summary.in_progress += 1,
            TaskStatus::Completed => summary.completed += 1,
            TaskStatus::Blocked => summary.blocked += 1,
            TaskStatus::Running => summary.running += 1,
            TaskStatus::Failed | TaskStatus::Killed => summary.failed += 1,
        }
    }

    summary
}

fn summarize_todos(todos: &[Todo]) -> CountSummary {
    let mut summary = CountSummary {
        total: todos.len(),
        ..Default::default()
    };

    for todo in todos {
        if todo.done {
            summary.completed += 1;
        } else {
            summary.pending += 1;
        }
    }

    summary
}

fn summarize_agents(agents: &[AgentTask]) -> CountSummary {
    let mut summary = CountSummary {
        total: agents.len(),
        ..Default::default()
    };

    for agent in agents {
        match &agent.status {
            AgentStatus::Pending => summary.pending += 1,
            AgentStatus::Running => summary.running += 1,
            AgentStatus::Completed => summary.completed += 1,
            AgentStatus::Failed => summary.failed += 1,
        }
    }

    summary
}

fn summarize_mcp(cwd: &Path) -> Option<CountSummary> {
    let mut configs = rustycode_mcp::McpConfigFile::load_from_standard_locations();

    // Also check for project-local mcp.json in cwd
    let local_mcp = cwd.join("mcp.json");
    if let Ok(content) = std::fs::read_to_string(&local_mcp) {
        if let Ok(config) = serde_json::from_str::<rustycode_mcp::McpConfigFile>(&content) {
            configs.push((local_mcp, config));
        }
    }
    let mut summary = CountSummary::default();

    for (_path, config) in configs {
        for server in config.servers.values() {
            summary.total += 1;
            if server.enabled {
                summary.completed += 1;
            } else {
                summary.failed += 1;
            }
        }
    }

    if summary.total == 0 {
        None
    } else {
        Some(summary)
    }
}

fn summarize_harness(cwd: &Path) -> Option<CountSummary> {
    let harness_path = cwd.join(".harness").join("harness-tasks.json");
    let content = std::fs::read_to_string(&harness_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    let tasks = json.get("tasks")?.as_array()?;

    let mut summary = CountSummary {
        total: tasks.len(),
        ..Default::default()
    };

    for task in tasks {
        match task.get("status").and_then(|s| s.as_str()) {
            Some("pending") => summary.pending += 1,
            Some("in_progress") => summary.in_progress += 1,
            Some("completed") => summary.completed += 1,
            Some("failed") => summary.failed += 1,
            Some("blocked") => summary.blocked += 1,
            _ => {}
        }
    }

    Some(summary)
}

fn summarize_orchestra(cwd: &Path) -> Option<Vec<String>> {
    let orchestra_dir = cwd.join(".orchestra");
    if !orchestra_dir.exists() {
        return None;
    }

    let deriver = StateDeriver::new(cwd.to_path_buf());
    let state = deriver.derive_state().ok()?;

    let mut lines = Vec::new();
    lines.push(format!(
        "Milestone {}",
        state
            .active_milestone
            .as_ref()
            .map(|m| format!("{}: {}", m.id, m.title))
            .unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!(
        "Slice {}",
        state
            .active_slice
            .as_ref()
            .map(|s| format!("{}: {}", s.id, s.title))
            .unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!(
        "Task {}",
        state
            .active_task
            .as_ref()
            .map(|t| format!("{}: {}", t.id, t.title))
            .unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!("Phase {:?}", state.phase));

    Some(lines)
}

fn format_task_lines(tasks: &[crate::tasks::Task]) -> Vec<String> {
    let mut sorted: Vec<_> = tasks.iter().collect();
    sorted.sort_by_key(|task| task_status_rank(&task.status));
    sorted
        .into_iter()
        .take(6)
        .map(|task| {
            format!(
                "{} {} - {}",
                task_status_icon(&task.status),
                task.id,
                task.description
            )
        })
        .collect()
}

fn format_todo_lines(todos: &[Todo]) -> Vec<String> {
    todos
        .iter()
        .filter(|todo| !todo.done)
        .take(6)
        .map(|todo| format!("☐ {} - {}", todo.id, todo.text))
        .collect()
}

fn format_agent_lines(agents: &[AgentTask]) -> Vec<String> {
    let mut sorted: Vec<_> = agents.iter().collect();
    sorted.sort_by_key(|agent| agent_status_rank(&agent.status));
    sorted
        .into_iter()
        .take(6)
        .map(|agent| {
            let status = match &agent.status {
                AgentStatus::Pending => "pending",
                AgentStatus::Running => "running",
                AgentStatus::Completed => "done",
                AgentStatus::Failed => "failed",
            };
            format!("#{} {} - {}", agent.id, status, agent.task)
        })
        .collect()
}

fn format_mcp_lines(cwd: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let mut configs = rustycode_mcp::McpConfigFile::load_from_standard_locations();

    // Also check for project-local mcp.json in cwd
    let local_mcp = cwd.join("mcp.json");
    if let Ok(content) = std::fs::read_to_string(&local_mcp) {
        if let Ok(config) = serde_json::from_str::<rustycode_mcp::McpConfigFile>(&content) {
            configs.push((local_mcp, config));
        }
    }

    if configs.is_empty() {
        lines.push("No MCP config files found".to_string());
        return lines;
    }

    for (path, config) in configs {
        lines.push(format!(
            "{} ({} server{})",
            path.display(),
            config.servers.len(),
            if config.servers.len() == 1 { "" } else { "s" }
        ));
        for (server_id, server) in config.servers.iter().take(4) {
            lines.push(format!(
                "- {} [{}]",
                server_id,
                if server.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
        }
    }

    lines
}

fn format_harness_lines(cwd: &Path, summary: &CountSummary) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} total | {} pending | {} in progress | {} done | {} failed",
        summary.total, summary.pending, summary.in_progress, summary.completed, summary.failed
    ));

    let progress_path = cwd.join(".harness").join("harness-progress.txt");
    if let Ok(content) = std::fs::read_to_string(progress_path) {
        if let Some(last) = content.lines().rev().find(|line| !line.trim().is_empty()) {
            lines.push(format!("Latest: {}", last));
        }
    }

    lines
}

fn task_status_rank(status: &TaskStatus) -> u8 {
    match status {
        TaskStatus::Running => 0,
        TaskStatus::InProgress => 1,
        TaskStatus::Pending => 2,
        TaskStatus::Blocked => 3,
        TaskStatus::Failed => 4,
        TaskStatus::Killed => 5,
        TaskStatus::Completed => 6,
    }
}

fn agent_status_rank(status: &AgentStatus) -> u8 {
    match status {
        AgentStatus::Running => 0,
        AgentStatus::Pending => 1,
        AgentStatus::Completed => 2,
        AgentStatus::Failed => 3,
    }
}

fn task_status_icon(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "⏳",
        TaskStatus::InProgress => "🔄",
        TaskStatus::Completed => "✅",
        TaskStatus::Blocked => "🚫",
        TaskStatus::Running => "🤖",
        TaskStatus::Failed => "💥",
        TaskStatus::Killed => "🗑️",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{Task, Todo};
    use std::time::SystemTime;

    fn sample_task(id: &str, description: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            description: description.to_string(),
            status,
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: None,
        }
    }

    fn sample_todo(id: &str, text: &str, done: bool) -> Todo {
        Todo {
            id: id.to_string(),
            text: text.to_string(),
            done,
            created_at: SystemTime::now(),
        }
    }

    fn sample_agent(id: usize, task: &str, status: AgentStatus) -> AgentTask {
        let mut agent = AgentTask::new(
            id,
            rustycode_runtime::multi_agent::AgentRole::Reviewer,
            task.to_string(),
        );
        agent.status = status;
        agent
    }

    #[test]
    fn render_workspace_progress_includes_core_counts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cwd = temp.path();
        std::fs::create_dir_all(cwd.join(".harness")).expect("harness dir");
        std::fs::write(
            cwd.join(".harness").join("harness-tasks.json"),
            r#"{"tasks":[{"id":"task-1","status":"completed"},{"id":"task-2","status":"pending"}]}"#,
        )
        .expect("harness file");
        std::fs::write(
            cwd.join("mcp.json"),
            r#"{"servers":{"filesystem":{"server_id":"fs","name":"filesystem","enabled":true,"command":"npx","args":[]}}}"#,
        )
        .expect("mcp file");

        let live_agents = vec![sample_agent(1, "Track progress", AgentStatus::Running)];

        let tasks = WorkspaceTasks {
            tasks: vec![
                sample_task("1", "Write tests", TaskStatus::InProgress),
                sample_task("2", "Ship feature", TaskStatus::Pending),
            ],
            todos: vec![
                sample_todo("todo-1", "Update docs", false),
                sample_todo("todo-2", "Clean up", true),
            ],
            active_agents: vec![],
        };

        let output = render_workspace_progress(cwd, &tasks, &live_agents);

        assert!(output.contains("Workspace Progress"));
        assert!(output.contains("2 tasks"));
        assert!(output.contains("2 todos"));
        assert!(output.contains("1 agents"));
        assert!(output.contains("MCP"));
        assert!(output.contains("Harness"));
        assert!(output.contains("Tasks:"));
        assert!(output.contains("Todos:"));
        assert!(output.contains("Agents:"));
    }

    #[test]
    fn render_workspace_progress_compact_is_small() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cwd = temp.path();
        let tasks = WorkspaceTasks {
            tasks: vec![sample_task("1", "Write tests", TaskStatus::Pending)],
            todos: vec![sample_todo("todo-1", "Update docs", false)],
            active_agents: vec![],
        };

        let output = render_workspace_progress_compact(cwd, &tasks, &[]);

        assert!(output.contains("Workspace"));
        assert!(output.contains("Use /track full for details"));
        assert!(output.lines().count() <= 8);
    }

    #[test]
    fn summarize_workspace_progress_handles_empty_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cwd = temp.path();
        let tasks = WorkspaceTasks {
            tasks: vec![],
            todos: vec![],
            active_agents: vec![],
        };

        let report = collect_workspace_progress(cwd, &tasks, &[]);

        assert_eq!(report.task_summary.total, 0);
        assert_eq!(report.todo_summary.total, 0);
        assert_eq!(report.agent_summary.total, 0);
        assert_eq!(report.task_summary.pending, 0);
        assert_eq!(report.todo_summary.pending, 0);
        assert_eq!(report.agent_summary.running, 0);
    }
}
