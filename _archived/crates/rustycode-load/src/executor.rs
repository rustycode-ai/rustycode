//! Load test execution engine

use crate::error::{LoadTestError, Result};
use crate::metrics::{metrics_channel, LoadTestResults};
use crate::ramp_up::RampUpStrategy;
use crate::scenario::LoadScenario;
use chrono::{DateTime, Utc};
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::{interval, sleep};
use tracing::{debug, info, warn};

/// Configuration for load test execution
#[derive(Clone)]
pub struct LoadTestConfig {
    /// HTTP client configuration
    pub http_client: Option<Client>,

    /// Metrics collection interval
    pub metrics_interval: Duration,

    /// Whether to collect time series data
    pub collect_time_series: bool,

    /// Maximum concurrent requests per user
    pub max_concurrent_per_user: usize,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            http_client: None,
            metrics_interval: crate::defaults::DEFAULT_METRICS_INTERVAL,
            collect_time_series: true,
            max_concurrent_per_user: 1,
        }
    }
}

/// Progress update for a running load test
#[derive(Clone, Debug)]
pub struct LoadTestProgress {
    /// Current time
    pub current_time: DateTime<Utc>,

    /// Elapsed time since start
    pub elapsed: Duration,

    /// Total test duration
    pub total_duration: Duration,

    /// Number of active users
    pub active_users: usize,

    /// Total requests completed
    pub completed_requests: usize,

    /// Successful requests
    pub successful_requests: usize,

    /// Failed requests
    pub failed_requests: usize,

    /// Current requests per second
    pub current_rps: f64,

    /// Average response time
    pub avg_response_time: Duration,
}

impl LoadTestProgress {
    /// Calculate percent complete
    pub fn percent_complete(&self) -> f64 {
        if self.total_duration.is_zero() {
            100.0
        } else {
            (self.elapsed.as_secs_f64() / self.total_duration.as_secs_f64()) * 100.0
        }
    }
}

/// Load test execution engine
pub struct LoadTestRunner {
    /// HTTP client
    http_client: Arc<Client>,

    /// Configuration
    #[allow(dead_code)] // Kept for future use
    config: LoadTestConfig,
}

impl LoadTestRunner {
    /// Create a new load test runner
    pub fn new() -> Self {
        Self::with_config(LoadTestConfig::default())
    }

    /// Create a new load test runner with custom configuration
    pub fn with_config(config: LoadTestConfig) -> Self {
        let http_client = config.http_client.clone().unwrap_or_else(|| {
            Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|e| panic!("failed to build HTTP client for load test: {e}"))
        });

