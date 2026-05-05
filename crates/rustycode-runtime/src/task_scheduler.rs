//! Task Scheduling and Prioritization System
//!
//! This module provides comprehensive task scheduling with:
//! - Priority-based queue management
//! - Deadline-aware scheduling
//! - Resource constraint optimization
//! - Task dependency resolution
//! - Dynamic priority adjustment
//! - Load balancing across agents

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Task priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TaskPriority {
    Critical = 5,
    High = 4,
    Medium = 3,
    Low = 2,
    Background = 1,
}

/// Task status in the scheduler
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum TaskStatus {
    Pending,
    Scheduled,
    InProgress { agent: AgentRole },
    Completed { result: TaskResult },
    Failed { error: String },
    Cancelled,
    Blocked { dependencies: Vec<String> },
}

/// Result of a completed task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub execution_time_ms: u64,
    pub quality_score: f64,
    pub resource_usage: ResourceUsage,
    pub output: String,
    pub completed_at: DateTime<Utc>,
}

/// Resource usage for a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUsage {
    pub cpu_time_ms: u64,
    pub memory_mb: u64,
    pub io_operations: u64,
    pub network_calls: u64,
}

/// Task to be scheduled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub estimated_duration_ms: u64,
    pub deadline: Option<DateTime<Utc>>,
    pub required_agents: Vec<AgentRole>,
    pub dependencies: Vec<String>,
    pub resource_requirements: TaskResourceRequirements,
    pub created_at: DateTime<Utc>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub metadata: HashMap<String, String>,
}

/// Resource requirements for a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskResourceRequirements {
    pub min_cpu_percent: f64,
    pub min_memory_mb: u64,
    pub required_specializations: Vec<String>,
    pub estimated_cost: f64,
}

/// Scheduling decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    pub task_id: String,
    pub assigned_agent: Option<AgentRole>,
    pub scheduled_start_time: DateTime<Utc>,
    pub estimated_completion_time: DateTime<Utc>,
    pub reason: String,
    pub confidence: f64,
}

/// Task queue entry for priority heap
#[derive(Debug, Clone)]
struct TaskQueueEntry {
    task: ScheduledTask,
    priority_score: i64,
}

impl PartialEq for TaskQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority_score == other.priority_score
    }
}

impl Eq for TaskQueueEntry {}

impl PartialOrd for TaskQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for max-heap (higher priority first)
        other.priority_score.cmp(&self.priority_score)
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub max_concurrent_tasks: usize,
    pub enable_deadline_scheduling: bool,
    pub enable_load_balancing: bool,
    pub enable_priority_boost: bool,
    pub time_slice_ms: u64,
    pub queue_size_limit: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 10,
            enable_deadline_scheduling: true,
            enable_load_balancing: true,
            enable_priority_boost: true,
            time_slice_ms: 100,
            queue_size_limit: 1000,
        }
    }
}

/// Main task scheduler
pub struct TaskScheduler {
    task_queue: Arc<RwLock<BinaryHeap<TaskQueueEntry>>>,
    active_tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    completed_tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    agent_workload: Arc<RwLock<HashMap<AgentRole, AgentWorkload>>>,
    config: SchedulerConfig,
    task_counter: Arc<RwLock<u64>>,
}

/// Workload information for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorkload {
    pub agent_role: AgentRole,
    pub active_tasks: usize,
    pub total_tasks_completed: u64,
    pub average_execution_time_ms: f64,
    pub success_rate: f64,
    pub current_cpu_usage: f64,
    pub current_memory_usage_mb: f64,
    pub last_updated: DateTime<Utc>,
}

