//! Task execution dashboard display for the TUI.
//!
//! Ported from goose's `task_execution_display` pattern. Provides formatted
//! display of task execution progress with status icons, timing, and output
//! previews. Designed to be rendered inline in the message stream.
//!
//! # Example
//!
//! ```ignore
//! let dashboard = TaskDashboard::new(&tasks, &agents);
//! let display = dashboard.render();
//! // Returns formatted string like:
//! // ━━ Task Dashboard ━━━━━━━━━━━━━━━━━
//! // ⏳ 2 pending | 🏃 1 running | ✅ 3 done
//! // 🏃 Implement auth [00:12] 💬 Writing middleware...
//! // ✅ Setup database [00:05]
//! // ⏳ Write tests
//! ```

use crate::tasks::{ActiveAgent, AgentStatus, Task, TaskStatus};

/// Dashboard for displaying task execution status.
pub struct TaskDashboard<'a> {
    tasks: &'a [Task],
    agents: &'a [ActiveAgent],
}

impl<'a> TaskDashboard<'a> {
    pub fn new(tasks: &'a [Task], agents: &'a [ActiveAgent]) -> Self {
        Self { tasks, agents }
    }

    /// Render the dashboard as a formatted string.
    pub fn render(&self) -> String {
        let stats = self.compute_stats();
        if stats.total == 0 {
            return String::new();
        }

        let mut display = String::new();

        // Header
        display.push_str("━━ Task Dashboard ━━━━━━━━━━━━━━━━━━\n");

        // Progress bar
        let progress = self.render_progress_bar(&stats);
        display.push_str(&format!("{}\n", progress));

        // Summary line
        display.push_str(&format!(
            "⏳ {} pending | 🏃 {} running | ✅ {} done | ❌ {} blocked | 💥 {} failed\n",
            stats.pending, stats.running, stats.completed, stats.blocked, stats.failed
        ));

        display.push('\n');

        // Sort tasks: running first, then pending, then completed, then blocked
        let mut sorted_tasks: Vec<_> = self.tasks.iter().collect();
        sorted_tasks.sort_by(|a, b| {
            let order = |s: &TaskStatus| match s {
                TaskStatus::InProgress => 0,
                TaskStatus::Running => 1,
                TaskStatus::Pending => 2,
                TaskStatus::Completed => 3,
                TaskStatus::Blocked => 4,
                TaskStatus::Failed => 5,
                TaskStatus::Killed => 6,
            };
            order(&a.status).cmp(&order(&b.status))
        });

        for task in &sorted_tasks {
            display.push_str(&self.render_task(task));
        }

        // Active agents section
        if !self.agents.is_empty() {
            display.push_str("━━ Agents ━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            for agent in self.agents {
                display.push_str(&self.render_agent(agent));
            }
        }

        display
    }

    /// Render a compact one-line summary for the status bar.
    pub fn render_compact(&self) -> String {
        let stats = self.compute_stats();
        if stats.total == 0 {
            return "no tasks".to_string();
        }

        let mut parts = Vec::new();
        if stats.running > 0 {
            parts.push(format!("🏃{}", stats.running));
        }
        if stats.pending > 0 {
            parts.push(format!("⏳{}", stats.pending));
        }
        if stats.completed > 0 {
            parts.push(format!("✅{}", stats.completed));
        }
        if stats.blocked > 0 {
            parts.push(format!("❌{}", stats.blocked));
        }
        parts.join(" ")
    }

    fn compute_stats(&self) -> TaskStats {
        let mut stats = TaskStats {
            total: self.tasks.len(),
            ..Default::default()
        };
        for task in self.tasks {
            match task.status {
                TaskStatus::Pending => stats.pending += 1,
                TaskStatus::InProgress => stats.running += 1,
                TaskStatus::Completed => stats.completed += 1,
                TaskStatus::Blocked => stats.blocked += 1,
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Failed => stats.failed += 1,
                TaskStatus::Killed => stats.blocked += 1,
            }
        }
        stats
    }

    fn render_progress_bar(&self, stats: &TaskStats) -> String {
        if stats.total == 0 {
            return String::new();
        }
        let width: usize = 30;
        let done = stats.completed;
        let filled = (done * width).checked_div(stats.total).unwrap_or(0);
        let empty = width - filled;

        let bar: String = "█".repeat(filled) + &"░".repeat(empty);
        let pct = (done * 100).checked_div(stats.total).unwrap_or(0);
        format!("[{}] {}%", bar, pct)
    }

    fn render_task(&self, task: &Task) -> String {
        let status_icon = match task.status {
            TaskStatus::Pending => "⏳",
            TaskStatus::InProgress => "🏃",
            TaskStatus::Running => "🤖",
            TaskStatus::Completed => "✅",
            TaskStatus::Blocked => "❌",
            TaskStatus::Failed => "💥",
            TaskStatus::Killed => "🗑️",
        };

        let mut line = format!("{} {}", status_icon, task.description);

        // Add elapsed time for in-progress tasks
        if task.status == TaskStatus::InProgress || task.status == TaskStatus::Running {
            if let Ok(duration) = task.created_at.elapsed() {
                let secs = duration.as_secs();
                line.push_str(&format!(" [{:02}:{:02}]", secs / 60, secs % 60));
            }
        }

        line.push('\n');

        if task.status == TaskStatus::InProgress || task.status == TaskStatus::Running {
            for agent in self.agents {
                if agent.task == task.id {
                    let agent_status = match agent.status {
                        AgentStatus::Starting => "starting",
                        AgentStatus::Running => "running",
                        AgentStatus::Completed => "done",
                        AgentStatus::Failed => "failed",
                        AgentStatus::Killed => "killed",
                    };
                    line.push_str(&format!("  🤖 agent: {}\n", agent_status));
                }
            }
        }

        line
    }