        Self {
            http_client: Arc::new(http_client),
            config,
        }
    }

    /// Run a load test scenario
    pub async fn run(&self, scenario: LoadScenario) -> Result<LoadTestResults> {
        let scenario_name = scenario.name.clone();
        let scenario_duration = scenario.duration;

        info!(
            "Starting load test: {} with {} concurrent users for {:?}",
            scenario_name, scenario.concurrent_users, scenario_duration
        );

        let start_time = Utc::now();
        let start_instant = Instant::now();
        #[allow(unused_assignments)]
        let mut results = LoadTestResults::new(scenario_name.clone());

        // Create metrics channel
        let (metrics_tx, mut metrics_rx) = metrics_channel();

        // Spawn metrics collection task
        let metrics_task_name = scenario_name.clone();
        let metrics_task = tokio::spawn(async move {
            let mut local_results = LoadTestResults::new(metrics_task_name);

            while let Some(result) = metrics_rx.recv().await {
                local_results.add_result(&result);
            }

            local_results
        });

        // Spawn user tasks
        let mut user_tasks = Vec::new();
        for user_id in 0..scenario.concurrent_users {
            let scenario_clone = scenario.clone();
            let http_client = self.http_client.clone();
            let metrics_tx = metrics_tx.clone();
            let start_time_clone = start_time;

            let task = tokio::spawn(async move {
                Self::run_user(
                    user_id,
                    scenario_clone,
                    http_client,
                    metrics_tx,
                    start_time_clone,
                )
                .await
            });

            user_tasks.push(task);
        }

        // Wait for test duration
        sleep(scenario_duration).await;

        // Stop all user tasks by dropping the metrics channel
        drop(metrics_tx);

        // Wait for all user tasks to complete
        for task in user_tasks {
            if let Err(e) = task.await {
                warn!("User task failed during shutdown: {}", e);
            }
        }

        // Wait for metrics collection to complete
        results = metrics_task
            .await
            .map_err(|e| LoadTestError::RuntimeError(format!("Metrics task failed: {e}")))?;

        // Finalize results
        results.end_time = Utc::now();
        results.total_duration = start_instant.elapsed();
        results.finalize();

        info!(
            "Load test completed: {} requests in {:?}",
            results.throughput.total_requests, results.total_duration
        );

        Ok(results)
    }

    /// Run a single user's load test
    async fn run_user(
        user_id: usize,
        scenario: LoadScenario,
        http_client: Arc<Client>,
        metrics_tx: crate::metrics::MetricsSender,
        _start_time: DateTime<Utc>,
    ) -> Result<()> {
        // Calculate ramp-up delay for this user
        let ramp_up_delay = match &scenario.ramp_up {
            RampUpStrategy::Immediate => Duration::ZERO,
            RampUpStrategy::Linear { duration } => {
                // Use nanosecond arithmetic to avoid Duration */u32 panic on overflow
                let users = scenario.concurrent_users.max(1) as u64;
                let delay_ns = duration.as_nanos() as u64 / users;
                let total_ns = delay_ns.saturating_mul(user_id as u64);
                Duration::from_nanos(total_ns)
            }
            RampUpStrategy::Stepped {
                steps,
                step_duration,
            } => {
                let users_per_step = scenario.concurrent_users.div_ceil(*steps);
                let step = user_id / users_per_step;
                // Use nanosecond arithmetic to avoid Duration * u32 panic on overflow
                let step_ns = step_duration.as_nanos() as u64;
                let total_ns = step_ns.saturating_mul(step as u64);
                Duration::from_nanos(total_ns)
            }
        };

        // Wait for ramp-up
        if !ramp_up_delay.is_zero() {
            sleep(ramp_up_delay).await;
        }

        let mut request_count = 0;
        let start_instant = Instant::now();

        // Execute requests until test duration is reached
        while start_instant.elapsed() < scenario.duration {
            // Generate request
            let mut request = (scenario.request_generator)(user_id);
            request = request.with_user_id(user_id);

            // Execute request
            let result = request.execute(&http_client).await;

            // Send result to metrics collector
            if metrics_tx.send(result).is_err() {
                // Channel closed, stop sending
                break;
            }

            request_count += 1;

            // Think time between requests
            if !scenario.think_time.is_zero() {
                sleep(scenario.think_time).await;
            }

            // Check if we should stop on error
            if scenario.stop_on_error {
                // This would require checking the result, but we've already sent it
                // For now, we'll continue
            }
        }

        debug!("User {} completed {} requests", user_id, request_count);

        Ok(())
    }

    /// Run a load test with progress updates
    #[allow(clippy::too_many_lines)]
    pub async fn run_with_progress<F>(
        &self,
        scenario: LoadScenario,
        mut progress_callback: F,
    ) -> Result<LoadTestResults>
    where
        F: FnMut(LoadTestProgress) + Send + 'static,
    {
        let scenario_name = scenario.name.clone();
        let _scenario_duration = scenario.duration;
        let scenario_concurrent_users = scenario.concurrent_users;
        let scenario_ramp_up = scenario.ramp_up.clone();

        info!(
            "Starting load test with progress: {} with {} concurrent users",
            scenario_name, scenario_concurrent_users
        );

        let start_time = Utc::now();
        let start_instant = Instant::now();
        #[allow(unused_assignments)]
        let mut results = LoadTestResults::new(scenario_name.clone());

        // Create metrics channel
        let (metrics_tx, mut metrics_rx) = metrics_channel();

        // Spawn metrics collection task with progress updates
        let metrics_task_name = scenario_name.clone();
        let metrics_task = tokio::spawn(async move {
            let mut local_results = LoadTestResults::new(metrics_task_name);
            let mut _completed_requests = 0;
            let mut _successful_requests = 0;
            let mut _failed_requests = 0;
            let mut _total_response_time = Duration::ZERO;

            while let Some(result) = metrics_rx.recv().await {
                _completed_requests += 1;
                _total_response_time += result.duration;

                if result.success {
                    _successful_requests += 1;
                } else {
                    _failed_requests += 1;
                }

                local_results.add_result(&result);
            }

            local_results
        });

        // Spawn progress reporting task
        let _progress_tx = metrics_tx.clone();
        let duration = scenario.duration;
        let _concurrent_users = scenario.concurrent_users;
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            let start = Instant::now();

            loop {
                interval.tick().await;
                let elapsed = start.elapsed();

                if elapsed >= duration {
                    break;
                }

                // Calculate active users based on ramp-up
                let active_users = match &scenario_ramp_up {
                    RampUpStrategy::Immediate => scenario_concurrent_users,
                    RampUpStrategy::Linear { duration: ramp_up } => {
                        if elapsed >= *ramp_up {
                            scenario_concurrent_users
                        } else {
                            #[allow(clippy::cast_precision_loss)]
                            let users = scenario_concurrent_users as f64;
                            (elapsed.as_secs_f64() / ramp_up.as_secs_f64() * users) as usize
                        }
                    }
                    RampUpStrategy::Stepped {
                        steps,
                        step_duration,
                    } => {
                        let current_step =
                            (elapsed.as_secs_f64() / step_duration.as_secs_f64()) as usize;
                        let users_per_step = scenario_concurrent_users.div_ceil(*steps);
                        std::cmp::min(
                            users_per_step * (current_step + 1),
                            scenario_concurrent_users,
                        )
                    }
                };

                let progress = LoadTestProgress {
                    current_time: Utc::now(),
                    elapsed,
                    total_duration: duration,
                    active_users,
                    completed_requests: 0, // Would need shared state
                    successful_requests: 0,
                    failed_requests: 0,
                    current_rps: 0.0,
                    avg_response_time: Duration::ZERO,
                };

                progress_callback(progress);
            }
        });

        // Spawn user tasks
        let mut user_tasks = Vec::new();
        for user_id in 0..scenario.concurrent_users {
            let scenario_clone = scenario.clone();
            let http_client = self.http_client.clone();
            let metrics_tx_clone = metrics_tx.clone();
            let start_time_clone = start_time;

            let task = tokio::spawn(async move {
                Self::run_user(
                    user_id,
                    scenario_clone,
                    http_client,
                    metrics_tx_clone,
                    start_time_clone,
                )
                .await
            });

            user_tasks.push(task);
        }

        // Wait for test duration
        sleep(scenario.duration).await;

        // Stop all tasks
        drop(metrics_tx);

        // Wait for completion
        for task in user_tasks {
            if let Err(e) = task.await {
                warn!("User task failed during shutdown: {}", e);
            }
        }

        results = metrics_task
            .await
            .map_err(|e| LoadTestError::RuntimeError(format!("Metrics task failed: {e}")))?;

        results.end_time = Utc::now();
        results.total_duration = start_instant.elapsed();
        results.finalize();

        Ok(results)
    }
}

