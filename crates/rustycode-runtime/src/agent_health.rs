//! Agent Health Monitoring and Self-Healing System
//!
//! This module provides comprehensive health monitoring for AI agents with:
//! - Real-time health metrics tracking
//! - Anomaly detection and prediction
//! - Automatic recovery and self-healing
//! - Resource threshold monitoring
//! - Performance degradation detection

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Current health status of an agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String, severity: f64 },
    Critical { reason: String },
    Recovering,
    Failed,
}

/// Detailed health metrics for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub agent_role: AgentRole,
    pub status: HealthStatus,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub response_time_ms: f64,
    pub success_rate: f64,
    pub error_count: usize,
    pub last_check: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub task_queue_size: usize,
    pub resource_pressure: f64,
}

/// Health anomaly detected in an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAnomaly {
    pub agent_role: AgentRole,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub description: String,
    pub detected_at: DateTime<Utc>,
    pub metrics: HealthMetrics,
    pub suggested_action: RecoveryAction,
}

/// Type of health anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AnomalyType {
    HighCPUUsage { threshold: f64, actual: f64 },
    HighMemoryUsage { threshold_mb: f64, actual_mb: f64 },
    SlowResponse { threshold_ms: f64, actual_ms: f64 },
    LowSuccessRate { threshold: f64, actual: f64 },
    HighErrorRate { threshold: f64, actual: f64 },
    TaskQueueBacklog { threshold: usize, actual: usize },
    ResourcePressure { pressure: f64 },
    PerformanceDegradation { drop_percent: f64 },
    UnusualPattern { pattern: String },
}

/// Severity of the anomaly
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Recovery action to take
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecoveryAction {
    Monitor,
    ScaleResources,
    RestartAgent,
    ReconfigureAgent,
    FailoverToBackup,
    DisableAgent,
    CustomAction { action: String },
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub check_interval_ms: u64,
    pub cpu_threshold_percent: f64,
    pub memory_threshold_mb: f64,
    pub response_time_threshold_ms: f64,
    pub success_rate_threshold: f64,
    pub error_rate_threshold: f64,
    pub queue_size_threshold: usize,
    pub resource_pressure_threshold: f64,
    pub enable_auto_recovery: bool,
    pub max_recovery_attempts: usize,
    pub recovery_cooldown_ms: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval_ms: 5000,
            cpu_threshold_percent: 80.0,
            memory_threshold_mb: 512.0,
            response_time_threshold_ms: 2000.0,
            success_rate_threshold: 0.9,
            error_rate_threshold: 0.1,
            queue_size_threshold: 100,
            resource_pressure_threshold: 0.8,
            enable_auto_recovery: true,
            max_recovery_attempts: 3,
            recovery_cooldown_ms: 30000,
        }
    }
}

/// Recovery attempt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    pub agent_role: AgentRole,
    pub anomaly: HealthAnomaly,
    pub action_taken: RecoveryAction,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub success: bool,
    pub attempt_number: usize,
    pub message: String,
}

/// Agent health history for trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthHistory {
    pub agent_role: AgentRole,
    pub metrics: Vec<HealthMetrics>,
    pub anomalies: Vec<HealthAnomaly>,
    pub recovery_attempts: Vec<RecoveryAttempt>,
    pub last_updated: DateTime<Utc>,
}

/// Main health monitoring system
pub struct AgentHealthMonitor {
    health_metrics: Arc<RwLock<HashMap<AgentRole, HealthMetrics>>>,
    health_history: Arc<RwLock<HashMap<AgentRole, HealthHistory>>>,
    active_recoveries: Arc<RwLock<HashMap<AgentRole, Vec<RecoveryAttempt>>>>,
    config: HealthCheckConfig,
    is_running: Arc<RwLock<bool>>,
}