    fn render_agent(&self, agent: &ActiveAgent) -> String {
        let status_icon = match agent.status {
            AgentStatus::Starting => "🔄",
            AgentStatus::Running => "🏃",
            AgentStatus::Completed => "✅",
            AgentStatus::Failed => "❌",
            AgentStatus::Killed => "💀",
        };

        let mut line = format!("{} {} - {}", status_icon, agent.task, agent.id);

        if let Ok(duration) = agent.created_at.elapsed() {
            let secs = duration.as_secs();
            line.push_str(&format!(" [{:02}:{:02}]", secs / 60, secs % 60));
        }

        line.push('\n');
        line
    }
}

#[derive(Default)]
struct TaskStats {
    total: usize,
    pending: usize,
    running: usize,
    completed: usize,
    blocked: usize,
    failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn make_task(id: &str, desc: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            description: desc.to_string(),
            status,
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: None,
        }
    }

    fn make_agent(id: &str, task: &str, status: AgentStatus) -> ActiveAgent {
        ActiveAgent {
            id: id.to_string(),
            task: task.to_string(),
            status,
            created_at: SystemTime::now(),
        }
    }

    #[test]
    fn test_empty_dashboard() {
        let dashboard = TaskDashboard::new(&[], &[]);
        assert!(dashboard.render().is_empty());
        assert_eq!(dashboard.render_compact(), "no tasks");
    }

    #[test]
    fn test_single_pending_task() {
        let tasks = vec![make_task("1", "Write tests", TaskStatus::Pending)];
        let dashboard = TaskDashboard::new(&tasks, &[]);
        let rendered = dashboard.render();
        assert!(rendered.contains("⏳ Write tests"));
        assert!(rendered.contains("1 pending"));
        let compact = dashboard.render_compact();
        assert!(compact.contains("⏳1"));
    }

    #[test]
    fn test_mixed_status_tasks() {
        let tasks = vec![
            make_task("1", "Setup DB", TaskStatus::Completed),
            make_task("2", "Auth module", TaskStatus::InProgress),
            make_task("3", "Write tests", TaskStatus::Pending),
            make_task("4", "Deploy", TaskStatus::Blocked),
        ];
        let dashboard = TaskDashboard::new(&tasks, &[]);
        let rendered = dashboard.render();

        // Running tasks should appear first
        let running_pos = rendered.find("🏃 Auth module").unwrap();
        let pending_pos = rendered.find("⏳ Write tests").unwrap();
        let done_pos = rendered.find("✅ Setup DB").unwrap();
        let blocked_pos = rendered.find("❌ Deploy").unwrap();

        assert!(running_pos < pending_pos);
        assert!(pending_pos < done_pos);
        assert!(done_pos < blocked_pos);
    }

    #[test]
    fn test_progress_bar() {
        let tasks = vec![
            make_task("1", "A", TaskStatus::Completed),
            make_task("2", "B", TaskStatus::Completed),
            make_task("3", "C", TaskStatus::Pending),
            make_task("4", "D", TaskStatus::Pending),
        ];
        let dashboard = TaskDashboard::new(&tasks, &[]);
        let rendered = dashboard.render();
        // 50% complete (2/4)
        assert!(rendered.contains("50%"));
        assert!(rendered.contains("█"));
        assert!(rendered.contains("░"));
    }

    #[test]
    fn test_agent_display() {
        let tasks = vec![make_task("1", "Build feature", TaskStatus::InProgress)];
        let agents = vec![make_agent("agent-1", "1", AgentStatus::Running)];
        let dashboard = TaskDashboard::new(&tasks, &agents);
        let rendered = dashboard.render();
        assert!(rendered.contains("🤖 agent: running"));
        assert!(rendered.contains("Agents"));
    }

    #[test]
    fn test_compact_all_completed() {
        let tasks = vec![
            make_task("1", "A", TaskStatus::Completed),
            make_task("2", "B", TaskStatus::Completed),
        ];
        let dashboard = TaskDashboard::new(&tasks, &[]);
        let compact = dashboard.render_compact();
        assert!(compact.contains("✅2"));
        assert!(!compact.contains("⏳"));
    }

    #[test]
    fn test_in_progress_elapsed_time() {
        let mut task = make_task("1", "Working", TaskStatus::InProgress);
        // Use a past time to show elapsed
        task.created_at = SystemTime::now() - Duration::from_secs(125);
        let tasks = vec![task];
        let dashboard = TaskDashboard::new(&tasks, &[]);
        let rendered = dashboard.render();
        assert!(rendered.contains("[02:05]"));
    }
}
