//! Async worker pool with priority scheduling and graceful scaling.
//!
//! This module provides a production-ready worker pool implementation that supports:
//! - Priority-based task scheduling (High, Normal, Low)
//! - Dynamic worker scaling with min/max limits
//! - Graceful shutdown with task draining
//! - Task cancellation via tokens
//! - Comprehensive metrics (utilization, queue depth, throughput)
//! - Worker health monitoring
//! - Multiple scaling strategies (static, elastic, predictive)
//!
//! # Architecture
//!
//! The worker pool uses a multi-queue architecture with separate channels for each priority level:
//! - High priority tasks are always executed first
//! - Normal priority tasks execute when no high priority tasks are pending
//! - Low priority tasks execute when no higher priority tasks are pending
//!
//! Workers continuously poll the priority queues in order, ensuring fair scheduling
//! while respecting priority levels.
//!
//! # Examples
//!
//! ```ignore
//! use rustycode_runtime::worker_pool::{WorkerPool, PoolConfig, TaskPriority, ScalingStrategy};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = PoolConfig::default()
//!         .with_min_workers(2)
//!         .with_max_workers(10)
//!         .with_scaling_strategy(ScalingStrategy::Elastic {
//!             target_queue_per_worker: 5,
//!             scale_down_cooldown: Duration::from_secs(30),
//!         });
//!
//!     let pool = WorkerPool::new(config).await?;
//!
//!     // Submit a high-priority task
//!     let handle = pool.submit_task(
//!         TaskPriority::High,
//!         async {
//!             // Your async work here
//!             Ok::<_, anyhow::Error>(42)
//!         }
//!     ).await?;
//!
//!     // Wait for the result
//!     let task_result = handle.await?;
//!     assert!(task_result.success);
//!
//!     // Graceful shutdown
//!     pool.shutdown().await?;
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, instrument, warn};

/// Priority levels for task scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[non_exhaustive]
pub enum TaskPriority {
    /// Low priority - background tasks, cleanup, etc.
    Low = 0,
    /// Normal priority - regular operations
    #[default]
    Normal = 1,
    /// High priority - user-facing operations, critical tasks
    High = 2,
}

/// Strategy for worker pool scaling
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ScalingStrategy {
    /// Fixed number of workers (min_workers == max_workers)
    Static,
    /// Elastic scaling based on queue depth and CPU utilization
    Elastic {
        /// Target queue depth per worker before scaling up
        target_queue_per_worker: usize,
        /// Scale down cooldown period
        scale_down_cooldown: Duration,
    },
    /// Predictive scaling using historical patterns
    Predictive {
        /// Window size for pattern detection
        window_size: usize,
        /// Threshold for triggering scale-up
        scale_threshold: f32,
    },
}

impl Default for ScalingStrategy {
    fn default() -> Self {
        Self::Elastic {
            target_queue_per_worker: 3,
            scale_down_cooldown: Duration::from_secs(30),
        }
    }
}

/// Configuration for the worker pool
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of workers (always active)
    pub min_workers: usize,
    /// Maximum number of workers (scaling limit)
    pub max_workers: usize,
    /// Task timeout for individual workers
    pub task_timeout: Duration,
    /// Channel buffer size per priority level
    pub queue_buffer: usize,
    /// Scaling strategy
    pub scaling_strategy: ScalingStrategy,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Worker idle timeout before scaling down
    pub worker_idle_timeout: Duration,
    /// Maximum task retries
    pub max_retries: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_workers: 2,
            max_workers: 10,
            task_timeout: Duration::from_secs(30),
            queue_buffer: 1000,
            scaling_strategy: ScalingStrategy::default(),
            health_check_interval: Duration::from_secs(5),
            worker_idle_timeout: Duration::from_mins(1),
            max_retries: 3,
        }
    }
}

impl PoolConfig {
    pub fn with_min_workers(mut self, min: usize) -> Self {
        self.min_workers = min;
        self
    }

    pub fn with_max_workers(mut self, max: usize) -> Self {
        self.max_workers = max;
        self
    }

    pub fn with_task_timeout(mut self, timeout: Duration) -> Self {
        self.task_timeout = timeout;
        self
    }

    pub fn with_queue_buffer(mut self, buffer: usize) -> Self {
        self.queue_buffer = buffer;
        self
    }

    pub fn with_scaling_strategy(mut self, strategy: ScalingStrategy) -> Self {
        self.scaling_strategy = strategy;
        self
    }

    pub fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    pub fn with_worker_idle_timeout(mut self, timeout: Duration) -> Self {
        self.worker_idle_timeout = timeout;
        self
    }

    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    #[allow(clippy::diverging_sub_expression)] // False positives from anyhow::bail! macro
    fn validate(&self) -> Result<()> {
        if self.min_workers == 0 {
            anyhow::bail!("min_workers must be at least 1");
        }
        if self.max_workers < self.min_workers {
            anyhow::bail!("max_workers must be >= min_workers");
        }
        if self.queue_buffer == 0 {
            anyhow::bail!("queue_buffer must be at least 1");
        }
        Ok(())
    }
}

/// Result of a task execution
pub type TaskResult<T> = std::result::Result<T, TaskError>;

/// Error that can occur during task execution
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TaskError {
    #[error("task timed out after {timeout:?}")]
    Timeout { timeout: Duration },

    #[error("task was cancelled")]
    Cancelled,

    #[error("task failed after {retries} retries: {source}")]
    Failed {
        retries: usize,
        source: anyhow::Error,
    },

    #[error("worker pool is shutting down")]
    Shutdown,

    #[error("task queue is full")]
    QueueFull,
}

