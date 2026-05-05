use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "low" => Self::Low,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub session_id: String,
    pub project_id: String,
    pub content: String,
    pub status: TodoStatus,
    pub priority: Priority,
    pub position: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Blocked,
    Running,
    Killed,
}

impl TaskStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Running => "running",
            Self::Killed => "killed",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "blocked" => Self::Blocked,
            "running" => Self::Running,
            "killed" => Self::Killed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub session_id: Option<String>,
    pub description: String,
    pub status: TaskStatus,
    pub owner: Option<String>,
    pub dependencies: Vec<String>,
    pub output: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub created_at: DateTime<Utc>,
}

impl Storage {
    pub fn insert_project(&self, project: &Project) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert or ignore into projects (id, path, created_at) values (?1, ?2, ?3)",
            params![project.id, project.path, project.created_at.to_rfc3339()],
        )
        .with_context(|| "Failed to insert project")?;
        Ok(())
    }

    pub fn project_by_path(&self, path: &str) -> Result<Option<Project>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare("select id, path, created_at from projects where path = ?1")?;
        let project = stmt
            .query_row(params![path], |row| {
                Ok(Project {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    created_at: parse_datetime(row.get::<_, String>(2)?),
                })
            })
            .ok();
        Ok(project)
    }

    pub fn or_create_project(&self, path: &str) -> Result<Project> {
        if let Some(project) = self.project_by_path(path)? {
            return Ok(project);
        }
        let project = Project {
            id: format!("proj-{}", ulid::Ulid::new()),
            path: path.to_string(),
            created_at: Utc::now(),
        };
        self.insert_project(&project)?;
        Ok(project)
    }

    pub fn replace_todos(&self, session_id: &str, todos: &[TodoItem]) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "delete from todos where session_id = ?1",
            params![session_id],
        )?;
        for todo in todos {
            tx.execute(
                "insert into todos (id, session_id, project_id, content, status, priority, position, created_at, updated_at)
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    todo.id,
                    todo.session_id,
                    todo.project_id,
                    todo.content,
                    todo.status.as_str(),
                    todo.priority.as_str(),
                    todo.position,
                    todo.created_at.to_rfc3339(),
                    todo.updated_at.to_rfc3339(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn todos(&self, session_id: &str) -> Result<Vec<TodoItem>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, project_id, content, status, priority, position, created_at, updated_at
             from todos where session_id = ?1 order by position",
        )?;
        let todos = stmt
            .query_map(params![session_id], |row| {
                Ok(TodoItem {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project_id: row.get(2)?,
                    content: row.get(3)?,
                    status: TodoStatus::from_str_lossy(&row.get::<_, String>(4)?),
                    priority: Priority::from_str_lossy(&row.get::<_, String>(5)?),
                    position: row.get(6)?,
                    created_at: parse_datetime(row.get::<_, String>(7)?),
                    updated_at: parse_datetime(row.get::<_, String>(8)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "Failed to query todos")?;
        Ok(todos)
    }

    pub fn todos_by_project(&self, project_id: &str) -> Result<Vec<TodoItem>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, project_id, content, status, priority, position, created_at, updated_at
             from todos where project_id = ?1 order by session_id, position",
        )?;
        let todos = stmt
            .query_map(params![project_id], |row| {
                Ok(TodoItem {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project_id: row.get(2)?,
                    content: row.get(3)?,
                    status: TodoStatus::from_str_lossy(&row.get::<_, String>(4)?),
                    priority: Priority::from_str_lossy(&row.get::<_, String>(5)?),
                    position: row.get(6)?,
                    created_at: parse_datetime(row.get::<_, String>(7)?),
                    updated_at: parse_datetime(row.get::<_, String>(8)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "Failed to query todos by project")?;
        Ok(todos)
    }

    pub fn insert_task(&self, task: &Task) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert into tasks (id, project_id, session_id, description, status, owner, dependencies, output, created_at, updated_at, started_at, completed_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                task.id,
                task.project_id,
                task.session_id,
                task.description,
                task.status.as_str(),
                task.owner,
                serde_json::to_string(&task.dependencies)?,
                task.output,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
                task.started_at.map(|t| t.to_rfc3339()),
                task.completed_at.map(|t| t.to_rfc3339()),
            ],
        )
        .with_context(|| "Failed to insert task")?;
        Ok(())
    }

    pub fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now().to_rfc3339();
        let completed_at = matches!(
            status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Killed
        )
        .then_some(&now);
        conn.execute(
            "update tasks set status = ?1, updated_at = ?2, completed_at = ?3 where id = ?4",
            params![status.as_str(), now, completed_at, task_id],
        )
        .with_context(|| "Failed to update task status")?;
        Ok(())
    }

    pub fn update_task_owner(&self, task_id: &str, owner: Option<&str>) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "update tasks set owner = ?1, updated_at = ?2 where id = ?3",
            params![owner, Utc::now().to_rfc3339(), task_id],
        )
        .with_context(|| "Failed to update task owner")?;
        Ok(())
    }

    pub fn update_task_output(&self, task_id: &str, output: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "update tasks set output = ?1, updated_at = ?2 where id = ?3",
            params![output, Utc::now().to_rfc3339(), task_id],
        )
        .with_context(|| "Failed to update task output")?;
        Ok(())
    }

    pub fn task(&self, task_id: &str) -> Result<Option<Task>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, project_id, session_id, description, status, owner, dependencies, output, created_at, updated_at, started_at, completed_at
             from tasks where id = ?1",
        )?;
        let task = stmt
            .query_row(params![task_id], |row| {
                let deps_str: String = row.get(6)?;
                let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
                Ok(Task {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    description: row.get(3)?,
                    status: TaskStatus::from_str_lossy(&row.get::<_, String>(4)?),
                    owner: row.get(5)?,
                    dependencies: deps,
                    output: row.get(7)?,
                    created_at: parse_datetime(row.get::<_, String>(8)?),
                    updated_at: parse_datetime(row.get::<_, String>(9)?),
                    started_at: row.get::<_, Option<String>>(10)?.map(parse_datetime),
                    completed_at: row.get::<_, Option<String>>(11)?.map(parse_datetime),
                })
            })
            .ok();
        Ok(task)
    }

    pub fn tasks_by_project(&self, project_id: &str) -> Result<Vec<Task>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, project_id, session_id, description, status, owner, dependencies, output, created_at, updated_at, started_at, completed_at
             from tasks where project_id = ?1 order by created_at",
        )?;
        let tasks = stmt
            .query_map(params![project_id], |row| {
                let deps_str: String = row.get(6)?;
                let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
                Ok(Task {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    description: row.get(3)?,
                    status: TaskStatus::from_str_lossy(&row.get::<_, String>(4)?),
                    owner: row.get(5)?,
                    dependencies: deps,
                    output: row.get(7)?,
                    created_at: parse_datetime(row.get::<_, String>(8)?),
                    updated_at: parse_datetime(row.get::<_, String>(9)?),
                    started_at: row.get::<_, Option<String>>(10)?.map(parse_datetime),
                    completed_at: row.get::<_, Option<String>>(11)?.map(parse_datetime),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "Failed to query tasks by project")?;
        Ok(tasks)
    }

    pub fn tasks_by_owner(&self, owner: &str) -> Result<Vec<Task>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, project_id, session_id, description, status, owner, dependencies, output, created_at, updated_at, started_at, completed_at
             from tasks where owner = ?1 order by created_at",
        )?;
        let tasks = stmt
            .query_map(params![owner], |row| {
                let deps_str: String = row.get(6)?;
                let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
                Ok(Task {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    description: row.get(3)?,
                    status: TaskStatus::from_str_lossy(&row.get::<_, String>(4)?),
                    owner: row.get(5)?,
                    dependencies: deps,
                    output: row.get(7)?,
                    created_at: parse_datetime(row.get::<_, String>(8)?),
                    updated_at: parse_datetime(row.get::<_, String>(9)?),
                    started_at: row.get::<_, Option<String>>(10)?.map(parse_datetime),
                    completed_at: row.get::<_, Option<String>>(11)?.map(parse_datetime),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "Failed to query tasks by owner")?;
        Ok(tasks)
    }

    pub fn claim_task(&self, task_id: &str, owner: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let rows = conn.execute(
            "update tasks set owner = ?1, status = 'running', updated_at = ?2, started_at = ?2 where id = ?3 and owner is null",
            params![owner, Utc::now().to_rfc3339(), task_id],
        )
        .with_context(|| "Failed to claim task")?;
        Ok(rows > 0)
    }
}

fn parse_datetime(s: String) -> DateTime<Utc> {
    s.parse().unwrap_or_else(|e| {
        tracing::warn!("Failed to parse datetime '{s}': {e}, using current time");
        Utc::now()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_storage() -> Storage {
        let dir = std::env::temp_dir().join(format!(
            "rustycode-task-store-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.db");
        let storage = Storage::open(&path).unwrap();
        storage.migrate().unwrap();
        storage
    }

    fn test_project() -> Project {
        Project {
            id: "test-project".to_string(),
            path: "/tmp/test".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn project_create_and_retrieve() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        let found = storage.project_by_path("/tmp/test").unwrap().unwrap();
        assert_eq!(found.id, project.id);
    }

    #[test]
    fn get_or_create_project_idempotent() {
        let storage = test_storage();
        let p1 = storage.or_create_project("/tmp/test").unwrap();
        let p2 = storage.or_create_project("/tmp/test").unwrap();
        assert_eq!(p1.id, p2.id);
    }

    #[test]
    fn todo_replace_and_retrieve() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        let todos = vec![
            TodoItem {
                id: "1".into(),
                session_id: "sess-1".into(),
                project_id: project.id.clone(),
                content: "Task A".into(),
                status: TodoStatus::Pending,
                priority: Priority::High,
                position: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            TodoItem {
                id: "2".into(),
                session_id: "sess-1".into(),
                project_id: project.id.clone(),
                content: "Task B".into(),
                status: TodoStatus::Completed,
                priority: Priority::Medium,
                position: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ];

        storage.replace_todos("sess-1", &todos).unwrap();

        let retrieved = storage.todos("sess-1").unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0].content, "Task A");
        assert_eq!(retrieved[1].status, TodoStatus::Completed);
    }

    #[test]
    fn todo_replace_clears_old() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        let first = vec![TodoItem {
            id: "1".into(),
            session_id: "sess-1".into(),
            project_id: project.id.clone(),
            content: "Old".into(),
            status: TodoStatus::Pending,
            priority: Priority::Medium,
            position: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];
        storage.replace_todos("sess-1", &first).unwrap();

        let second: Vec<TodoItem> = vec![];
        storage.replace_todos("sess-1", &second).unwrap();

        let retrieved = storage.todos("sess-1").unwrap();
        assert!(retrieved.is_empty());
    }

    #[test]
    fn task_crud_lifecycle() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        let task = Task {
            id: "task-1".into(),
            project_id: project.id.clone(),
            session_id: Some("sess-1".into()),
            description: "Implement auth".into(),
            status: TaskStatus::Pending,
            owner: None,
            dependencies: vec![],
            output: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            completed_at: None,
        };
        storage.insert_task(&task).unwrap();

        let found = storage.task("task-1").unwrap().unwrap();
        assert_eq!(found.description, "Implement auth");
        assert_eq!(found.status, TaskStatus::Pending);

        storage
            .update_task_status("task-1", TaskStatus::Completed)
            .unwrap();
        let found = storage.task("task-1").unwrap().unwrap();
        assert_eq!(found.status, TaskStatus::Completed);
        assert!(found.completed_at.is_some());
    }

    #[test]
    fn task_claim_atomic() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        let task = Task {
            id: "task-1".into(),
            project_id: project.id.clone(),
            session_id: None,
            description: "Build feature".into(),
            status: TaskStatus::Pending,
            owner: None,
            dependencies: vec![],
            output: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            completed_at: None,
        };
        storage.insert_task(&task).unwrap();

        let claimed = storage.claim_task("task-1", "agent-1").unwrap();
        assert!(claimed);

        let claimed_again = storage.claim_task("task-1", "agent-2").unwrap();
        assert!(!claimed_again);

        let found = storage.task("task-1").unwrap().unwrap();
        assert_eq!(found.owner.unwrap(), "agent-1");
        assert_eq!(found.status, TaskStatus::Running);
    }

    #[test]
    fn tasks_by_project_and_owner() {
        let storage = test_storage();
        let project = test_project();
        storage.insert_project(&project).unwrap();

        for i in 0..3 {
            let task = Task {
                id: format!("task-{i}"),
                project_id: project.id.clone(),
                session_id: None,
                description: format!("Task {i}"),
                status: TaskStatus::Pending,
                owner: if i == 0 { Some("agent-a".into()) } else { None },
                dependencies: vec![],
                output: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                started_at: None,
                completed_at: None,
            };
            storage.insert_task(&task).unwrap();
        }

        let by_project = storage.tasks_by_project(&project.id).unwrap();
        assert_eq!(by_project.len(), 3);

        let by_owner = storage.tasks_by_owner("agent-a").unwrap();
        assert_eq!(by_owner.len(), 1);
        assert_eq!(by_owner[0].id, "task-0");
    }
}