impl TaskScheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            task_queue: Arc::new(RwLock::new(BinaryHeap::new())),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            completed_tasks: Arc::new(RwLock::new(HashMap::new())),
            agent_workload: Arc::new(RwLock::new(HashMap::new())),
            config,
            task_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Submit a new task for scheduling
    pub async fn submit_task(&self, mut task: ScheduledTask) -> Result<String, String> {
        // Generate task ID if not provided
        if task.id.is_empty() {
            let mut counter = self.task_counter.write().await;
            *counter += 1;
            task.id = format!("task_{}", *counter);
        }

        // Check queue size limit
        {
            let queue = self.task_queue.read().await;
            if queue.len() >= self.config.queue_size_limit {
                return Err("Task queue is full".to_string());
            }
        }

        // Calculate priority score
        let priority_score = self.calculate_priority_score(&task);

        // Add to queue
        let entry = TaskQueueEntry {
            task: task.clone(),
            priority_score,
        };

        let mut queue = self.task_queue.write().await;
        queue.push(entry);

        Ok(task.id)
    }

    /// Calculate priority score for a task
    fn calculate_priority_score(&self, task: &ScheduledTask) -> i64 {
        let mut score = (task.priority as i64) * 1000;

        // Boost for deadline proximity
        if let Some(deadline) = task.deadline {
            let time_until_deadline = (deadline - Utc::now()).num_milliseconds();
            if time_until_deadline < 3600000 {
                // Less than 1 hour
                score += 500;
            } else if time_until_deadline < 86400000 {
                // Less than 1 day
                score += 200;
            }
        }

        // Boost for high priority dependencies
        if !task.dependencies.is_empty() {
            score += 100;
        }

        // Reduce score for long-running tasks
        if task.estimated_duration_ms > 10000 {
            score -= 100;
        } else if task.estimated_duration_ms > 60000 {
            score -= 300;
        }

        // Age boost (older tasks get priority)
        let age_ms = (Utc::now() - task.created_at).num_milliseconds();
        if age_ms > 60000 {
            // More than 1 minute old
            score += age_ms / 1000; // +1 per second of waiting
        }

        score
    }

    /// Schedule next task(s)
    pub async fn schedule_tasks(&self) -> Result<Vec<SchedulingDecision>, String> {
        let mut decisions = Vec::new();

        // Check current workload
        let current_workload = self.get_total_workload().await;

        // Determine how many tasks we can schedule
        let available_slots = self
            .config
            .max_concurrent_tasks
            .saturating_sub(current_workload);

        if available_slots == 0 {
            return Ok(decisions);
        }

        // Get tasks from queue
        let mut tasks_to_schedule = Vec::new();
        {
            let mut queue = self.task_queue.write().await;

            for _ in 0..available_slots {
                if let Some(entry) = queue.pop() {
                    tasks_to_schedule.push(entry.task);
                } else {
                    break;
                }
            }
        }

        // Schedule each task
        for task in tasks_to_schedule {
            if let Some(decision) = self.schedule_single_task(task).await? {
                decisions.push(decision);
            }
        }

        Ok(decisions)
    }

    /// Schedule a single task
    async fn schedule_single_task(
        &self,
        mut task: ScheduledTask,
    ) -> Result<Option<SchedulingDecision>, String> {
        // Check if dependencies are satisfied
        let completed_ids = {
            let completed = self.completed_tasks.read().await;
            completed.keys().cloned().collect::<HashSet<_>>()
        };

        for dep_id in &task.dependencies {
            if !completed_ids.contains(dep_id) {
                // Dependencies not met, return to queue
                task.status = TaskStatus::Blocked {
                    dependencies: task.dependencies.clone(),
                };

                let priority_score = self.calculate_priority_score(&task);
                let entry = TaskQueueEntry {
                    task,
                    priority_score,
                };

                let mut queue = self.task_queue.write().await;
                queue.push(entry);

                return Ok(None);
            }
        }

        // Find best agent
        let assigned_agent = self.select_agent_for_task(&task).await?;

        // Update task status
        task.status = TaskStatus::Scheduled;
        task.scheduled_at = Some(Utc::now());

        // Calculate completion time
        let scheduled_start = Utc::now();
        let estimated_completion =
            scheduled_start + chrono::Duration::milliseconds(task.estimated_duration_ms as i64);

        let decision = SchedulingDecision {
            task_id: task.id.clone(),
            assigned_agent,
            scheduled_start_time: scheduled_start,
            estimated_completion_time: estimated_completion,
            reason: format!("Scheduled with priority {:?}", task.priority),
            confidence: self.calculate_scheduling_confidence(&task, &assigned_agent),
        };

        // Add to active tasks
        {
            let mut active = self.active_tasks.write().await;
            active.insert(task.id.clone(), task);
        }

        // Update agent workload
        if let Some(agent) = assigned_agent {
            self.update_agent_workload(agent, true).await;
        }

        Ok(Some(decision))
    }

    /// Select best agent for a task
    async fn select_agent_for_task(
        &self,
        task: &ScheduledTask,
    ) -> Result<Option<AgentRole>, String> {
        // If task requires specific agents, use them
        if !task.required_agents.is_empty() {
            // Check which required agents are available
            let workload = self.agent_workload.read().await;

            for agent in &task.required_agents {
                if let Some(agent_workload) = workload.get(agent) {
                    if agent_workload.active_tasks < 5 {
                        // Max 5 concurrent tasks per agent
                        return Ok(Some(*agent));
                    }
                } else {
                    // Agent not in workload map, assume available
                    return Ok(Some(*agent));
                }
            }

            return Ok(None); // All required agents busy
        }

        // Use load balancing to select best available agent
        if self.config.enable_load_balancing {
            let workload = self.agent_workload.read().await;

            let mut best_agent = None;
            let mut lowest_load = f64::MAX;

            for (agent, agent_workload) in workload.iter() {
                let load_score = (agent_workload.active_tasks as f64)
                    .mul_add(0.5, agent_workload.current_cpu_usage / 100.0)
                    + agent_workload.current_memory_usage_mb / 1024.0;

                if load_score < lowest_load {
                    lowest_load = load_score;
                    best_agent = Some(*agent);
                }
            }

            Ok(best_agent)
        } else {
            // Simple round-robin (just pick first available)
            Ok(task.required_agents.first().copied())
        }
    }

    /// Calculate scheduling confidence
    fn calculate_scheduling_confidence(
        &self,
        task: &ScheduledTask,
        agent: &Option<AgentRole>,
    ) -> f64 {
        let mut confidence: f64 = 0.8;

        // Boost confidence if agent is assigned
        if agent.is_some() {
            confidence += 0.1;
        }

        // Reduce confidence for complex tasks
        if task.estimated_duration_ms > 30000 {
            confidence -= 0.1;
        }

        // Boost confidence if requirements are met
        if !task.required_agents.is_empty() {
            confidence += 0.05;
        }

        confidence.clamp(0.0, 1.0)
    }

    /// Update agent workload
    async fn update_agent_workload(&self, agent: AgentRole, increment: bool) {
        let mut workload = self.agent_workload.write().await;

        let agent_workload = workload.entry(agent).or_insert_with(|| AgentWorkload {
            agent_role: agent,
            active_tasks: 0,
            total_tasks_completed: 0,
            average_execution_time_ms: 0.0,
            success_rate: 1.0,
            current_cpu_usage: 0.0,
            current_memory_usage_mb: 0.0,
            last_updated: Utc::now(),
        });

        if increment {
            agent_workload.active_tasks = agent_workload.active_tasks.saturating_add(1);
        } else {
            agent_workload.active_tasks = agent_workload.active_tasks.saturating_sub(1);
            agent_workload.total_tasks_completed =
                agent_workload.total_tasks_completed.saturating_add(1);
        }

        agent_workload.last_updated = Utc::now();
    }

    /// Get total workload across all agents
    async fn get_total_workload(&self) -> usize {
        let workload = self.agent_workload.read().await;
        workload.values().map(|w| w.active_tasks).sum()
    }

    /// Complete a task
    pub async fn complete_task(&self, task_id: &str, result: TaskResult) -> Result<(), String> {
        let mut active = self.active_tasks.write().await;

        if let Some(mut task) = active.remove(task_id) {
            // Save agent before moving task
            let agent_to_update = if let TaskStatus::InProgress { agent } = &task.status {
                Some(*agent)
            } else {
                None
            };

            task.status = TaskStatus::Completed {
                result: result.clone(),
            };
            task.completed_at = Some(Utc::now());

            // Move to completed tasks
            let mut completed = self.completed_tasks.write().await;
            completed.insert(task_id.to_string(), task);

            // Update agent workload
            if let Some(agent) = agent_to_update {
                self.update_agent_workload(agent, false).await;
            }

            Ok(())
        } else {
            Err(format!("Task {} not found in active tasks", task_id))
        }
    }

    /// Get task status
    pub async fn task_status(&self, task_id: &str) -> Option<TaskStatus> {
        // Check active tasks
        {
            let active = self.active_tasks.read().await;
            if let Some(task) = active.get(task_id) {
                return Some(task.status.clone());
            }
        }

        // Check completed tasks
        {
            let completed = self.completed_tasks.read().await;
            if let Some(task) = completed.get(task_id) {
                return Some(task.status.clone());
            }
        }

        None
    }

    /// Get queue statistics
    pub async fn queue_statistics(&self) -> QueueStatistics {
        let queue = self.task_queue.read().await;
        let active = self.active_tasks.read().await;
        let completed = self.completed_tasks.read().await;
        let workload = self.agent_workload.read().await;

        let total_agents = workload.len();
        let busy_agents = workload.values().filter(|w| w.active_tasks > 0).count();

        let (critical, high, medium, low, background) = queue.iter().fold(
            (0usize, 0usize, 0usize, 0usize, 0usize),
            |(c, h, m, l, b), entry| match entry.task.priority {
                TaskPriority::Critical => (c.saturating_add(1), h, m, l, b),
                TaskPriority::High => (c, h.saturating_add(1), m, l, b),
                TaskPriority::Medium => (c, h, m.saturating_add(1), l, b),
                TaskPriority::Low => (c, h, m, l.saturating_add(1), b),
                TaskPriority::Background => (c, h, m, l, b.saturating_add(1)),
            },
        );

        QueueStatistics {
            queue_size: queue.len(),
            active_tasks: active.len(),
            completed_tasks: completed.len(),
            total_agents,
            busy_agents,
            critical_tasks: critical,
            high_priority_tasks: high,
            medium_priority_tasks: medium,
            low_priority_tasks: low,
            background_tasks: background,
        }
    }

    /// Rebalance workload across agents
    pub async fn rebalance_workload(&self) -> Result<usize, String> {
        let mut rebalanced = 0;
        let workload = self.agent_workload.read().await;

        // Find overloaded and underloaded agents
        let avg_load = if workload.is_empty() {
            0.0
        } else {
            workload
                .values()
                .map(|w| w.active_tasks as f64)
                .sum::<f64>()
                / workload.len() as f64
        };

        for (_agent, agent_workload) in workload.iter() {
            if agent_workload.active_tasks as f64 > avg_load * 1.5 {
                // Agent is overloaded, consider redistributing
                // In a real system, this would redistribute tasks
                rebalanced += 1;
            }
        }

        Ok(rebalanced)
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatistics {
    pub queue_size: usize,
    pub active_tasks: usize,
    pub completed_tasks: usize,
    pub total_agents: usize,
    pub busy_agents: usize,
    pub critical_tasks: usize,
    pub high_priority_tasks: usize,
    pub medium_priority_tasks: usize,
    pub low_priority_tasks: usize,
    pub background_tasks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Medium);
        assert!(TaskPriority::Medium > TaskPriority::Low);
        assert!(TaskPriority::Low > TaskPriority::Background);
    }

    #[test]
    fn test_task_creation() {
        let task = ScheduledTask {
            id: "test_task".to_string(),
            name: "Test Task".to_string(),
            description: "A test task".to_string(),
            priority: TaskPriority::High,
            status: TaskStatus::Pending,
            estimated_duration_ms: 1000,
            deadline: None,
            required_agents: vec![],
            dependencies: vec![],
            resource_requirements: TaskResourceRequirements {
                min_cpu_percent: 50.0,
                min_memory_mb: 256,
                required_specializations: vec![],
                estimated_cost: 10.0,
            },
            created_at: Utc::now(),
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
            metadata: HashMap::new(),
        };

        assert_eq!(task.id, "test_task");
        assert_eq!(task.priority, TaskPriority::High);
    }

    #[test]
    fn test_priority_score_calculation() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());

        let high_priority_task = ScheduledTask {
            id: "high".to_string(),
            name: "High Priority Task".to_string(),
            description: "".to_string(),
            priority: TaskPriority::High,
            status: TaskStatus::Pending,
            estimated_duration_ms: 1000,
            deadline: None,
            required_agents: vec![],
            dependencies: vec![],
            resource_requirements: TaskResourceRequirements {
                min_cpu_percent: 50.0,
                min_memory_mb: 256,
                required_specializations: vec![],
                estimated_cost: 10.0,
            },
            created_at: Utc::now(),
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
            metadata: HashMap::new(),
        };

        let score = scheduler.calculate_priority_score(&high_priority_task);
        // High priority (4 * 1000 = 4000), may have age boost
        assert!(score >= 4000);
    }

    #[tokio::test]
    async fn test_queue_statistics() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let stats = scheduler.queue_statistics().await;

        assert_eq!(stats.queue_size, 0);
        assert_eq!(stats.active_tasks, 0);
        assert_eq!(stats.completed_tasks, 0);
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn task_priority_serde_roundtrip() {
        let priorities = [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Medium,
            TaskPriority::Low,
            TaskPriority::Background,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let decoded: TaskPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    #[test]
    fn task_status_serde_roundtrip() {
        let statuses = [
            TaskStatus::Pending,
            TaskStatus::Scheduled,
            TaskStatus::InProgress {
                agent: AgentRole::Reviewer,
            },
            TaskStatus::Completed {
                result: TaskResult {
                    task_id: "t1".to_string(),
                    success: true,
                    execution_time_ms: 500,
                    quality_score: 0.95,
                    resource_usage: ResourceUsage {
                        cpu_time_ms: 400,
                        memory_mb: 128,
                        io_operations: 10,
                        network_calls: 5,
                    },
                    output: "done".to_string(),
                    completed_at: Utc::now(),
                },
            },
            TaskStatus::Failed {
                error: "timeout".to_string(),
            },
            TaskStatus::Cancelled,
            TaskStatus::Blocked {
                dependencies: vec!["dep1".to_string()],
            },
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let decoded: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn resource_usage_serde_roundtrip() {
        let ru = ResourceUsage {
            cpu_time_ms: 1000,
            memory_mb: 512,
            io_operations: 42,
            network_calls: 7,
        };
        let json = serde_json::to_string(&ru).unwrap();
        let decoded: ResourceUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ru);
    }

    #[test]
    fn task_result_serde_roundtrip() {
        let tr = TaskResult {
            task_id: "t42".to_string(),
            success: true,
            execution_time_ms: 3000,
            quality_score: 0.88,
            resource_usage: ResourceUsage {
                cpu_time_ms: 2500,
                memory_mb: 256,
                io_operations: 20,
                network_calls: 3,
            },
            output: "All good".to_string(),
            completed_at: Utc::now(),
        };
        let json = serde_json::to_string(&tr).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, tr);
    }

    #[test]
    fn scheduled_task_serde_roundtrip() {
        let task = make_task("t1", TaskPriority::High);
        let json = serde_json::to_string(&task).unwrap();
        let decoded: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "t1");
        assert_eq!(decoded.priority, TaskPriority::High);
        assert_eq!(decoded.status, TaskStatus::Pending);
    }

    #[test]
    fn task_resource_requirements_serde_roundtrip() {
        let req = TaskResourceRequirements {
            min_cpu_percent: 75.0,
            min_memory_mb: 1024,
            required_specializations: vec!["Security".to_string()],
            estimated_cost: 5.0,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: TaskResourceRequirements = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn scheduling_decision_serde_roundtrip() {
        let sd = SchedulingDecision {
            task_id: "t1".to_string(),
            assigned_agent: Some(AgentRole::Reviewer),
            scheduled_start_time: Utc::now(),
            estimated_completion_time: Utc::now(),
            reason: "Best fit".to_string(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&sd).unwrap();
        let decoded: SchedulingDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.task_id, "t1");
        assert!((decoded.confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn scheduler_config_default_values() {
        let config = SchedulerConfig::default();
        assert_eq!(config.max_concurrent_tasks, 10);
        assert!(config.enable_deadline_scheduling);
        assert!(config.enable_load_balancing);
        assert!(config.enable_priority_boost);
        assert_eq!(config.time_slice_ms, 100);
        assert_eq!(config.queue_size_limit, 1000);
    }

    #[test]
    fn scheduler_config_serde_roundtrip() {
        let config = SchedulerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: SchedulerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_concurrent_tasks, config.max_concurrent_tasks);
        assert_eq!(decoded.queue_size_limit, config.queue_size_limit);
    }

    #[test]
    fn agent_workload_serde_roundtrip() {
        let w = AgentWorkload {
            agent_role: AgentRole::Skeptic,
            active_tasks: 3,
            total_tasks_completed: 50,
            average_execution_time_ms: 1200.0,
            success_rate: 0.92,
            current_cpu_usage: 45.0,
            current_memory_usage_mb: 512.0,
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&w).unwrap();
        let decoded: AgentWorkload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_role, AgentRole::Skeptic);
        assert_eq!(decoded.active_tasks, 3);
        assert!((decoded.success_rate - 0.92).abs() < 0.001);
    }

    #[test]
    fn task_priority_ordering_values() {
        assert_eq!(TaskPriority::Critical as i32, 5);
        assert_eq!(TaskPriority::High as i32, 4);
        assert_eq!(TaskPriority::Medium as i32, 3);
        assert_eq!(TaskPriority::Low as i32, 2);
        assert_eq!(TaskPriority::Background as i32, 1);
    }

    // --- Scheduler logic tests ---

    #[tokio::test]
    async fn test_scheduler_new() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let stats = scheduler.queue_statistics().await;
        assert_eq!(stats.queue_size, 0);
    }

    #[test]
    fn test_priority_score_background_lower_than_critical() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let critical = make_task("t1", TaskPriority::Critical);
        let background = make_task("t2", TaskPriority::Background);

        let critical_score = scheduler.calculate_priority_score(&critical);
        let background_score = scheduler.calculate_priority_score(&background);
        assert!(critical_score > background_score);
    }

    // Helper to create a minimal ScheduledTask
    fn make_task(id: &str, priority: TaskPriority) -> ScheduledTask {
        ScheduledTask {
            id: id.to_string(),
            name: format!("Task {}", id),
            description: "Test task".to_string(),
            priority,
            status: TaskStatus::Pending,
            estimated_duration_ms: 1000,
            deadline: None,
            required_agents: vec![],
            dependencies: vec![],
            resource_requirements: TaskResourceRequirements {
                min_cpu_percent: 50.0,
                min_memory_mb: 256,
                required_specializations: vec![],
                estimated_cost: 1.0,
            },
            created_at: Utc::now(),
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
            metadata: HashMap::new(),
        }
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for task_scheduler
    // =========================================================================

    // 1. QueueStatistics serde roundtrip
    #[test]
    fn queue_statistics_serde_roundtrip() {
        let stats = QueueStatistics {
            queue_size: 5,
            active_tasks: 3,
            completed_tasks: 10,
            total_agents: 4,
            busy_agents: 2,
            critical_tasks: 1,
            high_priority_tasks: 2,
            medium_priority_tasks: 1,
            low_priority_tasks: 1,
            background_tasks: 0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: QueueStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.queue_size, 5);
        assert_eq!(decoded.active_tasks, 3);
        assert_eq!(decoded.completed_tasks, 10);
        assert_eq!(decoded.critical_tasks, 1);
    }

    // 2. SchedulerConfig custom values serde roundtrip
    #[test]
    fn scheduler_config_custom_serde_roundtrip() {
        let config = SchedulerConfig {
            max_concurrent_tasks: 4,
            enable_deadline_scheduling: false,
            enable_load_balancing: false,
            enable_priority_boost: false,
            time_slice_ms: 50,
            queue_size_limit: 100,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: SchedulerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_concurrent_tasks, 4);
        assert!(!decoded.enable_deadline_scheduling);
        assert_eq!(decoded.queue_size_limit, 100);
    }

    // 3. ScheduledTask with deadline set serde roundtrip
    #[test]
    fn scheduled_task_with_deadline_serde() {
        let deadline = Utc::now() + chrono::Duration::hours(2);
        let mut task = make_task("dl-task", TaskPriority::Critical);
        task.deadline = Some(deadline);
        let json = serde_json::to_string(&task).unwrap();
        let decoded: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert!(decoded.deadline.is_some());
        assert_eq!(decoded.id, "dl-task");
    }

    // 4. ScheduledTask with metadata serde roundtrip
    #[test]
    fn scheduled_task_with_metadata_serde() {
        let mut task = make_task("meta-task", TaskPriority::Medium);
        task.metadata.insert("env".into(), "production".into());
        task.metadata.insert("team".into(), "backend".into());
        let json = serde_json::to_string(&task).unwrap();
        let decoded: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.metadata.len(), 2);
        assert_eq!(decoded.metadata.get("env").unwrap(), "production");
    }

    // 5. Submit task generates id when empty
    #[tokio::test]
    async fn submit_task_generates_id() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let mut task = make_task("", TaskPriority::High);
        task.id = String::new();
        let id = scheduler.submit_task(task).await.unwrap();
        assert!(id.starts_with("task_"));
    }

    // 6. Schedule tasks returns empty when queue empty
    #[tokio::test]
    async fn schedule_empty_queue_returns_empty() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let decisions = scheduler.schedule_tasks().await.unwrap();
        assert!(decisions.is_empty());
    }

    // 7. Get task status returns None for unknown task
    #[tokio::test]
    async fn get_task_status_unknown_returns_none() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let status = scheduler.task_status("nonexistent").await;
        assert!(status.is_none());
    }

    // 8. Rebalance with empty workload returns 0
    #[tokio::test]
    async fn rebalance_empty_workload() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let rebalanced = scheduler.rebalance_workload().await.unwrap();
        assert_eq!(rebalanced, 0);
    }

    // 9. Priority score with dependencies is higher than without
    #[test]
    fn priority_score_dependencies_boost() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let mut task_with_deps = make_task("dep", TaskPriority::Medium);
        task_with_deps.dependencies = vec!["dep1".into()];
        let task_no_deps = make_task("no_dep", TaskPriority::Medium);
        let score_with = scheduler.calculate_priority_score(&task_with_deps);
        let score_without = scheduler.calculate_priority_score(&task_no_deps);
        assert!(score_with > score_without);
    }

    // 10. Priority score penalty for long estimated duration
    #[test]
    fn priority_score_long_duration_penalty() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default());
        let mut long_task = make_task("long", TaskPriority::Medium);
        long_task.estimated_duration_ms = 120_000;
        let short_task = make_task("short", TaskPriority::Medium);
        let long_score = scheduler.calculate_priority_score(&long_task);
        let short_score = scheduler.calculate_priority_score(&short_task);
        assert!(long_score < short_score);
    }

    // 11. TaskStatus Failed variant serde roundtrip
    #[test]
    fn task_status_failed_serde() {
        let status = TaskStatus::Failed {
            error: "OOM killed".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let decoded: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, status);
    }

    // 12. TaskStatus Cancelled variant serde roundtrip
    #[test]
    fn task_status_cancelled_serde() {
        let json = serde_json::to_string(&TaskStatus::Cancelled).unwrap();
        let decoded: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, TaskStatus::Cancelled);
    }

    // 13. SchedulingDecision with no assigned agent serde roundtrip
    #[test]
    fn scheduling_decision_no_agent_serde() {
        let sd = SchedulingDecision {
            task_id: "t_solo".into(),
            assigned_agent: None,
            scheduled_start_time: Utc::now(),
            estimated_completion_time: Utc::now(),
            reason: "No agent available".into(),
            confidence: 0.3,
        };
        let json = serde_json::to_string(&sd).unwrap();
        let decoded: SchedulingDecision = serde_json::from_str(&json).unwrap();
        assert!(decoded.assigned_agent.is_none());
        assert_eq!(decoded.task_id, "t_solo");
    }

    // 14. TaskResourceRequirements equality check
    #[test]
    fn task_resource_requirements_equality() {
        let req1 = TaskResourceRequirements {
            min_cpu_percent: 50.0,
            min_memory_mb: 256,
            required_specializations: vec!["Security".into()],
            estimated_cost: 10.0,
        };
        let req2 = req1.clone();
        assert_eq!(req1, req2);
    }

    // 15. AgentWorkload default-ish values serde roundtrip
    #[test]
    fn agent_workload_zero_tasks_serde() {
        let w = AgentWorkload {
            agent_role: AgentRole::Reviewer,
            active_tasks: 0,
            total_tasks_completed: 0,
            average_execution_time_ms: 0.0,
            success_rate: 1.0,
            current_cpu_usage: 0.0,
            current_memory_usage_mb: 0.0,
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&w).unwrap();
        let decoded: AgentWorkload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active_tasks, 0);
        assert!((decoded.success_rate - 1.0).abs() < f64::EPSILON);
    }
}