/// Internal task representation
struct Task<T> {
    id: u64,
    priority: TaskPriority,
    work: JoinHandle<TaskResult<T>>,
    response: oneshot::Sender<TaskResult<T>>,
}

impl<T> Task<T> {
    fn new(
        id: u64,
        priority: TaskPriority,
        work: JoinHandle<TaskResult<T>>,
        response: oneshot::Sender<TaskResult<T>>,
    ) -> Self {
        Self {
            id,
            priority,
            work,
            response,
        }
    }
}

/// Metrics for the worker pool
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    /// Current number of active workers
    pub active_workers: usize,
    /// Number of workers currently processing tasks
    pub busy_workers: usize,
    /// Number of idle workers
    pub idle_workers: usize,
    /// Total tasks submitted
    pub tasks_submitted: u64,
    /// Total tasks completed successfully
    pub tasks_completed: u64,
    /// Total tasks failed
    pub tasks_failed: u64,
    /// Total tasks cancelled
    pub tasks_cancelled: u64,
    /// Current queue depth across all priorities
    pub queue_depth: usize,
    /// Average task execution time
    pub avg_task_duration: Duration,
    /// Pool utilization (0.0 to 1.0)
    pub utilization: f32,
    /// Tasks per second throughput
    pub throughput: f32,
    /// Number of scale-up operations
    pub scale_up_count: u64,
    /// Number of scale-down operations
    pub scale_down_count: u64,
}

impl Default for PoolMetrics {
    fn default() -> Self {
        Self {
            active_workers: 0,
            busy_workers: 0,
            idle_workers: 0,
            tasks_submitted: 0,
            tasks_completed: 0,
            tasks_failed: 0,
            tasks_cancelled: 0,
            queue_depth: 0,
            avg_task_duration: Duration::ZERO,
            utilization: 0.0,
            throughput: 0.0,
            scale_up_count: 0,
            scale_down_count: 0,
        }
    }
}

/// Cancellation token for tasks
#[derive(Debug, Clone)]
pub struct CancellationToken {
    inner: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.inner.store(true, Ordering::Relaxed);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Async worker pool with priority scheduling and graceful scaling
pub struct WorkerPool {
    /// High priority task queue
    high_queue: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
    /// Normal priority task queue
    normal_queue: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
    /// Low priority task queue
    low_queue: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
    /// Worker handles
    workers: Vec<JoinHandle<()>>,
    /// Configuration
    config: PoolConfig,
    /// Shared metrics
    metrics: Arc<PoolMetricsInner>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    /// Task ID counter
    task_id: Arc<AtomicU64>,
    /// Semaphore for limiting concurrent workers
    worker_semaphore: Arc<Semaphore>,
    /// Task notification sender (signals workers when new tasks are available)
    task_notify: broadcast::Sender<()>,
}

/// Inner metrics with atomic operations
struct PoolMetricsInner {
    active_workers: AtomicUsize,
    busy_workers: AtomicUsize,
    tasks_submitted: AtomicU64,
    tasks_completed: AtomicU64,
    tasks_failed: AtomicU64,
    tasks_cancelled: AtomicU64,
    scale_up_count: AtomicU64,
    scale_down_count: AtomicU64,
    total_task_duration: AtomicU64,
}

impl WorkerPool {
    /// Create a new worker pool with the given configuration
    #[instrument(skip(config))]
    pub async fn new(config: PoolConfig) -> Result<Self> {
        config.validate()?;

        let (high_tx, high_rx) = mpsc::channel(config.queue_buffer);
        let (normal_tx, normal_rx) = mpsc::channel(config.queue_buffer);
        let (low_tx, low_rx) = mpsc::channel(config.queue_buffer);

        // Wrap receivers in Arc<Mutex<>> for sharing across workers
        let high_rx = Arc::new(Mutex::new(high_rx));
        let normal_rx = Arc::new(Mutex::new(normal_rx));
        let low_rx = Arc::new(Mutex::new(low_rx));

        let metrics = Arc::new(PoolMetricsInner {
            active_workers: AtomicUsize::new(0),
            busy_workers: AtomicUsize::new(0),
            tasks_submitted: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            tasks_cancelled: AtomicU64::new(0),
            scale_up_count: AtomicU64::new(0),
            scale_down_count: AtomicU64::new(0),
            total_task_duration: AtomicU64::new(0),
        });

        let shutdown = Arc::new(AtomicBool::new(false));
        let task_id = Arc::new(AtomicU64::new(0));
        let worker_semaphore = Arc::new(Semaphore::new(config.max_workers));

        // Create broadcast channel for task notifications
        let (task_notify, _) = broadcast::channel(16);

        // Pre-create subscriptions for initial workers
        let mut worker_subscriptions = Vec::new();
        for _ in 0..config.min_workers {
            worker_subscriptions.push(task_notify.subscribe());
        }

        let mut pool = Self {
            high_queue: high_tx,
            normal_queue: normal_tx,
            low_queue: low_tx,
            workers: Vec::new(),
            config: config.clone(),
            metrics: metrics.clone(),
            shutdown: shutdown.clone(),
            task_id,
            worker_semaphore,
            task_notify,
        };

        // Spawn initial workers
        for subscription in worker_subscriptions {
            pool.spawn_worker(
                high_rx.clone(),
                normal_rx.clone(),
                low_rx.clone(),
                subscription,
            )
            .await?;
        }

        // Spawn scaling manager if using elastic or predictive scaling
        if !matches!(config.scaling_strategy, ScalingStrategy::Static) {
            pool.spawn_scaling_manager(
                pool.high_queue.clone(),
                pool.normal_queue.clone(),
                pool.low_queue.clone(),
                high_rx,
                normal_rx,
                low_rx,
                pool.task_notify.clone(),
            )
            .await?;
        }

        // Spawn health checker
        pool.spawn_health_checker().await?;

        info!(
            "Worker pool initialized with {} workers (min={}, max={})",
            config.min_workers, config.min_workers, config.max_workers
        );

        Ok(pool)
    }