impl Default for LoadTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runner_creation() {
        let runner = LoadTestRunner::new();
        assert_eq!(
            runner.config.metrics_interval,
            crate::defaults::DEFAULT_METRICS_INTERVAL
        );
    }

    #[tokio::test]
    async fn test_runner_with_config() {
        let config = LoadTestConfig {
            metrics_interval: Duration::from_millis(500),
            ..Default::default()
        };

        let runner = LoadTestRunner::with_config(config);
        assert_eq!(runner.config.metrics_interval, Duration::from_millis(500));
    }

    #[test]
    fn test_progress_percent_complete() {
        let progress = LoadTestProgress {
            current_time: Utc::now(),
            elapsed: Duration::from_secs(30),
            total_duration: Duration::from_mins(1),
            active_users: 10,
            completed_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            current_rps: 3.33,
            avg_response_time: Duration::from_millis(100),
        };

        assert_eq!(progress.percent_complete(), 50.0);
    }

    #[tokio::test]
    async fn test_load_test_progress() {
        let progress = LoadTestProgress {
            current_time: Utc::now(),
            elapsed: Duration::from_secs(45),
            total_duration: Duration::from_mins(1),
            active_users: 50,
            completed_requests: 1000,
            successful_requests: 950,
            failed_requests: 50,
            current_rps: 22.22,
            avg_response_time: Duration::from_millis(150),
        };

        assert!((progress.percent_complete() - 75.0).abs() < 0.1);
    }

    #[test]
    fn test_progress_percent_complete_zero_duration() {
        let progress = LoadTestProgress {
            current_time: Utc::now(),
            elapsed: Duration::from_secs(10),
            total_duration: Duration::ZERO,
            active_users: 5,
            completed_requests: 50,
            successful_requests: 50,
            failed_requests: 0,
            current_rps: 0.0,
            avg_response_time: Duration::ZERO,
        };
        assert_eq!(progress.percent_complete(), 100.0);
    }

    #[test]
    fn test_progress_percent_complete_full() {
        let progress = LoadTestProgress {
            current_time: Utc::now(),
            elapsed: Duration::from_mins(1),
            total_duration: Duration::from_mins(1),
            active_users: 10,
            completed_requests: 100,
            successful_requests: 100,
            failed_requests: 0,
            current_rps: 1.0,
            avg_response_time: Duration::from_millis(100),
        };
        assert_eq!(progress.percent_complete(), 100.0);
    }

    #[test]
    fn test_load_test_config_default() {
        let config = LoadTestConfig::default();
        assert!(config.http_client.is_none());
        assert!(config.collect_time_series);
        assert_eq!(config.max_concurrent_per_user, 1);
        assert_eq!(
            config.metrics_interval,
            crate::defaults::DEFAULT_METRICS_INTERVAL
        );
    }

    #[test]
    fn test_load_test_progress_clone() {
        let progress = LoadTestProgress {
            current_time: Utc::now(),
            elapsed: Duration::from_secs(5),
            total_duration: Duration::from_mins(1),
            active_users: 2,
            completed_requests: 10,
            successful_requests: 9,
            failed_requests: 1,
            current_rps: 2.0,
            avg_response_time: Duration::from_millis(50),
        };
        let cloned = progress.clone();
        assert_eq!(cloned.elapsed, progress.elapsed);
        assert_eq!(cloned.active_users, progress.active_users);
    }

    #[test]
    fn test_runner_default() {
        let runner = LoadTestRunner::default();
        assert_eq!(
            runner.config.metrics_interval,
            crate::defaults::DEFAULT_METRICS_INTERVAL
        );
    }
}
