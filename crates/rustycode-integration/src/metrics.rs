#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::match_wildcard_for_single_variants,
    clippy::float_cmp
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Integration metrics collector
#[derive(Debug)]
pub struct MetricsCollector {
    /// Collected metrics
    metrics: tokio::sync::Mutex<IntegrationMetrics>,
    /// Metrics configuration
    config: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// Metrics retention period in days
    pub retention_days: u32,
    /// Whether to collect detailed per-task metrics
    pub collect_detailed_metrics: bool,
    pub alert_thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum acceptable error rate
    pub max_error_rate: f64,
    /// Minimum acceptable success rate
    pub min_success_rate: f64,
    /// Maximum acceptable average execution time
    pub max_avg_execution_time_seconds: f64,
    /// Alert if orchestration is slower than legacy by this ratio
    pub max_orchestration_slowdown_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationMetrics {
    /// Overall routing statistics
    pub routing_stats: RoutingStats,
    /// Shadow mode comparison statistics
    pub shadow_stats: ShadowComparisonStats,
    pub performance_metrics: PerformanceMetrics,
    pub error_tracking: ErrorTracking,
    /// Collection metadata
    pub metadata: MetricsMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStats {
    pub total_tasks_routed: u64,
    pub orchestration_routed: u64,
    pub legacy_routed: u64,
    pub rejected_tasks: u64,
    pub routing_accuracy_estimate: f64,
    pub avg_classification_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowComparisonStats {
    pub total_comparisons: u64,
    pub orchestration_better: u64,
    pub legacy_better: u64,
    pub equivalent_performance: u64,
    pub both_failed: u64,
    pub avg_time_improvement_percent: f64,
    pub avg_cost_improvement_percent: f64,
    pub avg_success_rate_improvement: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub avg_orchestration_execution_time: f64,
    pub avg_legacy_execution_time: f64,
    pub avg_orchestration_cost: f64,
    pub avg_legacy_cost: f64,
    pub orchestration_success_rate: f64,
    pub legacy_success_rate: f64,
    pub peak_concurrent_executions: u32,
    pub resource_utilization_trends: Vec<ResourceTrend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTrend {
    pub timestamp: DateTime<Utc>,
    pub cpu_utilization_percent: f64,
    pub memory_utilization_percent: f64,
    pub active_orchestration_tasks: u32,
    pub active_legacy_tasks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTracking {
    pub total_errors: u64,
    pub orchestration_errors: u64,
    pub legacy_errors: u64,
    pub routing_errors: u64,
    pub top_error_categories: Vec<ErrorCategoryCount>,
    pub error_rate_trend: Vec<ErrorRatePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCategoryCount {
    pub category: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRatePoint {
    pub timestamp: DateTime<Utc>,
    pub error_rate: f64,
    pub orchestration_error_rate: f64,
    pub legacy_error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsMetadata {
    pub collection_start_time: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub version: String,
    pub active_alerts: Vec<String>,
}

/// Metrics alert types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsAlert {
    HighErrorRate {
        current_rate: f64,
        threshold: f64,
        affected_system: String,
    },
    PerformanceDegradation {
        metric_name: String,
        degradation_percent: f64,
        baseline_value: f64,
        current_value: f64,
    },
    RoutingInaccuracy {
        accuracy_drop: f64,
        expected_accuracy: f64,
        current_accuracy: f64,
    },
    ResourceExhaustion {
        resource_type: String,
        utilization_percent: f64,
        threshold_percent: f64,
    },
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 0.1,                   // 10% error rate
                min_success_rate: 0.85,                // 85% success rate
                max_avg_execution_time_seconds: 300.0, // 5 minutes
                max_orchestration_slowdown_ratio: 2.0, // 2x slower max
            },
        }
    }
}

impl MetricsCollector {
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            metrics: tokio::sync::Mutex::new(IntegrationMetrics {
                routing_stats: RoutingStats {
                    total_tasks_routed: 0,
                    orchestration_routed: 0,
                    legacy_routed: 0,
                    rejected_tasks: 0,
                    routing_accuracy_estimate: 0.8, // Initial estimate
                    avg_classification_confidence: 0.7,
                },
                shadow_stats: ShadowComparisonStats {
                    total_comparisons: 0,
                    orchestration_better: 0,
                    legacy_better: 0,
                    equivalent_performance: 0,
                    both_failed: 0,
                    avg_time_improvement_percent: 0.0,
                    avg_cost_improvement_percent: 0.0,
                    avg_success_rate_improvement: 0.0,
                },
                performance_metrics: PerformanceMetrics {
                    avg_orchestration_execution_time: 0.0,
                    avg_legacy_execution_time: 0.0,
                    avg_orchestration_cost: 0.0,
                    avg_legacy_cost: 0.0,
                    orchestration_success_rate: 0.0,
                    legacy_success_rate: 0.0,
                    peak_concurrent_executions: 0,
                    resource_utilization_trends: vec![],
                },
                error_tracking: ErrorTracking {
                    total_errors: 0,
                    orchestration_errors: 0,
                    legacy_errors: 0,
                    routing_errors: 0,
                    top_error_categories: vec![],
                    error_rate_trend: vec![],
                },
                metadata: MetricsMetadata {
                    collection_start_time: Utc::now(),
                    last_updated: Utc::now(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    active_alerts: vec![],
                },
            }),
            config,
        }
    }

    /// Record a routing decision
    pub async fn record_routing(
        &self,
        routing_decision: &crate::router::RoutingDecision,
        classification_confidence: f64,
    ) {
        if !self.config.enabled {
            return;
        }

        let mut metrics = self.metrics.lock().await;
        metrics.routing_stats.total_tasks_routed += 1;

        match routing_decision {
            crate::router::RoutingDecision::Orchestration { .. } => {
                metrics.routing_stats.orchestration_routed += 1;
            }
            crate::router::RoutingDecision::Legacy { .. } => {
                metrics.routing_stats.legacy_routed += 1;
            }
            crate::router::RoutingDecision::Reject { .. } => {
                metrics.routing_stats.rejected_tasks += 1;
            }
        }

        // Update average confidence
        let total = metrics.routing_stats.total_tasks_routed;
        metrics.routing_stats.avg_classification_confidence =
            (metrics.routing_stats.avg_classification_confidence * (total - 1) as f64
                + classification_confidence)
                / total as f64;

        metrics.metadata.last_updated = Utc::now();
    }

    /// Record execution results
    pub async fn record_execution(&self, result: &crate::router::TaskExecutionResult) {
        if !self.config.enabled {
            return;
        }

        let mut metrics = self.metrics.lock().await;

        match &result.result {
            crate::router::ExecutionOutcome::Success(_) => match result.execution_path {
                crate::router::ExecutionPath::Orchestration => {
                    let prev_count = metrics.routing_stats.orchestration_routed.saturating_sub(1);
                    metrics.performance_metrics.orchestration_success_rate =
                        (metrics.performance_metrics.orchestration_success_rate
                            * prev_count as f64
                            + 1.0)
                            / metrics.routing_stats.orchestration_routed.max(1) as f64;
                }
                crate::router::ExecutionPath::Legacy => {
                    let prev_count = metrics.routing_stats.legacy_routed.saturating_sub(1);
                    metrics.performance_metrics.legacy_success_rate =
                        (metrics.performance_metrics.legacy_success_rate * prev_count as f64 + 1.0)
                            / metrics.routing_stats.legacy_routed.max(1) as f64;
                }
                _ => {}
            },
            crate::router::ExecutionOutcome::Failure(_) => {
                match result.execution_path {
                    crate::router::ExecutionPath::Orchestration => {
                        metrics.error_tracking.orchestration_errors += 1;
                    }
                    crate::router::ExecutionPath::Legacy => {
                        metrics.error_tracking.legacy_errors += 1;
                    }
                    _ => {}
                }
                metrics.error_tracking.total_errors += 1;
            }
        }

        if let Some(metadata) = &result.execution_metadata {
            if matches!(
                result.execution_path,
                crate::router::ExecutionPath::Orchestration
            ) {
                let total = metrics.routing_stats.orchestration_routed.max(1) as f64;
                let perf = &mut metrics.performance_metrics;
                perf.avg_orchestration_execution_time = (perf.avg_orchestration_execution_time
                    * (total - 1.0)
                    + metadata.wall_time_secs)
                    / total;
                if let Some(cost) = metadata.cost_usd {
                    perf.avg_orchestration_cost =
                        (perf.avg_orchestration_cost * (total - 1.0) + cost) / total;
                }
            }
        }

        metrics.metadata.last_updated = Utc::now();
    }

    /// Record shadow mode comparison
    pub async fn record_shadow_comparison(
        &self,
        comparison: &crate::shadow_mode::ShadowExecutionResult,
    ) {
        if !self.config.enabled {
            return;
        }

        let mut metrics = self.metrics.lock().await;
        metrics.shadow_stats.total_comparisons += 1;

        match comparison.comparison.recommendation {
            crate::shadow_mode::ShadowRecommendation::PreferOrchestration => {
                metrics.shadow_stats.orchestration_better += 1;
            }
            crate::shadow_mode::ShadowRecommendation::PreferLegacy => {
                metrics.shadow_stats.legacy_better += 1;
            }
            crate::shadow_mode::ShadowRecommendation::BothViable => {
                metrics.shadow_stats.equivalent_performance += 1;
            }
            crate::shadow_mode::ShadowRecommendation::BothPoor => {
                metrics.shadow_stats.both_failed += 1;
            }
            _ => {}
        }

        metrics.metadata.last_updated = Utc::now();
    }

    /// Check for alerts based on current metrics
    pub async fn check_alerts(&self) -> Vec<MetricsAlert> {
        if !self.config.enabled {
            return vec![];
        }

        let metrics = self.metrics.lock().await;
        let mut alerts = vec![];

        // Check error rates
        let total_tasks = metrics.routing_stats.total_tasks_routed as f64;
        if total_tasks > 10.0 {
            let error_rate = metrics.error_tracking.total_errors as f64 / total_tasks;
            if error_rate > self.config.alert_thresholds.max_error_rate {
                alerts.push(MetricsAlert::HighErrorRate {
                    current_rate: error_rate,
                    threshold: self.config.alert_thresholds.max_error_rate,
                    affected_system: "integration".to_string(),
                });
            }
        }

        // Check success rates
        if metrics.performance_metrics.orchestration_success_rate
            < self.config.alert_thresholds.min_success_rate
        {
            alerts.push(MetricsAlert::PerformanceDegradation {
                metric_name: "orchestration_success_rate".to_string(),
                degradation_percent: (self.config.alert_thresholds.min_success_rate
                    - metrics.performance_metrics.orchestration_success_rate)
                    * 100.0,
                baseline_value: self.config.alert_thresholds.min_success_rate,
                current_value: metrics.performance_metrics.orchestration_success_rate,
            });
        }

        // Check performance degradation
        if metrics.performance_metrics.avg_orchestration_execution_time
            > self.config.alert_thresholds.max_avg_execution_time_seconds
        {
            alerts.push(MetricsAlert::PerformanceDegradation {
                metric_name: "avg_orchestration_execution_time".to_string(),
                degradation_percent: (metrics.performance_metrics.avg_orchestration_execution_time
                    / self.config.alert_thresholds.max_avg_execution_time_seconds
                    - 1.0)
                    * 100.0,
                baseline_value: self.config.alert_thresholds.max_avg_execution_time_seconds,
                current_value: metrics.performance_metrics.avg_orchestration_execution_time,
            });
        }

        alerts
    }

    /// Generate a metrics report
    pub async fn generate_report(&self) -> MetricsReport {
        let metrics = self.metrics.lock().await;

        MetricsReport {
            summary: MetricsSummary {
                total_tasks_processed: metrics.routing_stats.total_tasks_routed,
                orchestration_adoption_rate: if metrics.routing_stats.total_tasks_routed > 0 {
                    metrics.routing_stats.orchestration_routed as f64
                        / metrics.routing_stats.total_tasks_routed as f64
                } else {
                    0.0
                },
                overall_success_rate: if metrics.routing_stats.total_tasks_routed > 0 {
                    (metrics.routing_stats.total_tasks_routed - metrics.error_tracking.total_errors)
                        as f64
                        / metrics.routing_stats.total_tasks_routed as f64
                } else {
                    0.0
                },
                avg_routing_confidence: metrics.routing_stats.avg_classification_confidence,
            },
            routing_breakdown: metrics.routing_stats.clone(),
            performance_comparison: PerformanceComparison {
                orchestration_vs_legacy_time: if metrics
                    .performance_metrics
                    .avg_legacy_execution_time
                    > 0.0
                {
                    metrics.performance_metrics.avg_orchestration_execution_time
                        / metrics.performance_metrics.avg_legacy_execution_time
                } else {
                    1.0
                },
                orchestration_vs_legacy_cost: if metrics.performance_metrics.avg_legacy_cost > 0.0 {
                    metrics.performance_metrics.avg_orchestration_cost
                        / metrics.performance_metrics.avg_legacy_cost
                } else {
                    1.0
                },
                orchestration_vs_legacy_success: metrics
                    .performance_metrics
                    .orchestration_success_rate
                    - metrics.performance_metrics.legacy_success_rate,
            },
            recommendations: self.generate_recommendations(&metrics),
            generated_at: Utc::now(),
        }
    }

    /// Generate recommendations based on metrics
    #[allow(clippy::unused_self)]
    fn generate_recommendations(&self, metrics: &IntegrationMetrics) -> Vec<String> {
        let mut recommendations = vec![];

        // Routing accuracy
        if metrics.routing_stats.routing_accuracy_estimate < 0.7 {
            recommendations.push(
                "Consider improving task classification rules to increase routing accuracy"
                    .to_string(),
            );
        }

        // Performance comparison
        if metrics.shadow_stats.orchestration_better > metrics.shadow_stats.legacy_better * 2 {
            recommendations.push("Orchestration shows strong performance advantages - consider increasing adoption rate".to_string());
        } else if metrics.shadow_stats.legacy_better > metrics.shadow_stats.orchestration_better * 2
        {
            recommendations.push(
                "Legacy execution shows better performance - review orchestration implementation"
                    .to_string(),
            );
        }

        // Error rates
        if metrics.error_tracking.orchestration_errors > metrics.error_tracking.legacy_errors * 2 {
            recommendations.push(
                "Orchestration has higher error rate than legacy - investigate root causes"
                    .to_string(),
            );
        }

        // Resource utilization
        if metrics.performance_metrics.avg_orchestration_cost
            > metrics.performance_metrics.avg_legacy_cost * 2.0
        {
            recommendations.push(
                "Orchestration is significantly more expensive - optimize cost efficiency"
                    .to_string(),
            );
        }

        if recommendations.is_empty() {
            recommendations.push("Continue monitoring - no immediate recommendations".to_string());
        }

        recommendations
    }

    /// Get current metrics
    pub async fn metrics(&self) -> IntegrationMetrics {
        self.metrics.lock().await.clone()
    }
}

/// Metrics report for stakeholders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub summary: MetricsSummary,
    pub routing_breakdown: RoutingStats,
    pub performance_comparison: PerformanceComparison,
    pub recommendations: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

/// Metrics summary for quick overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub total_tasks_processed: u64,
    pub orchestration_adoption_rate: f64,
    pub overall_success_rate: f64,
    pub avg_routing_confidence: f64,
}

/// Performance comparison between orchestration and legacy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceComparison {
    pub orchestration_vs_legacy_time: f64,
    pub orchestration_vs_legacy_cost: f64,
    pub orchestration_vs_legacy_success: f64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::router::{
        ExecutionMetadata, ExecutionOutcome, ExecutionPath, RoutingDecision, TaskExecutionResult,
    };
    use crate::shadow_mode::{ShadowComparison, ShadowExecutionResult, ShadowRecommendation};

    fn create_metrics_collector() -> MetricsCollector {
        MetricsCollector::new(MetricsConfig::default())
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = create_metrics_collector();

        let routing_decision = RoutingDecision::Orchestration {
            starting_tier: 2,
            reasoning: "Complex task".to_string(),
        };
        collector.record_routing(&routing_decision, 0.8).await;

        let legacy_decision = RoutingDecision::Legacy {
            reasoning: "Simple task".to_string(),
        };
        collector.record_routing(&legacy_decision, 0.9).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.routing_stats.total_tasks_routed, 2);
        assert_eq!(metrics.routing_stats.orchestration_routed, 1);
        assert_eq!(metrics.routing_stats.legacy_routed, 1);
    }