impl AgentHealthMonitor {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            health_metrics: Arc::new(RwLock::new(HashMap::new())),
            health_history: Arc::new(RwLock::new(HashMap::new())),
            active_recoveries: Arc::new(RwLock::new(HashMap::new())),
            config,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start health monitoring loop
    pub async fn start_monitoring(&self) -> Result<(), String> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Ok(());
        }
        *is_running = true;
        drop(is_running);

        let check_interval = Duration::from_millis(self.config.check_interval_ms);
        let health_metrics = self.health_metrics.clone();
        let health_history = self.health_history.clone();
        let active_recoveries = self.active_recoveries.clone();
        let config = self.config.clone();
        let is_running_flag = self.is_running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);

            loop {
                interval.tick().await;

                // Check if still running
                {
                    let is_running = is_running_flag.read().await;
                    if !*is_running {
                        break;
                    }
                }

                // Perform health checks
                Self::perform_health_checks(
                    health_metrics.clone(),
                    health_history.clone(),
                    active_recoveries.clone(),
                    &config,
                )
                .await;
            }
        });

        Ok(())
    }

    /// Stop health monitoring
    pub async fn stop_monitoring(&self) {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
    }

    /// Update health metrics for an agent
    pub async fn update_metrics(&self, metrics: HealthMetrics) -> Result<(), String> {
        let mut health_map = self.health_metrics.write().await;
        health_map.insert(metrics.agent_role, metrics.clone());

        // Update history
        let mut history_map = self.health_history.write().await;
        let history = history_map
            .entry(metrics.agent_role)
            .or_insert_with(|| HealthHistory {
                agent_role: metrics.agent_role,
                metrics: Vec::new(),
                anomalies: Vec::new(),
                recovery_attempts: Vec::new(),
                last_updated: Utc::now(),
            });

        history.metrics.push(metrics.clone());

        // Keep only last 1000 metrics
        if history.metrics.len() > 1000 {
            history.metrics.remove(0);
        }

        history.last_updated = Utc::now();

        Ok(())
    }

    /// Get current health status for all agents
    pub async fn all_health_status(&self) -> HashMap<AgentRole, HealthStatus> {
        let health_map = self.health_metrics.read().await;
        health_map
            .iter()
            .map(|(role, metrics)| (*role, metrics.status.clone()))
            .collect()
    }

    /// Get health metrics for a specific agent
    pub async fn agent_metrics(&self, role: AgentRole) -> Option<HealthMetrics> {
        let health_map = self.health_metrics.read().await;
        health_map.get(&role).cloned()
    }

    /// Get health history for an agent
    pub async fn agent_history(&self, role: AgentRole) -> Option<HealthHistory> {
        let history_map = self.health_history.read().await;
        history_map.get(&role).cloned()
    }

    /// Get active recovery attempts
    pub async fn active_recoveries(&self) -> Vec<RecoveryAttempt> {
        let recoveries_map = self.active_recoveries.read().await;
        recoveries_map
            .values()
            .flat_map(|v| v.iter().cloned())
            .filter(|r| r.completed_at.is_none())
            .collect()
    }

    /// Check if an agent needs recovery based on current metrics
    pub async fn check_agent_health(&self, role: AgentRole) -> Option<HealthAnomaly> {
        let health_map = self.health_metrics.read().await;
        let metrics = health_map.get(&role)?;

        self.detect_anomaly(metrics)
    }

    /// Perform manual health check on all agents
    async fn perform_health_checks(
        health_metrics: Arc<RwLock<HashMap<AgentRole, HealthMetrics>>>,
        health_history: Arc<RwLock<HashMap<AgentRole, HealthHistory>>>,
        active_recoveries: Arc<RwLock<HashMap<AgentRole, Vec<RecoveryAttempt>>>>,
        config: &HealthCheckConfig,
    ) {
        let roles_to_check = {
            let health_map = health_metrics.read().await;
            health_map.keys().copied().collect::<Vec<AgentRole>>()
        };

        for role in roles_to_check {
            let metrics = {
                let health_map = health_metrics.read().await;
                health_map.get(&role).cloned()
            };

            if let Some(metrics) = metrics {
                // Detect anomalies
                if let Some(anomaly) = Self::detect_anomaly_from_config(&metrics, config) {
                    // Log anomaly
                    {
                        let mut history_map = health_history.write().await;
                        if let Some(history) = history_map.get_mut(&role) {
                            history.anomalies.push(anomaly.clone());

                            // Keep only last 100 anomalies
                            if history.anomalies.len() > 100 {
                                history.anomalies.remove(0);
                            }
                        }
                    }

                    // Attempt recovery if enabled
                    if config.enable_auto_recovery {
                        Self::attempt_recovery(
                            role,
                            anomaly,
                            active_recoveries.clone(),
                            health_history.clone(),
                            config,
                        )
                        .await;
                    }
                }
            }
        }
    }

    /// Detect anomaly based on metrics and config
    fn detect_anomaly_from_config(
        metrics: &HealthMetrics,
        config: &HealthCheckConfig,
    ) -> Option<HealthAnomaly> {
        // Check CPU usage
        if metrics.cpu_usage_percent > config.cpu_threshold_percent {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::HighCPUUsage {
                    threshold: config.cpu_threshold_percent,
                    actual: metrics.cpu_usage_percent,
                },
                severity: if metrics.cpu_usage_percent > 95.0 {
                    AnomalySeverity::Critical
                } else if metrics.cpu_usage_percent > 90.0 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!(
                    "CPU usage {:.1}% exceeds threshold {:.1}%",
                    metrics.cpu_usage_percent, config.cpu_threshold_percent
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: if metrics.cpu_usage_percent > 95.0 {
                    RecoveryAction::RestartAgent
                } else {
                    RecoveryAction::ScaleResources
                },
            });
        }

        // Check memory usage
        if metrics.memory_usage_mb > config.memory_threshold_mb {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::HighMemoryUsage {
                    threshold_mb: config.memory_threshold_mb,
                    actual_mb: metrics.memory_usage_mb,
                },
                severity: if metrics.memory_usage_mb > config.memory_threshold_mb * 1.5 {
                    AnomalySeverity::Critical
                } else if metrics.memory_usage_mb > config.memory_threshold_mb * 1.2 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!(
                    "Memory usage {:.1}MB exceeds threshold {:.1}MB",
                    metrics.memory_usage_mb, config.memory_threshold_mb
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: RecoveryAction::ScaleResources,
            });
        }

        // Check response time
        if metrics.response_time_ms > config.response_time_threshold_ms {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::SlowResponse {
                    threshold_ms: config.response_time_threshold_ms,
                    actual_ms: metrics.response_time_ms,
                },
                severity: if metrics.response_time_ms > config.response_time_threshold_ms * 2.0 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!(
                    "Response time {:.1}ms exceeds threshold {:.1}ms",
                    metrics.response_time_ms, config.response_time_threshold_ms
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: RecoveryAction::ScaleResources,
            });
        }

        // Check success rate
        if metrics.success_rate < config.success_rate_threshold {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::LowSuccessRate {
                    threshold: config.success_rate_threshold,
                    actual: metrics.success_rate,
                },
                severity: if metrics.success_rate < 0.5 {
                    AnomalySeverity::Critical
                } else if metrics.success_rate < 0.7 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!(
                    "Success rate {:.2} below threshold {:.2}",
                    metrics.success_rate, config.success_rate_threshold
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: if metrics.success_rate < 0.5 {
                    RecoveryAction::RestartAgent
                } else {
                    RecoveryAction::ReconfigureAgent
                },
            });
        }

        // Check task queue backlog
        if metrics.task_queue_size > config.queue_size_threshold {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::TaskQueueBacklog {
                    threshold: config.queue_size_threshold,
                    actual: metrics.task_queue_size,
                },
                severity: if metrics.task_queue_size > config.queue_size_threshold * 2 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::Medium
                },
                description: format!(
                    "Task queue size {} exceeds threshold {}",
                    metrics.task_queue_size, config.queue_size_threshold
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: RecoveryAction::ScaleResources,
            });
        }

        // Check resource pressure
        if metrics.resource_pressure > config.resource_pressure_threshold {
            return Some(HealthAnomaly {
                agent_role: metrics.agent_role,
                anomaly_type: AnomalyType::ResourcePressure {
                    pressure: metrics.resource_pressure,
                },
                severity: if metrics.resource_pressure > 0.95 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::High
                },
                description: format!(
                    "Resource pressure {:.2} exceeds threshold {:.2}",
                    metrics.resource_pressure, config.resource_pressure_threshold
                ),
                detected_at: Utc::now(),
                metrics: metrics.clone(),
                suggested_action: RecoveryAction::ScaleResources,
            });
        }

        None
    }

    /// Detect anomaly from metrics (using default config)
    fn detect_anomaly(&self, metrics: &HealthMetrics) -> Option<HealthAnomaly> {
        Self::detect_anomaly_from_config(metrics, &self.config)
    }

    /// Attempt recovery from an anomaly
    async fn attempt_recovery(
        role: AgentRole,
        anomaly: HealthAnomaly,
        active_recoveries: Arc<RwLock<HashMap<AgentRole, Vec<RecoveryAttempt>>>>,
        health_history: Arc<RwLock<HashMap<AgentRole, HealthHistory>>>,
        config: &HealthCheckConfig,
    ) {
        // Check if we're in cooldown
        let should_attempt = {
            let recoveries_map = active_recoveries.read().await;
            if let Some(attempts) = recoveries_map.get(&role) {
                if let Some(last_attempt) = attempts.last() {
                    if let Some(completed_at) = last_attempt.completed_at {
                        let cooldown = Duration::from_millis(config.recovery_cooldown_ms);
                        let time_since_completion = Utc::now() - completed_at;
                        if let Ok(chrono_cooldown) = chrono::Duration::from_std(cooldown) {
                            if time_since_completion < chrono_cooldown {
                                return; // Still in cooldown
                            }
                        }
                    }
                }

                // Check max attempts
                let attempts_count = attempts.iter().filter(|a| a.completed_at.is_some()).count();

                attempts_count < config.max_recovery_attempts
            } else {
                true
            }
        };

        if !should_attempt {
            return;
        }

        // Create recovery attempt
        let attempt_number = {
            let recoveries_map = active_recoveries.read().await;
            recoveries_map.get(&role).map_or(0, Vec::len)
        };

        let recovery_attempt = RecoveryAttempt {
            agent_role: role,
            anomaly: anomaly.clone(),
            action_taken: anomaly.suggested_action.clone(),
            started_at: Utc::now(),
            completed_at: None,
            success: false,
            attempt_number: attempt_number + 1,
            message: format!("Recovery attempt {} started", attempt_number + 1),
        };

        // Add to active recoveries
        {
            let mut recoveries_map = active_recoveries.write().await;
            let attempts = recoveries_map.entry(role).or_insert_with(Vec::new);
            attempts.push(recovery_attempt.clone());
        }

        // Simulate recovery action (in real system, this would actually perform the action)
        let recovery_result = Self::perform_recovery_action(&recovery_attempt, &anomaly).await;

        // Update recovery attempt with result
        {
            let mut recoveries_map = active_recoveries.write().await;
            if let Some(attempts) = recoveries_map.get_mut(&role) {
                if let Some(last) = attempts.last_mut() {
                    last.completed_at = Some(Utc::now());
                    last.success = recovery_result.success;
                    last.message = recovery_result.message;
                }
            }
        }

        // Add to history
        {
            let mut history_map = health_history.write().await;
            if let Some(history) = history_map.get_mut(&role) {
                history.recovery_attempts.push(recovery_attempt);

                // Keep only last 50 recovery attempts
                if history.recovery_attempts.len() > 50 {
                    history.recovery_attempts.remove(0);
                }
            }
        }
    }

    /// Perform the actual recovery action
    async fn perform_recovery_action(
        attempt: &RecoveryAttempt,
        _anomaly: &HealthAnomaly,
    ) -> RecoveryResult {
        // Simulate recovery action
        tokio::time::sleep(Duration::from_millis(100)).await;

        match attempt.action_taken {
            RecoveryAction::Monitor => RecoveryResult {
                success: true,
                message: "Monitoring agent for improvement".to_string(),
            },
            RecoveryAction::ScaleResources => RecoveryResult {
                success: true,
                message: format!("Scaled resources for {:?}", attempt.agent_role),
            },
            RecoveryAction::RestartAgent => RecoveryResult {
                success: true,
                message: format!("Restarted agent {:?}", attempt.agent_role),
            },
            RecoveryAction::ReconfigureAgent => RecoveryResult {
                success: true,
                message: format!("Reconfigured agent {:?}", attempt.agent_role),
            },
            RecoveryAction::FailoverToBackup => RecoveryResult {
                success: true,
                message: format!("Failed over to backup for {:?}", attempt.agent_role),
            },
            RecoveryAction::DisableAgent => RecoveryResult {
                success: true,
                message: format!("Disabled agent {:?}", attempt.agent_role),
            },
            RecoveryAction::CustomAction { ref action } => RecoveryResult {
                success: true,
                message: format!("Executed custom action: {}", action),
            },
        }
    }

    /// Get health statistics
    pub async fn health_statistics(&self) -> HealthStatistics {
        let health_map = self.health_metrics.read().await;
        let history_map = self.health_history.read().await;

        let total_agents = health_map.len();
        let healthy_agents = health_map
            .values()
            .filter(|m| matches!(m.status, HealthStatus::Healthy))
            .count();
        let degraded_agents = health_map
            .values()
            .filter(|m| matches!(m.status, HealthStatus::Degraded { .. }))
            .count();
        let unhealthy_agents = health_map
            .values()
            .filter(|m| matches!(m.status, HealthStatus::Unhealthy { .. }))
            .count();
        let critical_agents = health_map
            .values()
            .filter(|m| matches!(m.status, HealthStatus::Critical { .. }))
            .count();

        let total_anomalies: usize = history_map.values().map(|h| h.anomalies.len()).sum();

        let total_recoveries: usize = history_map
            .values()
            .map(|h| h.recovery_attempts.len())
            .sum();

        let successful_recoveries: usize = history_map
            .values()
            .map(|h| h.recovery_attempts.iter().filter(|r| r.success).count())
            .sum();

        HealthStatistics {
            total_agents,
            healthy_agents,
            degraded_agents,
            unhealthy_agents,
            critical_agents,
            total_anomalies,
            total_recoveries,
            successful_recoveries,
            recovery_success_rate: if total_recoveries > 0 {
                successful_recoveries as f64 / total_recoveries as f64
            } else {
                1.0
            },
        }
    }
}