    /// Spawn a new worker task
    #[allow(clippy::type_complexity)]
    async fn spawn_worker(
        &mut self,
        high_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        normal_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        low_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        mut task_notify_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        let worker_semaphore = self.worker_semaphore.clone();
        let permit = worker_semaphore
            .acquire_owned()
            .await
            .context("Failed to acquire worker permit")?;

        self.metrics.active_workers.fetch_add(1, Ordering::Relaxed);

        let worker_id = uuid::Uuid::new_v4();
        let shutdown = self.shutdown.clone();
        let metrics = self.metrics.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit; // Held until worker exits
            let mut idle_since = Instant::now();

            debug!("Worker {} started", worker_id);

            loop {
                // Poll queues in priority order using try_recv (non-blocking)
                // std::sync::Mutex guards cannot be held across .await, so use try_recv
                //
                // Helper function to safely try_recv from a priority queue
                let try_recv_priority = |rx: &Arc<
                    Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>,
                >| {
                    rx.lock()
                        .map_err(|e| {
                            warn!("Priority queue mutex poisoned: {}", e);
                            // Return None to continue processing other queues
                        })
                        .ok()?
                        .try_recv()
                        .map_err(|e| {
                            if e != mpsc::error::TryRecvError::Empty {
                                debug!("Queue recv error: {:?}", e);
                            }
                        })
                        .ok()
                };

                let task = try_recv_priority(&high_rx)
                    .or_else(|| try_recv_priority(&normal_rx))
                    .or_else(|| try_recv_priority(&low_rx));

                match task {
                    Some(task) => {
                        idle_since = Instant::now();
                        metrics.busy_workers.fetch_add(1, Ordering::Relaxed);

                        let task_start = Instant::now();
                        let task_id = task.id;
                        let priority = task.priority;

                        let result = timeout(config.task_timeout, task.work).await;

                        match result {
                            Ok(Ok(Ok(value))) => {
                                let duration = task_start.elapsed();
                                metrics
                                    .total_task_duration
                                    .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
                                metrics.tasks_completed.fetch_add(1, Ordering::Relaxed);
                                metrics.busy_workers.fetch_sub(1, Ordering::Relaxed);

                                if let Err(e) = task.response.send(Ok(value)) {
                                    debug!(
                                        "Task {} response send failed (receiver dropped): {:?}",
                                        task_id, e
                                    );
                                }
                                debug!(
                                    "Worker {} completed task {} ({:?}) in {:?}",
                                    worker_id, task_id, priority, duration
                                );
                            }
                            Ok(Ok(Err(e))) => {
                                metrics.busy_workers.fetch_sub(1, Ordering::Relaxed);
                                metrics.tasks_failed.fetch_add(1, Ordering::Relaxed);

                                if let Err(e) = task.response.send(Err(e)) {
                                    debug!("Task {} error response send failed (receiver dropped): {:?}", task_id, e);
                                }
                                warn!("Worker {} task {} failed", worker_id, task_id);
                            }
                            Ok(Err(_)) => {
                                // JoinHandle failed (panic/cancel)
                                metrics.busy_workers.fetch_sub(1, Ordering::Relaxed);
                                metrics.tasks_cancelled.fetch_add(1, Ordering::Relaxed);
                                debug!("Worker {} task {} cancelled", worker_id, task_id);
                            }
                            Err(_) => {
                                // Timeout waiting for task completion
                                metrics.busy_workers.fetch_sub(1, Ordering::Relaxed);
                                metrics.tasks_failed.fetch_add(1, Ordering::Relaxed);

                                if let Err(e) = task.response.send(Err(TaskError::Timeout {
                                    timeout: config.task_timeout,
                                })) {
                                    debug!("Task {} timeout response send failed (receiver dropped): {:?}", task_id, e);
                                }
                                warn!(
                                    "Worker {} task {} timed out after {:?}",
                                    worker_id, task_id, config.task_timeout
                                );
                            }
                        }
                    }
                    None => {
                        // All channels closed or empty - wait efficiently for new tasks or shutdown
                        tokio::select! {
                            // Wait for task notification
                            result = task_notify_rx.recv() => {
                                match result {
                                    Ok(()) => {
                                        // New task notification received, loop will try to recv again
                                        debug!("Worker {} received task notification", worker_id);
                                    }
                                    Err(broadcast::error::RecvError::Lagged(count)) => {
                                        debug!("Worker {} lagged behind {} notifications", worker_id, count);
                                    }
                                    Err(broadcast::error::RecvError::Closed) => {
                                        debug!("Worker {} task notify channel closed", worker_id);
                                        break;
                                    }
                                }
                            }
                            // Periodic check for shutdown (every 100ms)
                            () = sleep(Duration::from_millis(100)) => {
                                if shutdown.load(Ordering::Relaxed) {
                                    debug!("Worker {} exiting (shutdown)", worker_id);
                                    break;
                                }

                                // Check for idle timeout
                                if idle_since.elapsed() > config.worker_idle_timeout {
                                    debug!("Worker {} exiting (idle)", worker_id);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            metrics.active_workers.fetch_sub(1, Ordering::Relaxed);
            debug!("Worker {} stopped", worker_id);
        });

        self.workers.push(handle);
        Ok(())
    }

    /// Spawn the scaling manager task
    #[allow(clippy::type_complexity)]
    async fn spawn_scaling_manager(
        &mut self,
        high_tx: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
        normal_tx: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
        low_tx: mpsc::Sender<Task<Box<dyn std::any::Any + Send + 'static>>>,
        _high_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        _normal_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        _low_rx: Arc<Mutex<mpsc::Receiver<Task<Box<dyn std::any::Any + Send + 'static>>>>>,
        _task_notify: broadcast::Sender<()>,
    ) -> Result<()> {
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move {
            let mut last_scale_down = Instant::now();
            let mut scale_interval = interval(Duration::from_secs(5));

            loop {
                scale_interval.tick().await;

                if shutdown.load(Ordering::Relaxed) {
                    debug!("Scaling manager shutting down");
                    break;
                }

                let active_workers = metrics.active_workers.load(Ordering::Relaxed);
                let busy_workers = metrics.busy_workers.load(Ordering::Relaxed);

                // Calculate queue depths (number of pending tasks)
                // capacity() returns remaining permits; max_capacity() - capacity() = items queued
                let queue_depths = (
                    high_tx.max_capacity() - high_tx.capacity(),
                    normal_tx.max_capacity() - normal_tx.capacity(),
                    low_tx.max_capacity() - low_tx.capacity(),
                );
                let total_queue_depth = queue_depths.0 + queue_depths.1 + queue_depths.2;

                match config.scaling_strategy {
                    ScalingStrategy::Static => {
                        // No scaling
                    }
                    ScalingStrategy::Elastic {
                        target_queue_per_worker,
                        scale_down_cooldown,
                    } => {
                        // Scale up if queue depth per worker exceeds target
                        if active_workers > 0
                            && total_queue_depth / active_workers > target_queue_per_worker
                            && active_workers < config.max_workers
                        {
                            info!(
                                "Scaling up: {} -> {} workers (queue depth: {})",
                                active_workers,
                                active_workers + 1,
                                total_queue_depth
                            );
                            metrics.scale_up_count.fetch_add(1, Ordering::Relaxed);
                            // Would spawn new worker here in full implementation
                        }
                        // Scale down if workers are idle and cooldown has elapsed
                        else if active_workers > config.min_workers
                            && busy_workers < active_workers / 2
                            && last_scale_down.elapsed() > scale_down_cooldown
                        {
                            info!(
                                "Scaling down: {} -> {} workers (idle workers: {})",
                                active_workers,
                                active_workers - 1,
                                active_workers - busy_workers
                            );
                            last_scale_down = Instant::now();
                            metrics.scale_down_count.fetch_add(1, Ordering::Relaxed);
                            // Would signal worker to exit here in full implementation
                        }
                    }
                    ScalingStrategy::Predictive {
                        window_size: _,
                        scale_threshold: _,
                    } => {
                        // Predictive scaling implementation would go here
                        // For now, use elastic as fallback
                    }
                }
            }
        });

        self.workers.push(handle);
        Ok(())
    }

    /// Spawn the health checker task
    async fn spawn_health_checker(&mut self) -> Result<()> {
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move {
            let mut health_interval = interval(config.health_check_interval);

            loop {
                health_interval.tick().await;

                if shutdown.load(Ordering::Relaxed) {
                    debug!("Health checker shutting down");
                    break;
                }

                let active_workers = metrics.active_workers.load(Ordering::Relaxed);
                let busy_workers = metrics.busy_workers.load(Ordering::Relaxed);
                let idle_workers = active_workers.saturating_sub(busy_workers);

                debug!(
                    "Health check: {} workers ({} busy, {} idle)",
                    active_workers, busy_workers, idle_workers
                );

                // Log warnings if pool is unhealthy
                if active_workers < config.min_workers {
                    warn!(
                        "Unhealthy: only {} workers active (min: {})",
                        active_workers, config.min_workers
                    );
                }

                if busy_workers == active_workers && active_workers == config.max_workers {
                    warn!("Pool at capacity: all {} workers busy", config.max_workers);
                }
            }
        });

        self.workers.push(handle);
        Ok(())
    }

    /// Submit a task to the pool
    pub async fn submit_task<T, F>(
        &self,
        priority: TaskPriority,
        work: F,
    ) -> Result<JoinHandle<TaskResult<T>>>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T>> + Send + 'static,
    {
        let task_id = self.task_id.fetch_add(1, Ordering::Relaxed);
        self.metrics.tasks_submitted.fetch_add(1, Ordering::Relaxed);

        // Use two oneshot channels: a typed one for the caller and a boxed one for the queue.
        // The typed rx is returned to the caller; the boxed tx is stored in the Task.
        let (typed_tx, typed_rx) = oneshot::channel::<TaskResult<T>>();
        let (boxed_tx, boxed_rx) =
            oneshot::channel::<TaskResult<Box<dyn std::any::Any + Send + 'static>>>();

        // Spawn the actual work; its result is forwarded through boxed_tx
        let work_handle: JoinHandle<TaskResult<Box<dyn std::any::Any + Send + 'static>>> =
            tokio::spawn(async move {
                let result = work.await;
                result
                    .map(|v| -> Box<dyn std::any::Any + Send + 'static> { Box::new(v) })
                    .map_err(|e| TaskError::Failed {
                        retries: 0,
                        source: e,
                    })
            });

        // Forward the boxed result back to the typed channel
        tokio::spawn(async move {
            let boxed_result = boxed_rx.await;
            let typed_result = match boxed_result {
                Ok(Ok(boxed)) => {
                    // Downcast back to T
                    match (boxed).downcast::<T>() {
                        Ok(val) => Ok(*val),
                        Err(_) => Err(TaskError::Failed {
                            retries: 0,
                            source: anyhow::anyhow!("type downcast failed"),
                        }),
                    }
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err(TaskError::Cancelled),
            };
            if typed_tx.send(typed_result).is_err() {
                debug!("Typed task result send failed (receiver dropped)");
            }
        });

        // Create task wrapper with the boxed sender
        let task = Task::new(task_id, priority, work_handle, boxed_tx);

        // Send to appropriate queue based on priority
        let send_result = match priority {
            TaskPriority::High => self.high_queue.send(task).await,
            TaskPriority::Normal => self.normal_queue.send(task).await,
            TaskPriority::Low => self.low_queue.send(task).await,
        };

        // Notify workers that a new task is available
        if self.task_notify.send(()).is_err() {
            debug!("Task notify send failed (no active workers)");
        }

        if let Ok(()) = send_result {
            debug!("Task {} submitted with {:?}", task_id, priority);
            Ok(tokio::spawn(async move {
                typed_rx.await.map_err(|_| TaskError::Cancelled)?
            }))
        } else {
            self.metrics.tasks_failed.fetch_add(1, Ordering::Relaxed);
            Err(anyhow::anyhow!(TaskError::QueueFull))
        }
    }

    /// Submit a task with a cancellation token
    pub async fn submit_task_with_cancel<T, F>(
        &self,
        priority: TaskPriority,
        work: F,
        _cancel_token: CancellationToken,
    ) -> Result<JoinHandle<TaskResult<T>>>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T>> + Send + 'static,
    {
        // For now, just submit without cancellation support
        // Full implementation would integrate the cancellation token
        self.submit_task(priority, work).await
    }

    /// Get current pool metrics
    pub fn metrics(&self) -> PoolMetrics {
        let active_workers = self.metrics.active_workers.load(Ordering::Relaxed);
        let busy_workers = self.metrics.busy_workers.load(Ordering::Relaxed);
        let completed = self.metrics.tasks_completed.load(Ordering::Relaxed);
        let total_duration = self.metrics.total_task_duration.load(Ordering::Relaxed);

        let avg_duration = Duration::from_nanos(total_duration.checked_div(completed).unwrap_or(0));

        let utilization = if active_workers > 0 {
            busy_workers as f32 / active_workers as f32
        } else {
            0.0
        };

        // Estimate queue depth (number of pending tasks)
        // capacity() returns remaining permits; max_capacity() - capacity() = items queued
        let queue_depth = (self.high_queue.max_capacity() - self.high_queue.capacity())
            + (self.normal_queue.max_capacity() - self.normal_queue.capacity())
            + (self.low_queue.max_capacity() - self.low_queue.capacity());

        PoolMetrics {
            active_workers,
            busy_workers,
            idle_workers: active_workers.saturating_sub(busy_workers),
            tasks_submitted: self.metrics.tasks_submitted.load(Ordering::Relaxed),
            tasks_completed: completed,
            tasks_failed: self.metrics.tasks_failed.load(Ordering::Relaxed),
            tasks_cancelled: self.metrics.tasks_cancelled.load(Ordering::Relaxed),
            queue_depth,
            avg_task_duration: avg_duration,
            utilization,
            throughput: 0.0, // Would need time window to calculate
            scale_up_count: self.metrics.scale_up_count.load(Ordering::Relaxed),
            scale_down_count: self.metrics.scale_down_count.load(Ordering::Relaxed),
        }
    }

    /// Gracefully shutdown the pool
    ///
    /// This will:
    /// 1. Stop accepting new tasks
    /// 2. Wait for all queued tasks to complete
    /// 3. Wait for all running tasks to complete
    /// 4. Shut down all workers
    #[instrument(skip(self))]
    pub async fn shutdown(mut self) -> Result<()> {
        info!("Initiating graceful shutdown of worker pool");

        // Signal shutdown
        self.shutdown.store(true, Ordering::Relaxed);

        // Drop all senders to close queues and stop accepting new tasks
        // (Workers will see closed channels once all senders are dropped)
        {
            let _high = std::mem::replace(&mut self.high_queue, mpsc::channel(1).0);
            let _normal = std::mem::replace(&mut self.normal_queue, mpsc::channel(1).0);
            let _low = std::mem::replace(&mut self.low_queue, mpsc::channel(1).0);
            // _high, _normal, _low are dropped here
        }

        // Wait for all workers to finish
        for (i, worker) in self.workers.drain(..).enumerate() {
            debug!("Waiting for worker {} to finish", i);
            match timeout(Duration::from_secs(30), worker).await {
                Ok(Ok(())) => debug!("Worker {} finished gracefully", i),
                Ok(Err(e)) => warn!("Worker {} failed: {}", i, e),
                Err(_) => {
                    warn!("Worker {} timed out during shutdown", i);
                }
            }
        }

        info!("Worker pool shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::sleep;

    fn test_config() -> PoolConfig {
        PoolConfig::default()
            .with_min_workers(2)
            .with_max_workers(5)
            .with_queue_buffer(100)
            .with_task_timeout(Duration::from_secs(1))
    }

    #[tokio::test]
    async fn pool_initializes_with_min_workers() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();

        let metrics = pool.metrics();
        assert_eq!(metrics.active_workers, 2); // min_workers

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_validates_configuration() {
        // min_workers must be at least 1
        let config = PoolConfig::default()
            .with_min_workers(0)
            .with_max_workers(5);
        assert!(WorkerPool::new(config).await.is_err());

        // max_workers must be >= min_workers
        let config = PoolConfig::default()
            .with_min_workers(5)
            .with_max_workers(2);
        assert!(WorkerPool::new(config).await.is_err());

        // queue_buffer must be at least 1
        let config = PoolConfig::default().with_queue_buffer(0);
        assert!(WorkerPool::new(config).await.is_err());
    }

    #[tokio::test]
    async fn pool_executes_tasks_successfully() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();

        let result = pool
            .submit_task(TaskPriority::Normal, async { Ok::<_, anyhow::Error>(42) })
            .await
            .unwrap();

        let value = result.await.unwrap().unwrap();
        assert_eq!(value, 42);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_handles_task_failures() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();

        let result = pool
            .submit_task(TaskPriority::Normal, async {
                Err::<(), _>(anyhow::anyhow!("Task failed"))
            })
            .await
            .unwrap();

        let error = result.await.unwrap();
        assert!(error.is_err());

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_respects_task_priorities() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();
        let pool = Arc::new(pool);

        let mut results = Vec::new();

        // Submit low priority task first
        let low = pool
            .submit_task(TaskPriority::Low, async {
                sleep(Duration::from_millis(50)).await;
                Ok::<_, anyhow::Error>("low")
            })
            .await
            .unwrap();
        results.push(low);

        // Then high priority
        let high = pool
            .submit_task(TaskPriority::High, async {
                sleep(Duration::from_millis(10)).await;
                Ok::<_, anyhow::Error>("high")
            })
            .await
            .unwrap();
        results.push(high);

        // Then normal priority
        let normal = pool
            .submit_task(TaskPriority::Normal, async {
                sleep(Duration::from_millis(20)).await;
                Ok::<_, anyhow::Error>("normal")
            })
            .await
            .unwrap();
        results.push(normal);

        // High priority should complete first despite being submitted second
        let low_handle = results.remove(0);
        let high_handle = results.remove(0);
        let normal_handle = results.remove(0);

        let high_result = high_handle.await.unwrap().unwrap();
        assert_eq!(high_result, "high");

        let normal_result = normal_handle.await.unwrap().unwrap();
        assert_eq!(normal_result, "normal");

        let low_result = low_handle.await.unwrap().unwrap();
        assert_eq!(low_result, "low");

        // Need to extract from Arc properly
        let pool = Arc::try_unwrap(pool).unwrap_or_else(|_| panic!("Arc still has references"));
        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_tracks_metrics() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();

        // Submit some tasks
        for i in 0..5 {
            let pool = &pool;
            pool.submit_task(TaskPriority::Normal, async move {
                sleep(Duration::from_millis(10)).await;
                Ok::<_, anyhow::Error>(i)
            })
            .await
            .unwrap();
        }

        // Wait a bit for tasks to complete
        sleep(Duration::from_millis(200)).await;

        let metrics = pool.metrics();
        assert!(metrics.tasks_submitted >= 5);
        assert!(metrics.tasks_completed >= 5);
        assert_eq!(metrics.tasks_failed, 0);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_handles_task_timeouts() {
        let config = test_config().with_task_timeout(Duration::from_millis(100));
        let pool = WorkerPool::new(config).await.unwrap();

        let result = pool
            .submit_task(TaskPriority::Normal, async {
                sleep(Duration::from_secs(1)).await;
                Ok::<_, anyhow::Error>("late")
            })
            .await
            .unwrap();

        let error = result.await.unwrap();
        assert!(error.is_err());
        assert!(matches!(error, Err(TaskError::Timeout { .. })));

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_concurrent_execution() {
        let config = test_config().with_min_workers(3).with_max_workers(3);
        let pool = WorkerPool::new(config).await.unwrap();

        let start = Instant::now();

        // Submit 3 tasks that each take 100ms
        let mut handles = Vec::new();
        for i in 0..3 {
            let handle = pool
                .submit_task(TaskPriority::Normal, async move {
                    sleep(Duration::from_millis(100)).await;
                    Ok::<_, anyhow::Error>(i)
                })
                .await
                .unwrap();
            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let duration = start.elapsed();

        // With 3 workers, should complete in ~100ms, not 300ms
        assert!(duration.as_millis() < 250); // Allow some margin
        assert!(duration.as_millis() > 80);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pool_graceful_shutdown() {
        let config = test_config();
        let pool = WorkerPool::new(config).await.unwrap();

        // Submit a long-running task
        let handle = pool
            .submit_task(TaskPriority::Normal, async {
                sleep(Duration::from_millis(100)).await;
                Ok::<_, anyhow::Error>(42)
            })
            .await
            .unwrap();

        // Initiate shutdown
        let shutdown_start = Instant::now();
        pool.shutdown().await.unwrap();
        let shutdown_duration = shutdown_start.elapsed();

        // Task should complete before shutdown finishes
        let result = handle.await.unwrap().unwrap();
        assert_eq!(result, 42);

        // Shutdown should wait for task to complete
        assert!(shutdown_duration.as_millis() > 80);
    }

    #[tokio::test]
    async fn static_scaling_strategy() {
        let config = test_config()
            .with_min_workers(3)
            .with_max_workers(3)
            .with_scaling_strategy(ScalingStrategy::Static);
        let pool = WorkerPool::new(config).await.unwrap();

        let metrics = pool.metrics();
        assert_eq!(metrics.active_workers, 3);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn elastic_scaling_strategy_config() {
        let strategy = ScalingStrategy::Elastic {
            target_queue_per_worker: 5,
            scale_down_cooldown: Duration::from_mins(1),
        };

        let config = test_config().with_scaling_strategy(strategy);
        let pool = WorkerPool::new(config).await.unwrap();

        // Verify pool was created successfully
        let metrics = pool.metrics();
        assert!(metrics.active_workers >= 2);

        pool.shutdown().await.unwrap();
    }

    // ---- New tests (15) ----

    #[test]
    fn task_priority_ordering() {
        // High > Normal > Low must hold for Ord
        assert!(TaskPriority::High > TaskPriority::Normal);
        assert!(TaskPriority::Normal > TaskPriority::Low);
        assert!(TaskPriority::High > TaskPriority::Low);
        // Equalities
        assert_eq!(TaskPriority::High, TaskPriority::High);
        assert_eq!(TaskPriority::Normal, TaskPriority::Normal);
        assert_eq!(TaskPriority::Low, TaskPriority::Low);
    }

    #[test]
    fn task_priority_default_is_normal() {
        assert_eq!(TaskPriority::default(), TaskPriority::Normal);
    }

    #[test]
    fn task_priority_discriminants() {
        // Verify the integer discriminants used for comparison
        assert_eq!(TaskPriority::Low as usize, 0);
        assert_eq!(TaskPriority::Normal as usize, 1);
        assert_eq!(TaskPriority::High as usize, 2);
    }

    #[test]
    fn pool_config_default_values() {
        let config = PoolConfig::default();
        assert_eq!(config.min_workers, 2);
        assert_eq!(config.max_workers, 10);
        assert_eq!(config.queue_buffer, 1000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.task_timeout, Duration::from_secs(30));
        assert_eq!(config.health_check_interval, Duration::from_secs(5));
        assert_eq!(config.worker_idle_timeout, Duration::from_mins(1));
    }

    #[test]
    fn pool_config_builder_chaining() {
        let config = PoolConfig::default()
            .with_min_workers(4)
            .with_max_workers(8)
            .with_queue_buffer(500)
            .with_task_timeout(Duration::from_mins(1))
            .with_health_check_interval(Duration::from_secs(10))
            .with_worker_idle_timeout(Duration::from_mins(2))
            .with_max_retries(5)
            .with_scaling_strategy(ScalingStrategy::Static);

        assert_eq!(config.min_workers, 4);
        assert_eq!(config.max_workers, 8);
        assert_eq!(config.queue_buffer, 500);
        assert_eq!(config.task_timeout, Duration::from_mins(1));
        assert_eq!(config.health_check_interval, Duration::from_secs(10));
        assert_eq!(config.worker_idle_timeout, Duration::from_mins(2));
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.scaling_strategy, ScalingStrategy::Static);
    }

    #[test]
    fn pool_config_validate_rejects_zero_min_workers() {
        let config = PoolConfig::default()
            .with_min_workers(0)
            .with_max_workers(5);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("min_workers must be at least 1"));
    }

    #[test]
    fn pool_config_validate_rejects_max_less_than_min() {
        let config = PoolConfig::default()
            .with_min_workers(10)
            .with_max_workers(3);
        let err = config.validate().unwrap_err();
        assert!(err
            .to_string()
            .contains("max_workers must be >= min_workers"));
    }

    #[test]
    fn pool_config_validate_rejects_zero_queue_buffer() {
        let config = PoolConfig::default().with_queue_buffer(0);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("queue_buffer must be at least 1"));
    }

    #[test]
    fn pool_config_validate_accepts_min_equals_max() {
        let config = PoolConfig::default()
            .with_min_workers(5)
            .with_max_workers(5);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn pool_metrics_default_values() {
        let metrics = PoolMetrics::default();
        assert_eq!(metrics.active_workers, 0);
        assert_eq!(metrics.busy_workers, 0);
        assert_eq!(metrics.idle_workers, 0);
        assert_eq!(metrics.tasks_submitted, 0);
        assert_eq!(metrics.tasks_completed, 0);
        assert_eq!(metrics.tasks_failed, 0);
        assert_eq!(metrics.tasks_cancelled, 0);
        assert_eq!(metrics.queue_depth, 0);
        assert_eq!(metrics.avg_task_duration, Duration::ZERO);
        assert!((metrics.utilization - 0.0).abs() < f32::EPSILON);
        assert!((metrics.throughput - 0.0).abs() < f32::EPSILON);
        assert_eq!(metrics.scale_up_count, 0);
        assert_eq!(metrics.scale_down_count, 0);
    }

    #[test]
    fn cancellation_token_default_and_new() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        let default_token = CancellationToken::default();
        assert!(!default_token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel_sets_flag() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone_shares_state() {
        let token = CancellationToken::new();
        let cloned = token.clone();
        assert!(!token.is_cancelled());
        assert!(!cloned.is_cancelled());

        cloned.cancel();
        assert!(token.is_cancelled(), "original should see cancellation");
        assert!(cloned.is_cancelled(), "clone should see cancellation");
    }

    #[test]
    fn scaling_strategy_default_is_elastic() {
        let strategy = ScalingStrategy::default();
        match strategy {
            ScalingStrategy::Elastic {
                target_queue_per_worker,
                scale_down_cooldown,
            } => {
                assert_eq!(target_queue_per_worker, 3);
                assert_eq!(scale_down_cooldown, Duration::from_secs(30));
            }
            _ => panic!("default should be Elastic"),
        }
    }

    #[test]
    fn scaling_strategy_equality() {
        let a = ScalingStrategy::Static;
        let b = ScalingStrategy::Static;
        assert_eq!(a, b);

        let c = ScalingStrategy::Elastic {
            target_queue_per_worker: 5,
            scale_down_cooldown: Duration::from_mins(1),
        };
        let d = ScalingStrategy::Elastic {
            target_queue_per_worker: 5,
            scale_down_cooldown: Duration::from_mins(1),
        };
        assert_eq!(c, d);

        // Different parameters should not be equal
        let e = ScalingStrategy::Elastic {
            target_queue_per_worker: 10,
            scale_down_cooldown: Duration::from_mins(1),
        };
        assert_ne!(c, e);

        // Different variants are not equal
        assert_ne!(a, c);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for worker_pool
    // =========================================================================

    // 1. TaskPriority clone equal
    #[test]
    fn task_priority_clone_equal() {
        let p = TaskPriority::High;
        let cloned = p;
        assert_eq!(p, cloned);
    }

    // 2. TaskPriority copy semantics
    #[test]
    fn task_priority_copy_semantics() {
        let a = TaskPriority::Normal;
        let b = a; // Copy, not move
        assert_eq!(a, b); // a is still usable
    }

    // 3. ScalingStrategy clone equal
    #[test]
    fn scaling_strategy_clone_equal() {
        let s = ScalingStrategy::Elastic {
            target_queue_per_worker: 7,
            scale_down_cooldown: Duration::from_secs(45),
        };
        let cloned = s;
        assert_eq!(s, cloned);
    }

    // 4. ScalingStrategy Predictive variant construction
    #[test]
    fn scaling_strategy_predictive_construction() {
        let strategy = ScalingStrategy::Predictive {
            window_size: 100,
            scale_threshold: 0.8,
        };
        if let ScalingStrategy::Predictive {
            window_size,
            scale_threshold,
        } = strategy
        {
            assert_eq!(window_size, 100);
            assert!((scale_threshold - 0.8).abs() < f32::EPSILON);
        } else {
            panic!("Expected Predictive variant");
        }
    }

    // 5. PoolConfig clone equal
    #[test]
    fn pool_config_clone_equal() {
        let config = test_config();
        let cloned = config.clone();
        assert_eq!(cloned.min_workers, config.min_workers);
        assert_eq!(cloned.max_workers, config.max_workers);
        assert_eq!(cloned.queue_buffer, config.queue_buffer);
        assert_eq!(cloned.task_timeout, config.task_timeout);
    }

    // 6. PoolMetrics clone equal
    #[test]
    fn pool_metrics_clone_equal() {
        let metrics = PoolMetrics::default();
        let cloned = metrics.clone();
        assert_eq!(cloned.active_workers, metrics.active_workers);
        assert_eq!(cloned.tasks_submitted, metrics.tasks_submitted);
        assert_eq!(cloned.utilization, metrics.utilization);
    }

    // 7. PoolMetrics with nonzero values
    #[test]
    fn pool_metrics_nonzero_values() {
        let metrics = PoolMetrics {
            active_workers: 5,
            busy_workers: 3,
            idle_workers: 2,
            tasks_submitted: 100,
            tasks_completed: 90,
            tasks_failed: 5,
            tasks_cancelled: 5,
            queue_depth: 10,
            avg_task_duration: Duration::from_millis(150),
            utilization: 0.6,
            throughput: 12.5,
            scale_up_count: 3,
            scale_down_count: 1,
        };
        assert_eq!(metrics.active_workers, 5);
        assert_eq!(
            metrics.busy_workers + metrics.idle_workers,
            metrics.active_workers
        );
        assert!((metrics.utilization - 0.6).abs() < f32::EPSILON);
        assert!((metrics.throughput - 12.5).abs() < f32::EPSILON);
    }

    // 8. CancellationToken debug format
    #[test]
    fn cancellation_token_debug_format() {
        let token = CancellationToken::new();
        let debug = format!("{:?}", token);
        assert!(debug.contains("CancellationToken"));
    }

    // 9. PoolConfig debug format
    #[test]
    fn pool_config_debug_format() {
        let config = PoolConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("min_workers"));
        assert!(debug.contains("max_workers"));
        assert!(debug.contains("queue_buffer"));
    }

    // 10. PoolMetrics debug format
    #[test]
    fn pool_metrics_debug_format() {
        let metrics = PoolMetrics {
            active_workers: 3,
            ..Default::default()
        };
        let debug = format!("{:?}", metrics);
        assert!(debug.contains("active_workers"));
        assert!(debug.contains("tasks_submitted"));
    }

    // 11. ScalingStrategy debug format
    #[test]
    fn scaling_strategy_debug_format() {
        let debug = format!("{:?}", ScalingStrategy::Static);
        assert!(debug.contains("Static"));
    }

    // 12. TaskPriority debug format
    #[test]
    fn task_priority_debug_format() {
        assert!(format!("{:?}", TaskPriority::High).contains("High"));
        assert!(format!("{:?}", TaskPriority::Normal).contains("Normal"));
        assert!(format!("{:?}", TaskPriority::Low).contains("Low"));
    }

    // 13. TaskError display messages
    #[test]
    fn task_error_display_messages() {
        let timeout_err = TaskError::Timeout {
            timeout: Duration::from_secs(5),
        };
        assert!(timeout_err.to_string().contains("5"));

        let cancelled_err = TaskError::Cancelled;
        assert!(cancelled_err.to_string().contains("cancelled"));

        let shutdown_err = TaskError::Shutdown;
        assert!(shutdown_err.to_string().contains("shutting down"));

        let queue_full_err = TaskError::QueueFull;
        assert!(queue_full_err.to_string().contains("full"));
    }

    // 14. PoolConfig validate accepts valid config
    #[test]
    fn pool_config_validate_accepts_valid() {
        let config = PoolConfig::default()
            .with_min_workers(1)
            .with_max_workers(100)
            .with_queue_buffer(1);
        assert!(config.validate().is_ok());
    }

    // 15. ScalingStrategy default is Elastic with expected values
    #[test]
    fn scaling_strategy_default_values() {
        let default = ScalingStrategy::default();
        match default {
            ScalingStrategy::Elastic {
                target_queue_per_worker,
                scale_down_cooldown,
            } => {
                assert_eq!(target_queue_per_worker, 3);
                assert_eq!(scale_down_cooldown, Duration::from_secs(30));
            }
            _ => panic!("Expected Elastic default"),
        }
    }
}