    #[test]
    fn test_config_default() {
        let config = MetricsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.retention_days, 30);
        assert!(config.collect_detailed_metrics);
        assert_eq!(config.alert_thresholds.max_error_rate, 0.1);
        assert_eq!(config.alert_thresholds.min_success_rate, 0.85);
        assert_eq!(
            config.alert_thresholds.max_avg_execution_time_seconds,
            300.0
        );
        assert_eq!(
            config.alert_thresholds.max_orchestration_slowdown_ratio,
            2.0
        );
    }

    #[tokio::test]
    async fn test_routing_confidence_updates() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;
        collector
            .record_routing(
                &RoutingDecision::Legacy {
                    reasoning: String::new(),
                },
                0.7,
            )
            .await;

        let metrics = collector.metrics().await;
        let expected = f64::midpoint(0.7 * 1.0, 0.9);
        assert!((metrics.routing_stats.avg_classification_confidence - expected).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_reject_routing() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Reject {
                    reasoning: "Too complex".to_string(),
                },
                0.3,
            )
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.routing_stats.rejected_tasks, 1);
        assert_eq!(metrics.routing_stats.total_tasks_routed, 1);
    }

    #[tokio::test]
    async fn test_execution_success_orchestration() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Orchestration,
            result: ExecutionOutcome::Success(Some("done".to_string())),
            execution_metadata: Some(ExecutionMetadata {
                wall_time_secs: 5.0,
                tokens_used: Some(100),
                cost_usd: Some(0.01),
            }),
        };
        collector.record_execution(&result).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.error_tracking.total_errors, 0);
        assert!(metrics.performance_metrics.avg_orchestration_execution_time > 0.0);
    }

    #[tokio::test]
    async fn test_execution_failure_legacy() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Legacy {
                    reasoning: String::new(),
                },
                0.8,
            )
            .await;

        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Legacy,
            result: ExecutionOutcome::Failure("timeout".to_string()),
            execution_metadata: None,
        };
        collector.record_execution(&result).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.error_tracking.total_errors, 1);
        assert_eq!(metrics.error_tracking.legacy_errors, 1);
    }

    #[tokio::test]
    async fn test_shadow_comparison_orchestration_preferred() {
        let collector = create_metrics_collector();

        let comparison = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::PreferOrchestration,
                orchestration_time_secs: Some(5.0),
                legacy_time_secs: Some(10.0),
                orchestration_success: true,
                legacy_success: true,
            },
        };
        collector.record_shadow_comparison(&comparison).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.total_comparisons, 1);
        assert_eq!(metrics.shadow_stats.orchestration_better, 1);
    }

    #[tokio::test]
    async fn test_shadow_comparison_legacy_preferred() {
        let collector = create_metrics_collector();

        let comparison = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::PreferLegacy,
                orchestration_time_secs: Some(20.0),
                legacy_time_secs: Some(5.0),
                orchestration_success: true,
                legacy_success: true,
            },
        };
        collector.record_shadow_comparison(&comparison).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.legacy_better, 1);
    }

    #[tokio::test]
    async fn test_shadow_comparison_both_viable() {
        let collector = create_metrics_collector();

        let comparison = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::BothViable,
                orchestration_time_secs: Some(5.0),
                legacy_time_secs: Some(5.0),
                orchestration_success: true,
                legacy_success: true,
            },
        };
        collector.record_shadow_comparison(&comparison).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.equivalent_performance, 1);
    }

    #[tokio::test]
    async fn test_shadow_comparison_both_poor() {
        let collector = create_metrics_collector();

        let comparison = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::BothPoor,
                orchestration_time_secs: None,
                legacy_time_secs: None,
                orchestration_success: false,
                legacy_success: false,
            },
        };
        collector.record_shadow_comparison(&comparison).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.both_failed, 1);
    }

    #[tokio::test]
    async fn test_alerts_empty_initially() {
        let config = MetricsConfig {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 1.0,
                min_success_rate: 0.0,
                max_avg_execution_time_seconds: f64::MAX,
                max_orchestration_slowdown_ratio: f64::MAX,
            },
        };
        let collector = MetricsCollector::new(config);
        let alerts = collector.check_alerts().await;
        assert!(alerts.is_empty());
    }

    #[tokio::test]
    async fn test_alerts_high_error_rate() {
        let config = MetricsConfig {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 0.1,
                min_success_rate: 0.0,
                max_avg_execution_time_seconds: f64::MAX,
                max_orchestration_slowdown_ratio: f64::MAX,
            },
        };
        let collector = MetricsCollector::new(config);

        // Route 20 tasks, then fail 3 (15% error rate > 10% threshold)
        for _ in 0..17 {
            collector
                .record_routing(
                    &RoutingDecision::Orchestration {
                        starting_tier: 1,
                        reasoning: String::new(),
                    },
                    0.9,
                )
                .await;
            collector
                .record_execution(&TaskExecutionResult {
                    execution_path: ExecutionPath::Orchestration,
                    result: ExecutionOutcome::Success(None),
                    execution_metadata: None,
                })
                .await;
        }
        for _ in 0..3 {
            collector
                .record_routing(
                    &RoutingDecision::Legacy {
                        reasoning: String::new(),
                    },
                    0.8,
                )
                .await;
            collector
                .record_execution(&TaskExecutionResult {
                    execution_path: ExecutionPath::Legacy,
                    result: ExecutionOutcome::Failure("err".to_string()),
                    execution_metadata: None,
                })
                .await;
        }

        let alerts = collector.check_alerts().await;
        assert!(alerts
            .iter()
            .any(|a| matches!(a, MetricsAlert::HighErrorRate { .. })));
    }

    #[tokio::test]
    async fn test_metrics_disabled() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let collector = MetricsCollector::new(config);

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.routing_stats.total_tasks_routed, 0);
    }

    #[tokio::test]
    async fn test_generate_report() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;
        collector
            .record_routing(
                &RoutingDecision::Legacy {
                    reasoning: String::new(),
                },
                0.7,
            )
            .await;

        let report = collector.generate_report().await;
        assert_eq!(report.summary.total_tasks_processed, 2);
        assert!((report.summary.orchestration_adoption_rate - 0.5).abs() < 0.01);
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn test_metrics_config_serde_roundtrip() {
        let config = MetricsConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: MetricsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.enabled, config.enabled);
        assert_eq!(back.retention_days, config.retention_days);
    }

    #[test]
    fn test_integration_metrics_serde_roundtrip() {
        let collector = create_metrics_collector();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let metrics = rt.block_on(collector.metrics());

        let json = serde_json::to_string(&metrics).unwrap();
        let back: IntegrationMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.routing_stats.total_tasks_routed,
            metrics.routing_stats.total_tasks_routed
        );
    }

    #[test]
    fn test_routing_stats_serde() {
        let stats = RoutingStats {
            total_tasks_routed: 100,
            orchestration_routed: 60,
            legacy_routed: 35,
            rejected_tasks: 5,
            routing_accuracy_estimate: 0.85,
            avg_classification_confidence: 0.78,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: RoutingStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_tasks_routed, 100);
        assert_eq!(back.orchestration_routed, 60);
    }

    #[test]
    fn test_alert_thresholds_serde() {
        let thresholds = AlertThresholds {
            max_error_rate: 0.15,
            min_success_rate: 0.9,
            max_avg_execution_time_seconds: 600.0,
            max_orchestration_slowdown_ratio: 1.5,
        };
        let json = serde_json::to_string(&thresholds).unwrap();
        let back: AlertThresholds = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_error_rate, 0.15);
    }

    #[test]
    fn test_metrics_alert_serde() {
        let alert = MetricsAlert::HighErrorRate {
            current_rate: 0.2,
            threshold: 0.1,
            affected_system: "test".to_string(),
        };
        let json = serde_json::to_string(&alert).unwrap();
        let back: MetricsAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            MetricsAlert::HighErrorRate {
                current_rate: 0.2,
                ..
            }
        ));
    }

    #[test]
    fn test_metrics_report_serde() {
        let report = MetricsReport {
            summary: MetricsSummary {
                total_tasks_processed: 50,
                orchestration_adoption_rate: 0.6,
                overall_success_rate: 0.9,
                avg_routing_confidence: 0.85,
            },
            routing_breakdown: RoutingStats {
                total_tasks_routed: 50,
                orchestration_routed: 30,
                legacy_routed: 18,
                rejected_tasks: 2,
                routing_accuracy_estimate: 0.85,
                avg_classification_confidence: 0.85,
            },
            performance_comparison: PerformanceComparison {
                orchestration_vs_legacy_time: 1.2,
                orchestration_vs_legacy_cost: 0.8,
                orchestration_vs_legacy_success: 0.05,
            },
            recommendations: vec!["Continue monitoring".to_string()],
            generated_at: Utc::now(),
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: MetricsReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.summary.total_tasks_processed, 50);
    }

    // ── Disabled metrics across all methods ─────────────

    #[tokio::test]
    async fn test_disabled_metrics_record_routing() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let collector = MetricsCollector::new(config);

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.routing_stats.total_tasks_routed, 0);
    }

    #[tokio::test]
    async fn test_disabled_metrics_record_execution() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let collector = MetricsCollector::new(config);

        collector
            .record_execution(&TaskExecutionResult {
                execution_path: ExecutionPath::Orchestration,
                result: ExecutionOutcome::Success(None),
                execution_metadata: None,
            })
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.error_tracking.total_errors, 0);
    }

    #[tokio::test]
    async fn test_disabled_metrics_record_shadow() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let collector = MetricsCollector::new(config);

        collector
            .record_shadow_comparison(&ShadowExecutionResult {
                comparison: ShadowComparison {
                    recommendation: ShadowRecommendation::PreferOrchestration,
                    orchestration_time_secs: None,
                    legacy_time_secs: None,
                    orchestration_success: true,
                    legacy_success: true,
                },
            })
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.total_comparisons, 0);
    }

    #[tokio::test]
    async fn test_disabled_metrics_check_alerts_returns_empty() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let collector = MetricsCollector::new(config);
        let alerts = collector.check_alerts().await;
        assert!(alerts.is_empty());
    }

    // ── Alert: performance degradation (low success rate) ─────────────

    #[tokio::test]
    async fn test_alerts_low_success_rate() {
        let config = MetricsConfig {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 1.0,
                min_success_rate: 0.5,
                max_avg_execution_time_seconds: f64::MAX,
                max_orchestration_slowdown_ratio: f64::MAX,
            },
        };
        let collector = MetricsCollector::new(config);

        // Route one task and complete it with success_rate starting at 0.0
        // The success_rate is initialized to 0.0 and won't be updated until
        // a successful execution is recorded via record_execution.
        // Since we never call record_execution for a success, the rate stays 0.0 < 0.5
        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let alerts = collector.check_alerts().await;
        assert!(alerts.iter().any(|a| matches!(a, MetricsAlert::PerformanceDegradation { metric_name, .. } if metric_name == "orchestration_success_rate")));
    }

    // ── Alert: performance degradation (slow execution time) ─────────────

    #[tokio::test]
    async fn test_alerts_slow_execution_time() {
        let config = MetricsConfig {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 1.0,
                min_success_rate: 0.0,
                max_avg_execution_time_seconds: 1.0,
                max_orchestration_slowdown_ratio: f64::MAX,
            },
        };
        let collector = MetricsCollector::new(config);

        // Record execution with high wall time
        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        collector
            .record_execution(&TaskExecutionResult {
                execution_path: ExecutionPath::Orchestration,
                result: ExecutionOutcome::Success(None),
                execution_metadata: Some(ExecutionMetadata {
                    wall_time_secs: 500.0,
                    tokens_used: None,
                    cost_usd: None,
                }),
            })
            .await;

        let alerts = collector.check_alerts().await;
        assert!(alerts.iter().any(|a| matches!(a, MetricsAlert::PerformanceDegradation { metric_name, .. } if metric_name == "avg_orchestration_execution_time")));
    }

    // ── Report with no tasks ─────────────

    #[tokio::test]
    async fn test_generate_report_empty() {
        let collector = create_metrics_collector();
        let report = collector.generate_report().await;

        assert_eq!(report.summary.total_tasks_processed, 0);
        assert!((report.summary.orchestration_adoption_rate - 0.0).abs() < 0.001);
        assert!((report.summary.overall_success_rate - 0.0).abs() < 0.001);
        // Default recommendation when no data
        assert!(!report.recommendations.is_empty());
    }

    // ── Recommendations: low routing accuracy ─────────────

    #[tokio::test]
    async fn test_recommendations_low_routing_accuracy() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Manually set low routing accuracy
        {
            let mut metrics = collector.metrics.lock().await;
            metrics.routing_stats.routing_accuracy_estimate = 0.5;
        }

        let report = collector.generate_report().await;
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.contains("classification")));
    }

    // ── Recommendations: legacy outperforms orchestration ─────────────

    #[tokio::test]
    async fn test_recommendations_legacy_better() {
        let collector = create_metrics_collector();

        // Record 1 orchestration-better and 5 legacy-better
        for _ in 0..5 {
            collector
                .record_shadow_comparison(&ShadowExecutionResult {
                    comparison: ShadowComparison {
                        recommendation: ShadowRecommendation::PreferLegacy,
                        orchestration_time_secs: Some(10.0),
                        legacy_time_secs: Some(2.0),
                        orchestration_success: true,
                        legacy_success: true,
                    },
                })
                .await;
        }
        collector
            .record_shadow_comparison(&ShadowExecutionResult {
                comparison: ShadowComparison {
                    recommendation: ShadowRecommendation::PreferOrchestration,
                    orchestration_time_secs: Some(2.0),
                    legacy_time_secs: Some(10.0),
                    orchestration_success: true,
                    legacy_success: true,
                },
            })
            .await;

        let report = collector.generate_report().await;
        assert!(report.recommendations.iter().any(|r| r.contains("Legacy")));
    }

    // ── Execution: failure with ExecutionPath::Unknown ─────────────

    #[tokio::test]
    async fn test_execution_failure_unknown_path() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Unknown,
            result: ExecutionOutcome::Failure("unknown error".to_string()),
            execution_metadata: None,
        };
        collector.record_execution(&result).await;

        let metrics = collector.metrics().await;
        // Unknown path failures still increment total_errors
        assert_eq!(metrics.error_tracking.total_errors, 1);
        // But not orchestration or legacy specific counters
        assert_eq!(metrics.error_tracking.orchestration_errors, 0);
        assert_eq!(metrics.error_tracking.legacy_errors, 0);
    }

    // ── Execution: success with ExecutionPath::Unknown ─────────────

    #[tokio::test]
    async fn test_execution_success_unknown_path() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Unknown,
            result: ExecutionOutcome::Success(Some("done".to_string())),
            execution_metadata: None,
        };
        collector.record_execution(&result).await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.error_tracking.total_errors, 0);
    }

    // ── Serde roundtrips for remaining types ─────────────

    #[test]
    fn test_shadow_comparison_stats_serde() {
        let stats = ShadowComparisonStats {
            total_comparisons: 100,
            orchestration_better: 60,
            legacy_better: 30,
            equivalent_performance: 8,
            both_failed: 2,
            avg_time_improvement_percent: 15.0,
            avg_cost_improvement_percent: 10.0,
            avg_success_rate_improvement: 0.05,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: ShadowComparisonStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_comparisons, 100);
        assert_eq!(back.orchestration_better, 60);
    }

    #[test]
    fn test_performance_metrics_serde() {
        let metrics = PerformanceMetrics {
            avg_orchestration_execution_time: 10.0,
            avg_legacy_execution_time: 5.0,
            avg_orchestration_cost: 0.01,
            avg_legacy_cost: 0.005,
            orchestration_success_rate: 0.9,
            legacy_success_rate: 0.85,
            peak_concurrent_executions: 10,
            resource_utilization_trends: vec![ResourceTrend {
                timestamp: Utc::now(),
                cpu_utilization_percent: 45.0,
                memory_utilization_percent: 60.0,
                active_orchestration_tasks: 3,
                active_legacy_tasks: 2,
            }],
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let back: PerformanceMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(back.peak_concurrent_executions, 10);
        assert_eq!(back.resource_utilization_trends.len(), 1);
    }

    #[test]
    fn test_error_tracking_serde() {
        let tracking = ErrorTracking {
            total_errors: 5,
            orchestration_errors: 3,
            legacy_errors: 2,
            routing_errors: 0,
            top_error_categories: vec![ErrorCategoryCount {
                category: "timeout".to_string(),
                count: 3,
                percentage: 60.0,
            }],
            error_rate_trend: vec![ErrorRatePoint {
                timestamp: Utc::now(),
                error_rate: 0.05,
                orchestration_error_rate: 0.03,
                legacy_error_rate: 0.02,
            }],
        };
        let json = serde_json::to_string(&tracking).unwrap();
        let back: ErrorTracking = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_errors, 5);
        assert_eq!(back.top_error_categories.len(), 1);
    }

    #[test]
    fn test_metrics_metadata_serde() {
        let meta = MetricsMetadata {
            collection_start_time: Utc::now(),
            last_updated: Utc::now(),
            version: "1.0.0".to_string(),
            active_alerts: vec!["high error rate".to_string()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: MetricsMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, "1.0.0");
        assert_eq!(back.active_alerts.len(), 1);
    }

    #[test]
    fn test_metrics_summary_serde() {
        let summary = MetricsSummary {
            total_tasks_processed: 100,
            orchestration_adoption_rate: 0.65,
            overall_success_rate: 0.92,
            avg_routing_confidence: 0.88,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: MetricsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_tasks_processed, 100);
        assert!((back.orchestration_adoption_rate - 0.65).abs() < 0.001);
    }

    #[test]
    fn test_performance_comparison_serde() {
        let comp = PerformanceComparison {
            orchestration_vs_legacy_time: 0.8,
            orchestration_vs_legacy_cost: 1.2,
            orchestration_vs_legacy_success: 0.05,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: PerformanceComparison = serde_json::from_str(&json).unwrap();
        assert!((back.orchestration_vs_legacy_time - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_resource_trend_serde() {
        let trend = ResourceTrend {
            timestamp: Utc::now(),
            cpu_utilization_percent: 55.0,
            memory_utilization_percent: 70.0,
            active_orchestration_tasks: 4,
            active_legacy_tasks: 1,
        };
        let json = serde_json::to_string(&trend).unwrap();
        let back: ResourceTrend = serde_json::from_str(&json).unwrap();
        assert!((back.cpu_utilization_percent - 55.0).abs() < 0.001);
    }

    #[test]
    fn test_all_metrics_alert_serde_variants() {
        // HighErrorRate
        let alert = MetricsAlert::HighErrorRate {
            current_rate: 0.2,
            threshold: 0.1,
            affected_system: "test".to_string(),
        };
        let json = serde_json::to_string(&alert).unwrap();
        let back: MetricsAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, MetricsAlert::HighErrorRate { .. }));

        // PerformanceDegradation
        let alert = MetricsAlert::PerformanceDegradation {
            metric_name: "latency".to_string(),
            degradation_percent: 25.0,
            baseline_value: 100.0,
            current_value: 125.0,
        };
        let json = serde_json::to_string(&alert).unwrap();
        let back: MetricsAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, MetricsAlert::PerformanceDegradation { .. }));

        // RoutingInaccuracy
        let alert = MetricsAlert::RoutingInaccuracy {
            accuracy_drop: 0.1,
            expected_accuracy: 0.9,
            current_accuracy: 0.8,
        };
        let json = serde_json::to_string(&alert).unwrap();
        let back: MetricsAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, MetricsAlert::RoutingInaccuracy { .. }));

        // ResourceExhaustion
        let alert = MetricsAlert::ResourceExhaustion {
            resource_type: "memory".to_string(),
            utilization_percent: 95.0,
            threshold_percent: 80.0,
        };
        let json = serde_json::to_string(&alert).unwrap();
        let back: MetricsAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, MetricsAlert::ResourceExhaustion { .. }));
    }

    // ── Report: performance comparison with zero legacy ─────────────

    #[tokio::test]
    async fn test_report_performance_comparison_no_legacy() {
        let collector = create_metrics_collector();

        // Only orchestration tasks, no legacy
        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        let report = collector.generate_report().await;
        // With avg_legacy_execution_time == 0.0, ratio should default to 1.0
        assert!((report.performance_comparison.orchestration_vs_legacy_time - 1.0).abs() < 0.001);
        assert!((report.performance_comparison.orchestration_vs_legacy_cost - 1.0).abs() < 0.001);
    }

    // ── Recommendations: high orchestration errors ─────────────

    #[tokio::test]
    async fn test_recommendations_high_orchestration_errors() {
        let collector = create_metrics_collector();

        // Record 3 orchestration successes and 10 failures
        for _ in 0..3 {
            collector
                .record_routing(
                    &RoutingDecision::Orchestration {
                        starting_tier: 1,
                        reasoning: String::new(),
                    },
                    0.9,
                )
                .await;
            collector
                .record_execution(&TaskExecutionResult {
                    execution_path: ExecutionPath::Orchestration,
                    result: ExecutionOutcome::Success(None),
                    execution_metadata: None,
                })
                .await;
        }
        for _ in 0..10 {
            collector
                .record_routing(
                    &RoutingDecision::Orchestration {
                        starting_tier: 1,
                        reasoning: String::new(),
                    },
                    0.9,
                )
                .await;
            collector
                .record_execution(&TaskExecutionResult {
                    execution_path: ExecutionPath::Orchestration,
                    result: ExecutionOutcome::Failure("err".to_string()),
                    execution_metadata: None,
                })
                .await;
        }
        // 1 legacy error (so orchestration errors > 2x legacy)
        collector
            .record_routing(
                &RoutingDecision::Legacy {
                    reasoning: String::new(),
                },
                0.8,
            )
            .await;
        collector
            .record_execution(&TaskExecutionResult {
                execution_path: ExecutionPath::Legacy,
                result: ExecutionOutcome::Failure("err".to_string()),
                execution_metadata: None,
            })
            .await;

        // Manually bump orchestration errors in the tracking to exceed 2x legacy
        {
            let mut m = collector.metrics.lock().await;
            m.error_tracking.orchestration_errors = 20;
            m.error_tracking.legacy_errors = 5;
        }

        let report = collector.generate_report().await;
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.contains("error rate")));
    }

    // ── Recommendations: expensive orchestration ─────────────

    #[tokio::test]
    async fn test_recommendations_expensive_orchestration() {
        let collector = create_metrics_collector();

        // Set up cost difference > 2x
        {
            let mut m = collector.metrics.lock().await;
            m.performance_metrics.avg_orchestration_cost = 10.0;
            m.performance_metrics.avg_legacy_cost = 2.0;
        }

        let report = collector.generate_report().await;
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.contains("expensive")));
    }

    // ── Shadow comparison: Pending recommendation ─────────────

    #[tokio::test]
    async fn test_shadow_comparison_pending_does_not_increment() {
        let collector = create_metrics_collector();

        collector
            .record_shadow_comparison(&ShadowExecutionResult {
                comparison: ShadowComparison {
                    recommendation: ShadowRecommendation::Pending,
                    orchestration_time_secs: None,
                    legacy_time_secs: None,
                    orchestration_success: false,
                    legacy_success: false,
                },
            })
            .await;

        let metrics = collector.metrics().await;
        assert_eq!(metrics.shadow_stats.total_comparisons, 1);
        // Pending does not increment any specific counter
        assert_eq!(metrics.shadow_stats.orchestration_better, 0);
        assert_eq!(metrics.shadow_stats.legacy_better, 0);
        assert_eq!(metrics.shadow_stats.equivalent_performance, 0);
        assert_eq!(metrics.shadow_stats.both_failed, 0);
    }

    // ── Alert: check_alerts with few tasks (below threshold) ─────────────

    #[tokio::test]
    async fn test_alerts_below_task_threshold() {
        let config = MetricsConfig {
            enabled: true,
            retention_days: 30,
            collect_detailed_metrics: true,
            alert_thresholds: AlertThresholds {
                max_error_rate: 0.0, // even 0% threshold
                min_success_rate: 0.0,
                max_avg_execution_time_seconds: f64::MAX,
                max_orchestration_slowdown_ratio: f64::MAX,
            },
        };
        let collector = MetricsCollector::new(config);

        // Only 5 tasks - below the 10-task threshold for error rate checking
        for _ in 0..5 {
            collector
                .record_routing(
                    &RoutingDecision::Orchestration {
                        starting_tier: 1,
                        reasoning: String::new(),
                    },
                    0.9,
                )
                .await;
            collector
                .record_execution(&TaskExecutionResult {
                    execution_path: ExecutionPath::Orchestration,
                    result: ExecutionOutcome::Failure("err".to_string()),
                    execution_metadata: None,
                })
                .await;
        }

        let alerts = collector.check_alerts().await;
        // Error rate alert should NOT fire because < 10 tasks
        assert!(!alerts
            .iter()
            .any(|a| matches!(a, MetricsAlert::HighErrorRate { .. })));
    }

    // ── Execution with cost_usd metadata ─────────────

    #[tokio::test]
    async fn test_execution_updates_cost() {
        let collector = create_metrics_collector();

        collector
            .record_routing(
                &RoutingDecision::Orchestration {
                    starting_tier: 1,
                    reasoning: String::new(),
                },
                0.9,
            )
            .await;

        collector
            .record_execution(&TaskExecutionResult {
                execution_path: ExecutionPath::Orchestration,
                result: ExecutionOutcome::Success(None),
                execution_metadata: Some(ExecutionMetadata {
                    wall_time_secs: 10.0,
                    tokens_used: Some(500),
                    cost_usd: Some(0.05),
                }),
            })
            .await;

        let metrics = collector.metrics().await;
        assert!((metrics.performance_metrics.avg_orchestration_cost - 0.05).abs() < 0.001);
    }
}