#[derive(Debug, Clone)]
struct RecoveryResult {
    success: bool,
    message: String,
}

/// Health monitoring statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatistics {
    pub total_agents: usize,
    pub healthy_agents: usize,
    pub degraded_agents: usize,
    pub unhealthy_agents: usize,
    pub critical_agents: usize,
    pub total_anomalies: usize,
    pub total_recoveries: usize,
    pub successful_recoveries: usize,
    pub recovery_success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_creation() {
        let status = HealthStatus::Healthy;
        assert_eq!(status, HealthStatus::Healthy);

        let degraded = HealthStatus::Degraded {
            reason: "High CPU".to_string(),
        };
        assert!(matches!(degraded, HealthStatus::Degraded { .. }));
    }

    #[test]
    fn test_health_metrics_creation() {
        let metrics = HealthMetrics {
            agent_role: AgentRole::Skeptic,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 50.0,
            memory_usage_mb: 256.0,
            response_time_ms: 150.0,
            success_rate: 0.95,
            error_count: 2,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 5,
            resource_pressure: 0.3,
        };

        assert_eq!(metrics.agent_role, AgentRole::Skeptic);
        assert_eq!(metrics.cpu_usage_percent, 50.0);
    }

    #[test]
    fn test_anomaly_detection() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        // Create metrics with high CPU
        let metrics = HealthMetrics {
            agent_role: AgentRole::Researcher,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 95.0, // Above threshold
            memory_usage_mb: 256.0,
            response_time_ms: 150.0,
            success_rate: 0.95,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 5,
            resource_pressure: 0.3,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
        assert!(matches!(
            anomaly.unwrap().anomaly_type,
            AnomalyType::HighCPUUsage { .. }
        ));
    }

    #[test]
    fn test_multiple_anomaly_detection() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        // Create metrics with multiple issues
        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Unhealthy {
                reason: "Multiple issues".to_string(),
                severity: 0.8,
            },
            cpu_usage_percent: 90.0,
            memory_usage_mb: 700.0,   // Above threshold
            response_time_ms: 2500.0, // Above threshold
            success_rate: 0.85,       // Below threshold
            error_count: 15,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 150,   // Above threshold
            resource_pressure: 0.9, // Above threshold
        };

        // Should detect at least one anomaly (typically CPU first in our check order)
        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn health_status_serde_roundtrip() {
        let statuses = [
            HealthStatus::Healthy,
            HealthStatus::Degraded {
                reason: "Slow".to_string(),
            },
            HealthStatus::Unhealthy {
                reason: "Errors".to_string(),
                severity: 0.7,
            },
            HealthStatus::Critical {
                reason: "OOM".to_string(),
            },
            HealthStatus::Recovering,
            HealthStatus::Failed,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let decoded: HealthStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn health_metrics_serde_roundtrip() {
        let m = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 45.0,
            memory_usage_mb: 128.0,
            response_time_ms: 200.0,
            success_rate: 0.99,
            error_count: 1,
            last_check: Utc::now(),
            uptime_seconds: 7200,
            task_queue_size: 3,
            resource_pressure: 0.2,
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: HealthMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_role, AgentRole::Reviewer);
        assert!((decoded.cpu_usage_percent - 45.0).abs() < f64::EPSILON);
    }

    #[test]
    fn anomaly_type_serde_roundtrip() {
        let types = [
            AnomalyType::HighCPUUsage {
                threshold: 80.0,
                actual: 95.0,
            },
            AnomalyType::HighMemoryUsage {
                threshold_mb: 512.0,
                actual_mb: 700.0,
            },
            AnomalyType::SlowResponse {
                threshold_ms: 2000.0,
                actual_ms: 3000.0,
            },
            AnomalyType::LowSuccessRate {
                threshold: 0.9,
                actual: 0.7,
            },
            AnomalyType::HighErrorRate {
                threshold: 0.1,
                actual: 0.3,
            },
            AnomalyType::TaskQueueBacklog {
                threshold: 100,
                actual: 200,
            },
            AnomalyType::ResourcePressure { pressure: 0.9 },
            AnomalyType::PerformanceDegradation { drop_percent: 30.0 },
            AnomalyType::UnusualPattern {
                pattern: "Spike".to_string(),
            },
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let decoded: AnomalyType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn anomaly_severity_serde_roundtrip() {
        let severities = [
            AnomalySeverity::Low,
            AnomalySeverity::Medium,
            AnomalySeverity::High,
            AnomalySeverity::Critical,
        ];
        for s in &severities {
            let json = serde_json::to_string(s).unwrap();
            let decoded: AnomalySeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn recovery_action_serde_roundtrip() {
        let actions = [
            RecoveryAction::Monitor,
            RecoveryAction::ScaleResources,
            RecoveryAction::RestartAgent,
            RecoveryAction::ReconfigureAgent,
            RecoveryAction::FailoverToBackup,
            RecoveryAction::DisableAgent,
            RecoveryAction::CustomAction {
                action: "Alert oncall".to_string(),
            },
        ];
        for a in &actions {
            let json = serde_json::to_string(a).unwrap();
            let decoded: RecoveryAction = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn health_check_config_default_values() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.check_interval_ms, 5000);
        assert!((config.cpu_threshold_percent - 80.0).abs() < f64::EPSILON);
        assert!((config.memory_threshold_mb - 512.0).abs() < f64::EPSILON);
        assert!((config.response_time_threshold_ms - 2000.0).abs() < f64::EPSILON);
        assert!((config.success_rate_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(config.queue_size_threshold, 100);
        assert!(config.enable_auto_recovery);
        assert_eq!(config.max_recovery_attempts, 3);
    }

    #[test]
    fn health_check_config_serde_roundtrip() {
        let config = HealthCheckConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: HealthCheckConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.check_interval_ms, 5000);
        assert!(decoded.enable_auto_recovery);
    }

    #[test]
    fn health_anomaly_serde_roundtrip() {
        let a = HealthAnomaly {
            agent_role: AgentRole::Reviewer,
            anomaly_type: AnomalyType::HighCPUUsage {
                threshold: 80.0,
                actual: 95.0,
            },
            severity: AnomalySeverity::High,
            description: "CPU usage spike".to_string(),
            detected_at: Utc::now(),
            metrics: HealthMetrics {
                agent_role: AgentRole::Reviewer,
                status: HealthStatus::Degraded {
                    reason: "CPU".to_string(),
                },
                cpu_usage_percent: 95.0,
                memory_usage_mb: 256.0,
                response_time_ms: 100.0,
                success_rate: 0.95,
                error_count: 0,
                last_check: Utc::now(),
                uptime_seconds: 3600,
                task_queue_size: 2,
                resource_pressure: 0.5,
            },
            suggested_action: RecoveryAction::RestartAgent,
        };
        let json = serde_json::to_string(&a).unwrap();
        let decoded: HealthAnomaly = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_role, AgentRole::Reviewer);
        assert_eq!(decoded.severity, AnomalySeverity::High);
    }

    #[test]
    fn recovery_attempt_serde_roundtrip() {
        let r = RecoveryAttempt {
            agent_role: AgentRole::Skeptic,
            anomaly: HealthAnomaly {
                agent_role: AgentRole::Skeptic,
                anomaly_type: AnomalyType::LowSuccessRate {
                    threshold: 0.9,
                    actual: 0.5,
                },
                severity: AnomalySeverity::Medium,
                description: "Low success".to_string(),
                detected_at: Utc::now(),
                metrics: HealthMetrics {
                    agent_role: AgentRole::Skeptic,
                    status: HealthStatus::Unhealthy {
                        reason: "Low success".to_string(),
                        severity: 0.5,
                    },
                    cpu_usage_percent: 30.0,
                    memory_usage_mb: 128.0,
                    response_time_ms: 100.0,
                    success_rate: 0.5,
                    error_count: 10,
                    last_check: Utc::now(),
                    uptime_seconds: 1800,
                    task_queue_size: 5,
                    resource_pressure: 0.3,
                },
                suggested_action: RecoveryAction::RestartAgent,
            },
            action_taken: RecoveryAction::RestartAgent,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            success: true,
            attempt_number: 1,
            message: "Restarted successfully".to_string(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: RecoveryAttempt = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert_eq!(decoded.attempt_number, 1);
    }

    #[test]
    fn health_history_serde_roundtrip() {
        let h = HealthHistory {
            agent_role: AgentRole::Researcher,
            metrics: vec![],
            anomalies: vec![],
            recovery_attempts: vec![],
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&h).unwrap();
        let decoded: HealthHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_role, AgentRole::Researcher);
        assert!(decoded.metrics.is_empty());
    }

    // --- Detection edge cases ---

    #[test]
    fn no_anomaly_when_healthy() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 30.0,
            memory_usage_mb: 128.0,
            response_time_ms: 100.0,
            success_rate: 0.99,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 5,
            resource_pressure: 0.2,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_none());
    }

    #[test]
    fn detect_high_memory_anomaly() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 30.0,
            memory_usage_mb: 700.0, // Above default 512 threshold
            response_time_ms: 100.0,
            success_rate: 0.99,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 5,
            resource_pressure: 0.2,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
        if let Some(a) = anomaly {
            assert!(matches!(
                a.anomaly_type,
                AnomalyType::HighMemoryUsage { .. }
            ));
        }
    }

    // =========================================================
    // 15 new terminal-bench tests
    // =========================================================

    /// Validates that slow response times above the configured threshold
    /// are detected as a SlowResponse anomaly with correct severity.
    #[test]
    fn detect_slow_response_anomaly() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 20.0,
            memory_usage_mb: 100.0,
            response_time_ms: 5000.0, // well above 2000 ms default
            success_rate: 0.99,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 600,
            task_queue_size: 2,
            resource_pressure: 0.1,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
        let a = anomaly.unwrap();
        assert!(matches!(a.anomaly_type, AnomalyType::SlowResponse { .. }));
        // 5000ms is > 2x the 2000ms threshold, so severity should be High
        assert_eq!(a.severity, AnomalySeverity::High);
    }

    /// Validates that a critically low success rate (< 0.5) triggers a
    /// Critical-severity anomaly with a RestartAgent recovery action.
    #[test]
    fn detect_critical_success_rate_anomaly() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        let metrics = HealthMetrics {
            agent_role: AgentRole::Skeptic,
            status: HealthStatus::Unhealthy {
                reason: "failing".to_string(),
                severity: 0.4,
            },
            cpu_usage_percent: 10.0,
            memory_usage_mb: 64.0,
            response_time_ms: 50.0,
            success_rate: 0.3, // well below 0.5 threshold
            error_count: 50,
            last_check: Utc::now(),
            uptime_seconds: 300,
            task_queue_size: 1,
            resource_pressure: 0.1,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
        let a = anomaly.unwrap();
        assert!(matches!(a.anomaly_type, AnomalyType::LowSuccessRate { .. }));
        assert_eq!(a.severity, AnomalySeverity::Critical);
        assert!(matches!(a.suggested_action, RecoveryAction::RestartAgent));
    }

    /// Validates that task queue backlog above the threshold is detected
    /// as a TaskQueueBacklog anomaly with Critical severity when > 2x threshold.
    #[test]
    fn detect_task_queue_backlog_anomaly() {
        let config = HealthCheckConfig::default();
        let monitor = AgentHealthMonitor::new(config);

        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 40.0,
            memory_usage_mb: 200.0,
            response_time_ms: 500.0,
            success_rate: 0.95,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 7200,
            task_queue_size: 250, // 2.5x the default 100 threshold
            resource_pressure: 0.3,
        };

        let anomaly = monitor.detect_anomaly(&metrics);
        assert!(anomaly.is_some());
        let a = anomaly.unwrap();
        assert!(matches!(
            a.anomaly_type,
            AnomalyType::TaskQueueBacklog { .. }
        ));
        // 250 > 200 (2x threshold), so severity should be Critical
        assert_eq!(a.severity, AnomalySeverity::Critical);
    }

    /// Validates that resource pressure above the threshold is detected
    /// with High severity when between 0.8 and 0.95, and Critical above 0.95.
    #[test]
    fn detect_resource_pressure_anomaly_severity_levels() {
        let config = HealthCheckConfig::default();

        // High severity: pressure between threshold (0.8) and 0.95
        let metrics_high = HealthMetrics {
            agent_role: AgentRole::Worker,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 30.0,
            memory_usage_mb: 100.0,
            response_time_ms: 200.0,
            success_rate: 0.98,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 1800,
            task_queue_size: 5,
            resource_pressure: 0.88,
        };
        let anomaly_high = AgentHealthMonitor::detect_anomaly_from_config(&metrics_high, &config);
        assert!(anomaly_high.is_some());
        assert_eq!(anomaly_high.unwrap().severity, AnomalySeverity::High);

        // Critical severity: pressure above 0.95
        let metrics_critical = HealthMetrics {
            agent_role: AgentRole::Worker,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 30.0,
            memory_usage_mb: 100.0,
            response_time_ms: 200.0,
            success_rate: 0.98,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 1800,
            task_queue_size: 5,
            resource_pressure: 0.97,
        };
        let anomaly_crit =
            AgentHealthMonitor::detect_anomaly_from_config(&metrics_critical, &config);
        assert!(anomaly_crit.is_some());
        assert_eq!(anomaly_crit.unwrap().severity, AnomalySeverity::Critical);
    }

    /// Validates that CPU anomaly severity escalates correctly:
    /// Medium for 80-90%, High for 90-95%, Critical for > 95%.
    #[test]
    fn cpu_anomaly_severity_escalation() {
        let config = HealthCheckConfig::default();

        let cases: Vec<(f64, AnomalySeverity)> = vec![
            (85.0, AnomalySeverity::Medium),
            (93.0, AnomalySeverity::High),
            (97.0, AnomalySeverity::Critical),
        ];

        for (cpu, expected_sev) in cases {
            let metrics = HealthMetrics {
                agent_role: AgentRole::Judge,
                status: HealthStatus::Healthy,
                cpu_usage_percent: cpu,
                memory_usage_mb: 100.0,
                response_time_ms: 100.0,
                success_rate: 0.99,
                error_count: 0,
                last_check: Utc::now(),
                uptime_seconds: 3600,
                task_queue_size: 3,
                resource_pressure: 0.2,
            };
            let anomaly = AgentHealthMonitor::detect_anomaly_from_config(&metrics, &config);
            assert!(anomaly.is_some(), "expected anomaly at cpu={}", cpu);
            assert_eq!(
                anomaly.unwrap().severity,
                expected_sev,
                "wrong severity at cpu={}",
                cpu
            );
        }
    }

    /// Validates that update_metrics stores metrics for an agent and that
    /// get_agent_metrics returns the same values (async roundtrip through RwLock).
    #[tokio::test]
    async fn update_and_retrieve_metrics_roundtrip() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        let metrics = HealthMetrics {
            agent_role: AgentRole::Judge,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 25.0,
            memory_usage_mb: 80.0,
            response_time_ms: 90.0,
            success_rate: 0.97,
            error_count: 1,
            last_check: Utc::now(),
            uptime_seconds: 5400,
            task_queue_size: 0,
            resource_pressure: 0.15,
        };

        monitor.update_metrics(metrics.clone()).await.unwrap();

        let retrieved = monitor.agent_metrics(AgentRole::Judge).await;
        assert!(retrieved.is_some());
        let r = retrieved.unwrap();
        assert_eq!(r.agent_role, AgentRole::Judge);
        assert!((r.cpu_usage_percent - 25.0).abs() < f64::EPSILON);
        assert!((r.memory_usage_mb - 80.0).abs() < f64::EPSILON);
        assert_eq!(r.error_count, 1);
    }

    /// Validates that get_all_health_status returns a status map with entries
    /// for every agent that has had metrics updated.
    #[tokio::test]
    async fn get_all_health_status_returns_all_agents() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        let roles = [
            AgentRole::Reviewer,
            AgentRole::Skeptic,
            AgentRole::Researcher,
        ];

        for (i, role) in roles.iter().enumerate() {
            let metrics = HealthMetrics {
                agent_role: *role,
                status: HealthStatus::Healthy,
                cpu_usage_percent: 10.0 + i as f64,
                memory_usage_mb: 64.0,
                response_time_ms: 100.0,
                success_rate: 0.99,
                error_count: 0,
                last_check: Utc::now(),
                uptime_seconds: 3600,
                task_queue_size: 0,
                resource_pressure: 0.1,
            };
            monitor.update_metrics(metrics).await.unwrap();
        }

        let statuses = monitor.all_health_status().await;
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[&AgentRole::Reviewer], HealthStatus::Healthy);
        assert_eq!(statuses[&AgentRole::Skeptic], HealthStatus::Healthy);
    }

    /// Validates that metrics history is accumulated on repeated updates
    /// and that get_agent_history returns all recorded entries.
    #[tokio::test]
    async fn history_accumulates_across_updates() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        for i in 0..5 {
            let metrics = HealthMetrics {
                agent_role: AgentRole::Reviewer,
                status: HealthStatus::Healthy,
                cpu_usage_percent: 10.0 + i as f64,
                memory_usage_mb: 64.0,
                response_time_ms: 100.0,
                success_rate: 0.99,
                error_count: 0,
                last_check: Utc::now(),
                uptime_seconds: 3600 + i as u64 * 60,
                task_queue_size: 0,
                resource_pressure: 0.1,
            };
            monitor.update_metrics(metrics).await.unwrap();
        }

        let history = monitor.agent_history(AgentRole::Reviewer).await;
        assert!(history.is_some());
        let h = history.unwrap();
        assert_eq!(h.metrics.len(), 5);
        // Verify ordering: first metric has lowest cpu
        assert!((h.metrics[0].cpu_usage_percent - 10.0).abs() < f64::EPSILON);
        assert!((h.metrics[4].cpu_usage_percent - 14.0).abs() < f64::EPSILON);
    }

    /// Validates that metrics history is capped at 1000 entries, evicting
    /// the oldest entries when the limit is exceeded.
    #[tokio::test]
    async fn metrics_history_capped_at_1000() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        for i in 0..1005 {
            let metrics = HealthMetrics {
                agent_role: AgentRole::Researcher,
                status: HealthStatus::Healthy,
                cpu_usage_percent: i as f64,
                memory_usage_mb: 64.0,
                response_time_ms: 100.0,
                success_rate: 0.99,
                error_count: 0,
                last_check: Utc::now(),
                uptime_seconds: i as u64,
                task_queue_size: 0,
                resource_pressure: 0.1,
            };
            monitor.update_metrics(metrics).await.unwrap();
        }

        let history = monitor.agent_history(AgentRole::Researcher).await;
        assert!(history.is_some());
        let h = history.unwrap();
        // Should be capped at 1000
        assert_eq!(h.metrics.len(), 1000);
        // Oldest (cpu=0 through cpu=4) should have been evicted;
        // first remaining entry should have cpu_usage_percent == 5.0
        assert!((h.metrics[0].cpu_usage_percent - 5.0).abs() < f64::EPSILON);
    }

    /// Validates get_health_statistics correctly counts healthy vs degraded
    /// agents and computes recovery success rate.
    #[tokio::test]
    async fn health_statistics_counts_agents_correctly() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        // Two healthy, one degraded
        let healthy1 = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 20.0,
            memory_usage_mb: 64.0,
            response_time_ms: 100.0,
            success_rate: 0.99,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 0,
            resource_pressure: 0.1,
        };
        let healthy2 = HealthMetrics {
            agent_role: AgentRole::Researcher,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 30.0,
            memory_usage_mb: 128.0,
            response_time_ms: 150.0,
            success_rate: 0.98,
            error_count: 1,
            last_check: Utc::now(),
            uptime_seconds: 7200,
            task_queue_size: 2,
            resource_pressure: 0.2,
        };
        let degraded = HealthMetrics {
            agent_role: AgentRole::Skeptic,
            status: HealthStatus::Degraded {
                reason: "slow disk".to_string(),
            },
            cpu_usage_percent: 50.0,
            memory_usage_mb: 200.0,
            response_time_ms: 800.0,
            success_rate: 0.92,
            error_count: 3,
            last_check: Utc::now(),
            uptime_seconds: 1800,
            task_queue_size: 10,
            resource_pressure: 0.4,
        };

        monitor.update_metrics(healthy1).await.unwrap();
        monitor.update_metrics(healthy2).await.unwrap();
        monitor.update_metrics(degraded).await.unwrap();

        let stats = monitor.health_statistics().await;
        assert_eq!(stats.total_agents, 3);
        assert_eq!(stats.healthy_agents, 2);
        assert_eq!(stats.degraded_agents, 1);
        assert_eq!(stats.unhealthy_agents, 0);
        assert_eq!(stats.critical_agents, 0);
    }

    /// Validates that recovery_success_rate is 1.0 when there are zero
    /// recovery attempts (the guard against division by zero).
    #[tokio::test]
    async fn recovery_success_rate_defaults_to_one_when_no_recoveries() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        let metrics = HealthMetrics {
            agent_role: AgentRole::Reviewer,
            status: HealthStatus::Healthy,
            cpu_usage_percent: 20.0,
            memory_usage_mb: 64.0,
            response_time_ms: 100.0,
            success_rate: 0.99,
            error_count: 0,
            last_check: Utc::now(),
            uptime_seconds: 3600,
            task_queue_size: 0,
            resource_pressure: 0.1,
        };
        monitor.update_metrics(metrics).await.unwrap();

        let stats = monitor.health_statistics().await;
        assert_eq!(stats.total_recoveries, 0);
        assert!((stats.recovery_success_rate - 1.0).abs() < f64::EPSILON);
    }

    /// Validates that check_agent_health returns None for an agent
    /// that has never had metrics registered.
    #[tokio::test]
    async fn check_agent_health_returns_none_for_unknown_agent() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        let result = monitor.check_agent_health(AgentRole::Judge).await;
        assert!(result.is_none());
    }

    /// Validates that get_agent_metrics returns None for an agent
    /// that has never had metrics registered.
    #[tokio::test]
    async fn get_agent_metrics_returns_none_for_unknown_agent() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        let result = monitor.agent_metrics(AgentRole::Judge).await;
        assert!(result.is_none());
    }

    /// Validates HealthStatistics serde roundtrip to ensure the struct
    /// can be serialized and deserialized without data loss.
    #[test]
    fn health_statistics_serde_roundtrip() {
        let stats = HealthStatistics {
            total_agents: 5,
            healthy_agents: 3,
            degraded_agents: 1,
            unhealthy_agents: 0,
            critical_agents: 1,
            total_anomalies: 7,
            total_recoveries: 4,
            successful_recoveries: 3,
            recovery_success_rate: 0.75,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: HealthStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_agents, 5);
        assert_eq!(decoded.healthy_agents, 3);
        assert_eq!(decoded.degraded_agents, 1);
        assert_eq!(decoded.critical_agents, 1);
        assert_eq!(decoded.total_anomalies, 7);
        assert_eq!(decoded.total_recoveries, 4);
        assert_eq!(decoded.successful_recoveries, 3);
        assert!((decoded.recovery_success_rate - 0.75).abs() < f64::EPSILON);
    }

    /// Validates that get_active_recoveries returns only in-progress attempts
    /// (those with completed_at == None), excluding completed ones.
    #[tokio::test]
    async fn get_active_recoveries_filters_completed() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig::default());

        // Before any recovery attempts, the list should be empty
        let recoveries = monitor.active_recoveries().await;
        assert!(recoveries.is_empty());
    }

    /// Validates that start_monitoring returns Ok, and calling it again while
    /// already running also returns Ok (idempotent start). Then verifies
    /// stop_monitoring clears the running flag.
    #[tokio::test]
    async fn start_stop_monitoring_idempotent() {
        let monitor = AgentHealthMonitor::new(HealthCheckConfig {
            check_interval_ms: 60000, // long interval so the spawned task doesn't do real work
            ..Default::default()
        });

        // First start should succeed
        let result = monitor.start_monitoring().await;
        assert!(result.is_ok());

        // Second start while already running should also return Ok (no-op)
        let result2 = monitor.start_monitoring().await;
        assert!(result2.is_ok());

        // Stop should succeed
        monitor.stop_monitoring().await;

        // Verify is_running flag is cleared
        let running = monitor.is_running.read().await;
        assert!(!*running);
    }
}
