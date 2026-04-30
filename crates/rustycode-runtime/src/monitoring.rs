//! Monitoring and Metrics Collection System
//!
//! This module provides comprehensive monitoring with:
//! - Metrics collection and aggregation
//! - Performance monitoring
//! - Alert generation and notification
//! - Historical data storage
//! - Dashboard data aggregation
//! - Custom metric definitions
//! - Real-time monitoring streams

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Metric type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MetricType {
    Counter,   // Monotonically increasing value
    Gauge,     // Point-in-time value
    Histogram, // Distribution of values
    Summary,   // Statistical summary
}

/// Metric data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub metadata: MetricMetadata,
}

/// Metric metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricMetadata {
    pub metric_type: MetricType,
    pub description: String,
    pub unit: String,
    pub source: String,
    pub agent_role: Option<AgentRole>,
}

impl Default for MetricMetadata {
    fn default() -> Self {
        Self {
            metric_type: MetricType::Gauge,
            description: String::new(),
            unit: String::new(),
            source: String::new(),
            agent_role: None,
        }
    }
}

/// Metric definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub name: String,
    pub metadata: MetricMetadata,
    pub labels: Vec<String>,
    pub aggregation: MetricAggregation,
}

/// Metric aggregation strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum MetricAggregation {
    Sum,
    Average,
    Min,
    Max,
    Count,
    Percentile(f64), // e.g., 0.50, 0.95, 0.99
}

/// Time series data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    pub metric_name: String,
    pub labels: HashMap<String, String>,
    pub data_points: VecDeque<MetricDataPoint>,
    pub max_data_points: usize,
}

impl TimeSeries {
    pub fn new(
        metric_name: String,
        labels: HashMap<String, String>,
        max_data_points: usize,
    ) -> Self {
        Self {
            metric_name,
            labels,
            data_points: VecDeque::with_capacity(max_data_points),
            max_data_points,
        }
    }

    pub fn add_point(&mut self, point: MetricDataPoint) {
        if self.data_points.len() >= self.max_data_points {
            self.data_points.pop_front();
        }
        self.data_points.push_back(point);
    }

    pub fn get_latest(&self) -> Option<&MetricDataPoint> {
        self.data_points.back()
    }

    pub fn query_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<&MetricDataPoint> {
        self.data_points
            .iter()
            .filter(|p| p.timestamp >= start && p.timestamp <= end)
            .collect()
    }
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub memory_usage_percent: f64,
    pub disk_io_read_mb: f64,
    pub disk_io_write_mb: f64,
    pub network_rx_mb: f64,
    pub network_tx_mb: f64,
    pub open_file_descriptors: u64,
    pub thread_count: u64,
    pub timestamp: DateTime<Utc>,
}

/// Alert severity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Alert status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
    Suppressed,
}

/// Alert condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    pub metric_name: String,
    pub operator: AlertOperator,
    pub threshold: f64,
    pub duration_seconds: u64,
}

/// Alert operator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlertOperator {
    GreaterThan,
    LessThan,
    Equals,
    NotEquals,
    GreaterOrEqual,
    LessOrEqual,
}

/// Alert definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub severity: AlertSeverity,
    pub condition: AlertCondition,
    pub labels: HashMap<String, String>,
    pub enabled: bool,
}

/// Alert event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub id: String,
    pub alert_id: String,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub message: String,
    pub triggered_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metric_value: f64,
    pub labels: HashMap<String, String>,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub max_data_points_per_series: usize,
    pub max_time_series: usize,
    pub data_retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub enable_alerts: bool,
    pub alert_evaluation_interval_seconds: u64,
    pub enable_historical_data: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            max_data_points_per_series: 1000,
            max_time_series: 10000,
            data_retention_hours: 24,
            collection_interval_seconds: 15,
            enable_alerts: true,
            alert_evaluation_interval_seconds: 30,
            enable_historical_data: true,
        }
    }
}

/// Monitoring system
pub struct MonitoringSystem {
    metrics: Arc<RwLock<HashMap<String, TimeSeries>>>,
    metric_definitions: Arc<RwLock<HashMap<String, MetricDefinition>>>,
    alerts: Arc<RwLock<HashMap<String, AlertDefinition>>>,
    active_alerts: Arc<RwLock<HashMap<String, AlertEvent>>>,
    alert_history: Arc<RwLock<Vec<AlertEvent>>>,
    performance_metrics: Arc<RwLock<VecDeque<PerformanceMetrics>>>,
    config: MonitoringConfig,
    alert_counter: Arc<RwLock<u64>>,
}

impl MonitoringSystem {
    pub fn new(config: MonitoringConfig) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            metric_definitions: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(HashMap::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            performance_metrics: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            config,
            alert_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Register a metric definition
    pub async fn register_metric(&self, definition: MetricDefinition) -> Result<(), String> {
        let mut definitions = self.metric_definitions.write().await;

        if definitions.contains_key(&definition.name) {
            return Err(format!("Metric {} already registered", definition.name));
        }

        definitions.insert(definition.name.clone(), definition);
        Ok(())
    }

    /// Record a metric value
    pub async fn record_metric(
        &self,
        metric_name: &str,
        value: f64,
        labels: HashMap<String, String>,
    ) -> Result<(), String> {
        // Get metric definition
        let definitions = self.metric_definitions.read().await;
        let definition = definitions
            .get(metric_name)
            .ok_or_else(|| format!("Metric {} not registered", metric_name))?;

        // Create data point
        let data_point = MetricDataPoint {
            timestamp: Utc::now(),
            value,
            labels: labels.clone(),
            metadata: definition.metadata.clone(),
        };

        // Get or create time series
        let series_key = self.series_key(metric_name, &labels);
        let mut metrics = self.metrics.write().await;

        let series = metrics.entry(series_key).or_insert_with(|| {
            TimeSeries::new(
                metric_name.to_string(),
                labels,
                self.config.max_data_points_per_series,
            )
        });

        series.add_point(data_point);

        // Check max time series limit
        if metrics.len() > self.config.max_time_series {
            // Remove oldest series
            if let Some(oldest_key) = metrics.keys().next().cloned() {
                metrics.remove(&oldest_key);
            }
        }

        Ok(())
    }

    /// Get metric value
    pub async fn get_metric(
        &self,
        metric_name: &str,
        labels: &HashMap<String, String>,
    ) -> Option<f64> {
        let series_key = self.series_key(metric_name, labels);
        let metrics = self.metrics.read().await;

        metrics.get(&series_key)?.get_latest().map(|p| p.value)
    }

    /// Query metric range
    pub async fn query_metric_range(
        &self,
        metric_name: &str,
        labels: &HashMap<String, String>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<MetricDataPoint> {
        let series_key = self.series_key(metric_name, labels);
        let metrics = self.metrics.read().await;

        metrics
            .get(&series_key)
            .map(|series| {
                series
                    .query_range(start, end)
                    .into_iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Aggregate metric
    pub async fn aggregate_metric(
        &self,
        metric_name: &str,
        labels: Option<&HashMap<String, String>>,
        aggregation: MetricAggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<f64, String> {
        let metrics = self.metrics.read().await;

        // Find matching time series
        let matching_series: Vec<_> = metrics
            .iter()
            .filter(|(_key, series)| {
                series.metric_name == metric_name && labels.is_none_or(|l| &series.labels == l)
            })
            .collect();

        if matching_series.is_empty() {
            return Err(format!("No data found for metric {}", metric_name));
        }

        // Collect all data points
        let mut values: Vec<f64> = Vec::new();
        for (_key, series) in matching_series {
            for point in series.query_range(start, end) {
                values.push(point.value);
            }
        }

        if values.is_empty() {
            return Err("No data points in time range".to_string());
        }

        // Apply aggregation
        match aggregation {
            MetricAggregation::Sum => Ok(values.iter().sum()),
            MetricAggregation::Average => {
                if values.is_empty() {
                    Ok(0.0)
                } else {
                    Ok(values.iter().sum::<f64>() / values.len() as f64)
                }
            }
            MetricAggregation::Min => Ok(values.iter().fold(f64::INFINITY, |a, &b| a.min(b))),
            MetricAggregation::Max => Ok(values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b))),
            MetricAggregation::Count => Ok(values.len() as f64),
            MetricAggregation::Percentile(p) => {
                if values.is_empty() {
                    return Ok(0.0);
                }
                let mut sorted = values;
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let max_idx = sorted.len() - 1;
                let index = (p * max_idx as f64).round() as usize;
                Ok(sorted[index.min(max_idx)])
            }
        }
    }

    /// Record performance metrics
    pub async fn record_performance(&self, metrics: PerformanceMetrics) {
        let mut perf_metrics = self.performance_metrics.write().await;

        if perf_metrics.len() >= 1000 {
            perf_metrics.pop_front();
        }

        perf_metrics.push_back(metrics);
    }

    /// Get current performance metrics
    pub async fn get_performance_metrics(&self) -> Option<PerformanceMetrics> {
        let perf_metrics = self.performance_metrics.read().await;
        perf_metrics.back().cloned()
    }

    /// Get average performance metrics
    pub async fn get_average_performance(
        &self,
        duration_minutes: u64,
    ) -> Option<PerformanceMetrics> {
        let perf_metrics = self.performance_metrics.read().await;
        let cutoff = Utc::now() - Duration::minutes(duration_minutes as i64);

        let relevant: Vec<_> = perf_metrics
            .iter()
            .filter(|m| m.timestamp > cutoff)
            .collect();

        if relevant.is_empty() {
            return None;
        }

        let count = relevant.len() as f64;

        Some(PerformanceMetrics {
            cpu_usage_percent: relevant.iter().map(|m| m.cpu_usage_percent).sum::<f64>() / count,
            memory_usage_mb: relevant.iter().map(|m| m.memory_usage_mb).sum::<f64>() / count,
            memory_usage_percent: relevant.iter().map(|m| m.memory_usage_percent).sum::<f64>()
                / count,
            disk_io_read_mb: relevant.iter().map(|m| m.disk_io_read_mb).sum::<f64>() / count,
            disk_io_write_mb: relevant.iter().map(|m| m.disk_io_write_mb).sum::<f64>() / count,
            network_rx_mb: relevant.iter().map(|m| m.network_rx_mb).sum::<f64>() / count,
            network_tx_mb: relevant.iter().map(|m| m.network_tx_mb).sum::<f64>() / count,
            open_file_descriptors: (relevant
                .iter()
                .map(|m| m.open_file_descriptors)
                .sum::<u64>() as f64
                / count) as u64,
            thread_count: (relevant.iter().map(|m| m.thread_count).sum::<u64>() as f64 / count)
                as u64,
            timestamp: Utc::now(),
        })
    }

    /// Create alert definition
    pub async fn create_alert(&self, alert: AlertDefinition) -> Result<(), String> {
        let mut alerts = self.alerts.write().await;

        alerts.insert(alert.id.clone(), alert);
        Ok(())
    }

    /// Evaluate alerts
    pub async fn evaluate_alerts(&self) -> Result<Vec<AlertEvent>, String> {
        if !self.config.enable_alerts {
            return Ok(Vec::new());
        }

        let mut new_alerts = Vec::new();
        let alerts = self.alerts.read().await;
        let metrics = self.metrics.read().await;

        for (_id, alert_def) in alerts.iter() {
            if !alert_def.enabled {
                continue;
            }

            // Find matching time series
            for (series_key, series) in metrics.iter() {
                if series.metric_name == alert_def.condition.metric_name {
                    if let Some(latest) = series.get_latest() {
                        let triggered = match alert_def.condition.operator {
                            AlertOperator::GreaterThan => {
                                latest.value > alert_def.condition.threshold
                            }
                            AlertOperator::LessThan => latest.value < alert_def.condition.threshold,
                            AlertOperator::Equals => {
                                (latest.value - alert_def.condition.threshold).abs() < f64::EPSILON
                            }
                            AlertOperator::NotEquals => {
                                (latest.value - alert_def.condition.threshold).abs() >= f64::EPSILON
                            }
                            AlertOperator::GreaterOrEqual => {
                                latest.value >= alert_def.condition.threshold
                            }
                            AlertOperator::LessOrEqual => {
                                latest.value <= alert_def.condition.threshold
                            }
                        };

                        if triggered {
                            // Check if alert already exists
                            let mut active_alerts = self.active_alerts.write().await;
                            let alert_key = format!("{}_{}", alert_def.id, series_key);

                            if let std::collections::hash_map::Entry::Vacant(e) =
                                active_alerts.entry(alert_key.clone())
                            {
                                let mut counter = self.alert_counter.write().await;
                                *counter += 1;

                                let event = AlertEvent {
                                    id: format!("alert_{}", *counter),
                                    alert_id: alert_def.id.clone(),
                                    severity: alert_def.severity,
                                    status: AlertStatus::Active,
                                    message: format!(
                                        "{}: {} {} threshold {}",
                                        alert_def.name,
                                        latest.value,
                                        Self::operator_to_string(alert_def.condition.operator),
                                        alert_def.condition.threshold
                                    ),
                                    triggered_at: Utc::now(),
                                    acknowledged_at: None,
                                    resolved_at: None,
                                    metric_value: latest.value,
                                    labels: alert_def.labels.clone(),
                                };

                                new_alerts.push(event.clone());
                                e.insert(event.clone());

                                // Add to history
                                let mut history = self.alert_history.write().await;
                                history.push(event);
                            }
                        }
                    }
                }
            }
        }

        Ok(new_alerts)
    }

    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<AlertEvent> {
        let active_alerts = self.active_alerts.read().await;
        active_alerts.values().cloned().collect()
    }

    /// Acknowledge alert
    pub async fn acknowledge_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_alerts = self.active_alerts.write().await;

        for alert in active_alerts.values_mut() {
            if alert.id == alert_id {
                alert.status = AlertStatus::Acknowledged;
                alert.acknowledged_at = Some(Utc::now());
                return Ok(());
            }
        }

        Err(format!("Alert {} not found", alert_id))
    }

    /// Resolve alert
    pub async fn resolve_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_alerts = self.active_alerts.write().await;

        // Find the alert by event.id
        let alert_key = active_alerts
            .iter()
            .find(|(_, alert)| alert.id == alert_id)
            .map(|(key, _)| key.clone());

        if let Some(key) = alert_key {
            // Remove the alert
            let _alert = active_alerts.remove(&key);

            // Update in history
            let mut history = self.alert_history.write().await;
            for event in history.iter_mut() {
                if event.id == alert_id {
                    event.status = AlertStatus::Resolved;
                    event.resolved_at = Some(Utc::now());
                }
            }

            Ok(())
        } else {
            Err(format!("Alert {} not found", alert_id))
        }
    }

    /// Get alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<AlertEvent> {
        let history = self.alert_history.read().await;

        match limit {
            Some(limit) => history.iter().rev().take(limit).cloned().collect(),
            None => history.iter().rev().cloned().collect(),
        }
    }

    /// Get all metrics
    pub async fn get_all_metrics(&self) -> HashMap<String, Vec<String>> {
        let metrics = self.metrics.read().await;

        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        for (key, series) in metrics.iter() {
            result
                .entry(series.metric_name.clone())
                .or_default()
                .push(key.clone());
        }

        result
    }

    /// Get metric definitions
    pub async fn get_metric_definitions(&self) -> Vec<MetricDefinition> {
        let definitions = self.metric_definitions.read().await;
        definitions.values().cloned().collect()
    }

    /// Get alert definitions
    pub async fn get_alert_definitions(&self) -> Vec<AlertDefinition> {
        let alerts = self.alerts.read().await;
        alerts.values().cloned().collect()
    }

    /// Cleanup old data
    pub async fn cleanup_old_data(&self) -> Result<usize, String> {
        let cutoff = Utc::now() - Duration::hours(self.config.data_retention_hours as i64);
        let mut cleaned = 0;

        let mut metrics = self.metrics.write().await;

        for series in metrics.values_mut() {
            let original_len = series.data_points.len();
            series.data_points.retain(|p| p.timestamp > cutoff);
            cleaned += original_len - series.data_points.len();
        }

        Ok(cleaned)
    }

    /// Get system statistics
    pub async fn get_statistics(&self) -> MonitoringStatistics {
        let metrics = self.metrics.read().await;
        let definitions = self.metric_definitions.read().await;
        let alerts = self.alerts.read().await;
        let active_alerts = self.active_alerts.read().await;
        let history = self.alert_history.read().await;
        let perf_metrics = self.performance_metrics.read().await;

        MonitoringStatistics {
            total_time_series: metrics.len(),
            total_data_points: metrics.values().map(|s| s.data_points.len()).sum(),
            registered_metrics: definitions.len(),
            registered_alerts: alerts.len(),
            active_alerts: active_alerts.len(),
            resolved_alerts: history
                .iter()
                .filter(|a| a.status == AlertStatus::Resolved)
                .count(),
            performance_samples: perf_metrics.len(),
            current_alert_severity: active_alerts
                .values()
                .map(|a| a.severity)
                .max()
                .unwrap_or(AlertSeverity::Info),
        }
    }

    /// Generate series key
    fn series_key(&self, metric_name: &str, labels: &HashMap<String, String>) -> String {
        let mut key_parts = vec![metric_name.to_string()];

        let mut sorted_labels: Vec<_> = labels.iter().collect();
        sorted_labels.sort_by_key(|&(k, _)| k);

        for (k, v) in sorted_labels {
            key_parts.push(format!("{}={}", k, v));
        }

        key_parts.join(",")
    }

    /// Convert operator to string
    fn operator_to_string(op: AlertOperator) -> &'static str {
        match op {
            AlertOperator::GreaterThan => ">",
            AlertOperator::LessThan => "<",
            AlertOperator::Equals => "==",
            AlertOperator::NotEquals => "!=",
            AlertOperator::GreaterOrEqual => ">=",
            AlertOperator::LessOrEqual => "<=",
        }
    }
}

/// Monitoring statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringStatistics {
    pub total_time_series: usize,
    pub total_data_points: usize,
    pub registered_metrics: usize,
    pub registered_alerts: usize,
    pub active_alerts: usize,
    pub resolved_alerts: usize,
    pub performance_samples: usize,
    pub current_alert_severity: AlertSeverity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_type_equality() {
        assert_eq!(MetricType::Counter, MetricType::Counter);
        assert_ne!(MetricType::Counter, MetricType::Gauge);
    }

    #[tokio::test]
    async fn test_register_metric() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata {
                metric_type: MetricType::Gauge,
                description: "Test metric".to_string(),
                unit: "count".to_string(),
                source: "test".to_string(),
                agent_role: None,
            },
            labels: vec!["label1".to_string()],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let definitions = monitoring.get_metric_definitions().await;
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, "test_metric");
    }

    #[tokio::test]
    async fn test_record_metric() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let mut labels = HashMap::new();
        labels.insert("host".to_string(), "localhost".to_string());

        monitoring
            .record_metric("test_metric", 42.0, labels.clone())
            .await
            .unwrap();

        let value = monitoring.get_metric("test_metric", &labels).await;
        assert_eq!(value, Some(42.0));
    }

    #[tokio::test]
    async fn test_aggregate_metric() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let mut labels = HashMap::new();
        labels.insert("host".to_string(), "localhost".to_string());

        monitoring
            .record_metric("test_metric", 10.0, labels.clone())
            .await
            .unwrap();
        monitoring
            .record_metric("test_metric", 20.0, labels.clone())
            .await
            .unwrap();
        monitoring
            .record_metric("test_metric", 30.0, labels.clone())
            .await
            .unwrap();

        let start = Utc::now() - Duration::minutes(1);
        let end = Utc::now() + Duration::minutes(1);

        let sum = monitoring
            .aggregate_metric(
                "test_metric",
                Some(&labels),
                MetricAggregation::Sum,
                start,
                end,
            )
            .await
            .unwrap();

        assert_eq!(sum, 60.0);

        let avg = monitoring
            .aggregate_metric(
                "test_metric",
                Some(&labels),
                MetricAggregation::Average,
                start,
                end,
            )
            .await
            .unwrap();

        assert_eq!(avg, 20.0);
    }

    #[tokio::test]
    async fn test_create_and_evaluate_alerts() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "cpu_usage".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let alert = AlertDefinition {
            id: "high_cpu".to_string(),
            name: "High CPU Usage".to_string(),
            description: "CPU usage is too high".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition {
                metric_name: "cpu_usage".to_string(),
                operator: AlertOperator::GreaterThan,
                threshold: 80.0,
                duration_seconds: 60,
            },
            labels: HashMap::new(),
            enabled: true,
        };

        monitoring.create_alert(alert).await.unwrap();

        let mut labels = HashMap::new();
        labels.insert("host".to_string(), "localhost".to_string());

        monitoring
            .record_metric("cpu_usage", 90.0, labels.clone())
            .await
            .unwrap();

        let triggered_alerts = monitoring.evaluate_alerts().await.unwrap();
        assert_eq!(triggered_alerts.len(), 1);
        assert_eq!(triggered_alerts[0].severity, AlertSeverity::Warning);
    }

    #[tokio::test]
    async fn test_alert_acknowledge_and_resolve() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let alert = AlertDefinition {
            id: "test_alert".to_string(),
            name: "Test Alert".to_string(),
            description: "Test alert".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition {
                metric_name: "test_metric".to_string(),
                operator: AlertOperator::GreaterThan,
                threshold: 50.0,
                duration_seconds: 60,
            },
            labels: HashMap::new(),
            enabled: true,
        };

        monitoring.create_alert(alert).await.unwrap();

        let mut labels = HashMap::new();
        labels.insert("host".to_string(), "localhost".to_string());

        monitoring
            .record_metric("test_metric", 100.0, labels)
            .await
            .unwrap();

        let triggered_alerts = monitoring.evaluate_alerts().await.unwrap();
        assert_eq!(triggered_alerts.len(), 1);

        let alert_id = &triggered_alerts[0].id;
        monitoring.acknowledge_alert(alert_id).await.unwrap();

        let active_alerts = monitoring.get_active_alerts().await;
        assert_eq!(active_alerts[0].status, AlertStatus::Acknowledged);

        monitoring.resolve_alert(alert_id).await.unwrap();

        let active_alerts = monitoring.get_active_alerts().await;
        assert_eq!(active_alerts.len(), 0);
    }

    #[tokio::test]
    async fn test_performance_metrics() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let metrics = PerformanceMetrics {
            cpu_usage_percent: 50.0,
            memory_usage_mb: 1024.0,
            memory_usage_percent: 25.0,
            disk_io_read_mb: 100.0,
            disk_io_write_mb: 50.0,
            network_rx_mb: 200.0,
            network_tx_mb: 150.0,
            open_file_descriptors: 100,
            thread_count: 10,
            timestamp: Utc::now(),
        };

        monitoring.record_performance(metrics.clone()).await;

        let retrieved = monitoring.get_performance_metrics().await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().cpu_usage_percent, 50.0);
    }

    #[tokio::test]
    async fn test_cleanup_old_data() {
        let config = MonitoringConfig {
            data_retention_hours: 1,
            ..Default::default()
        };

        let monitoring = MonitoringSystem::new(config);

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let mut labels = HashMap::new();
        labels.insert("host".to_string(), "localhost".to_string());

        monitoring
            .record_metric("test_metric", 42.0, labels.clone())
            .await
            .unwrap();

        // Cleanup won't remove recent data
        let cleaned = monitoring.cleanup_old_data().await.unwrap();
        assert_eq!(cleaned, 0);

        let value = monitoring.get_metric("test_metric", &labels).await;
        assert_eq!(value, Some(42.0));
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let stats = monitoring.get_statistics().await;
        assert_eq!(stats.registered_metrics, 1);
        assert_eq!(stats.total_time_series, 0);
        assert_eq!(stats.active_alerts, 0);
    }

    // --- New tests below ---

    #[test]
    fn test_metric_metadata_default() {
        let meta = MetricMetadata::default();
        assert_eq!(meta.metric_type, MetricType::Gauge);
        assert!(meta.description.is_empty());
        assert!(meta.unit.is_empty());
        assert!(meta.source.is_empty());
        assert!(meta.agent_role.is_none());
    }

    #[test]
    fn test_monitoring_config_default() {
        let config = MonitoringConfig::default();
        assert_eq!(config.max_data_points_per_series, 1000);
        assert_eq!(config.max_time_series, 10000);
        assert_eq!(config.data_retention_hours, 24);
        assert_eq!(config.collection_interval_seconds, 15);
        assert!(config.enable_alerts);
        assert_eq!(config.alert_evaluation_interval_seconds, 30);
        assert!(config.enable_historical_data);
    }

    #[test]
    fn test_time_series_add_point_overflow() {
        let mut series = TimeSeries::new(
            "test".to_string(),
            HashMap::new(),
            3, // max 3 data points
        );

        for i in 0..5 {
            series.add_point(MetricDataPoint {
                timestamp: Utc::now(),
                value: i as f64,
                labels: HashMap::new(),
                metadata: MetricMetadata::default(),
            });
        }

        assert_eq!(series.data_points.len(), 3);
        // Should retain the latest 3 points (values 2, 3, 4)
        assert_eq!(series.data_points[0].value, 2.0);
        assert_eq!(series.data_points[2].value, 4.0);
    }

    #[test]
    fn test_time_series_get_latest() {
        let mut series = TimeSeries::new("test".to_string(), HashMap::new(), 10);

        assert!(series.get_latest().is_none());

        series.add_point(MetricDataPoint {
            timestamp: Utc::now(),
            value: 10.0,
            labels: HashMap::new(),
            metadata: MetricMetadata::default(),
        });

        let latest = series.get_latest().unwrap();
        assert_eq!(latest.value, 10.0);
    }

    #[test]
    fn test_time_series_query_range() {
        let mut series = TimeSeries::new("test".to_string(), HashMap::new(), 100);

        let base_time = Utc::now();
        for i in 0..5 {
            series.add_point(MetricDataPoint {
                timestamp: base_time + Duration::seconds(i),
                value: i as f64,
                labels: HashMap::new(),
                metadata: MetricMetadata::default(),
            });
        }

        // Query for middle range (1s to 3s from base)
        let results = series.query_range(
            base_time + Duration::seconds(1),
            base_time + Duration::seconds(3),
        );
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_duplicate_metric_registration() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "duplicate_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring
            .register_metric(definition.clone())
            .await
            .unwrap();
        let result = monitoring.register_metric(definition).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[tokio::test]
    async fn test_record_unregistered_metric() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let result = monitoring
            .record_metric("nonexistent", 42.0, HashMap::new())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[tokio::test]
    async fn test_aggregate_min_max_count() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let labels = HashMap::new();
        monitoring
            .record_metric("test_metric", 10.0, labels.clone())
            .await
            .unwrap();
        monitoring
            .record_metric("test_metric", 30.0, labels.clone())
            .await
            .unwrap();
        monitoring
            .record_metric("test_metric", 20.0, labels.clone())
            .await
            .unwrap();

        let start = Utc::now() - Duration::minutes(1);
        let end = Utc::now() + Duration::minutes(1);

        let min = monitoring
            .aggregate_metric("test_metric", None, MetricAggregation::Min, start, end)
            .await
            .unwrap();
        assert_eq!(min, 10.0);

        let max = monitoring
            .aggregate_metric("test_metric", None, MetricAggregation::Max, start, end)
            .await
            .unwrap();
        assert_eq!(max, 30.0);

        let count = monitoring
            .aggregate_metric("test_metric", None, MetricAggregation::Count, start, end)
            .await
            .unwrap();
        assert_eq!(count, 3.0);
    }

    #[tokio::test]
    async fn test_aggregate_percentile() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "latency_ms".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Percentile(0.95),
        };

        monitoring.register_metric(definition).await.unwrap();

        let labels = HashMap::new();
        for v in 1..=100 {
            monitoring
                .record_metric("latency_ms", v as f64, labels.clone())
                .await
                .unwrap();
        }

        let start = Utc::now() - Duration::minutes(1);
        let end = Utc::now() + Duration::minutes(1);

        let p95 = monitoring
            .aggregate_metric(
                "latency_ms",
                None,
                MetricAggregation::Percentile(0.95),
                start,
                end,
            )
            .await
            .unwrap();
        // 95th percentile of 1..=100 should be close to 95
        assert!((90.0..=100.0).contains(&p95));
    }

    #[tokio::test]
    async fn test_aggregate_no_data() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let result = monitoring
            .aggregate_metric(
                "nonexistent",
                None,
                MetricAggregation::Average,
                Utc::now() - Duration::minutes(1),
                Utc::now() + Duration::minutes(1),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_metric_nonexistent() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let result = monitoring.get_metric("nonexistent", &HashMap::new()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_performance_metrics_averaging() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        for i in 0..3 {
            let metrics = PerformanceMetrics {
                cpu_usage_percent: (i + 1) as f64 * 10.0, // 10, 20, 30
                memory_usage_mb: 1024.0,
                memory_usage_percent: 50.0,
                disk_io_read_mb: 0.0,
                disk_io_write_mb: 0.0,
                network_rx_mb: 0.0,
                network_tx_mb: 0.0,
                open_file_descriptors: 100,
                thread_count: 10,
                timestamp: Utc::now(),
            };
            monitoring.record_performance(metrics).await;
        }

        let avg = monitoring.get_average_performance(5).await;
        assert!(avg.is_some());
        let avg = avg.unwrap();
        // Average of 10, 20, 30 = 20.0
        assert_eq!(avg.cpu_usage_percent, 20.0);
    }

    #[tokio::test]
    async fn test_get_average_performance_no_data() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let avg = monitoring.get_average_performance(5).await;
        assert!(avg.is_none());
    }

    #[tokio::test]
    async fn test_alert_history() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let alert = AlertDefinition {
            id: "test_alert".to_string(),
            name: "Test Alert".to_string(),
            description: "Test".to_string(),
            severity: AlertSeverity::Critical,
            condition: AlertCondition {
                metric_name: "test_metric".to_string(),
                operator: AlertOperator::GreaterThan,
                threshold: 50.0,
                duration_seconds: 60,
            },
            labels: HashMap::new(),
            enabled: true,
        };

        monitoring.create_alert(alert).await.unwrap();

        let labels = HashMap::new();
        monitoring
            .record_metric("test_metric", 100.0, labels)
            .await
            .unwrap();

        monitoring.evaluate_alerts().await.unwrap();

        let history = monitoring.get_alert_history(None).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].severity, AlertSeverity::Critical);

        let limited = monitoring.get_alert_history(Some(5)).await;
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn test_get_all_metrics() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "cpu_usage".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec!["host".to_string()],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let mut labels1 = HashMap::new();
        labels1.insert("host".to_string(), "server1".to_string());
        let mut labels2 = HashMap::new();
        labels2.insert("host".to_string(), "server2".to_string());

        monitoring
            .record_metric("cpu_usage", 50.0, labels1)
            .await
            .unwrap();
        monitoring
            .record_metric("cpu_usage", 75.0, labels2)
            .await
            .unwrap();

        let all = monitoring.get_all_metrics().await;
        assert!(all.contains_key("cpu_usage"));
        assert_eq!(all["cpu_usage"].len(), 2);
    }

    #[tokio::test]
    async fn test_alert_definitions_retrieval() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let alert = AlertDefinition {
            id: "a1".to_string(),
            name: "Alert One".to_string(),
            description: "Desc".to_string(),
            severity: AlertSeverity::Info,
            condition: AlertCondition {
                metric_name: "m1".to_string(),
                operator: AlertOperator::LessThan,
                threshold: 5.0,
                duration_seconds: 30,
            },
            labels: HashMap::new(),
            enabled: true,
        };

        monitoring.create_alert(alert).await.unwrap();

        let defs = monitoring.get_alert_definitions().await;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Alert One");
    }

    #[tokio::test]
    async fn test_monitoring_statistics_initial() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let stats = monitoring.get_statistics().await;

        assert_eq!(stats.total_time_series, 0);
        assert_eq!(stats.total_data_points, 0);
        assert_eq!(stats.registered_metrics, 0);
        assert_eq!(stats.registered_alerts, 0);
        assert_eq!(stats.active_alerts, 0);
        assert_eq!(stats.resolved_alerts, 0);
        assert_eq!(stats.performance_samples, 0);
        assert_eq!(stats.current_alert_severity, AlertSeverity::Info);
    }

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Emergency > AlertSeverity::Critical);
        assert!(AlertSeverity::Critical > AlertSeverity::Warning);
        assert!(AlertSeverity::Warning > AlertSeverity::Info);
    }

    #[test]
    fn test_alert_operator_serialization() {
        for op in [
            AlertOperator::GreaterThan,
            AlertOperator::LessThan,
            AlertOperator::Equals,
            AlertOperator::NotEquals,
            AlertOperator::GreaterOrEqual,
            AlertOperator::LessOrEqual,
        ] {
            let json = serde_json::to_string(&op).unwrap();
            let back: AlertOperator = serde_json::from_str(&json).unwrap();
            assert_eq!(back, op);
        }
    }

    #[tokio::test]
    async fn test_disabled_alert_not_triggered() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };

        monitoring.register_metric(definition).await.unwrap();

        let alert = AlertDefinition {
            id: "disabled_alert".to_string(),
            name: "Disabled Alert".to_string(),
            description: "Should not fire".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition {
                metric_name: "test_metric".to_string(),
                operator: AlertOperator::GreaterThan,
                threshold: 50.0,
                duration_seconds: 60,
            },
            labels: HashMap::new(),
            enabled: false,
        };

        monitoring.create_alert(alert).await.unwrap();

        monitoring
            .record_metric("test_metric", 100.0, HashMap::new())
            .await
            .unwrap();

        let triggered = monitoring.evaluate_alerts().await.unwrap();
        assert!(triggered.is_empty());
    }

    #[tokio::test]
    async fn test_query_metric_range() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());

        let definition = MetricDefinition {
            name: "requests".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Sum,
        };

        monitoring.register_metric(definition).await.unwrap();

        let labels = HashMap::new();
        monitoring
            .record_metric("requests", 100.0, labels.clone())
            .await
            .unwrap();
        monitoring
            .record_metric("requests", 200.0, labels.clone())
            .await
            .unwrap();

        let start = Utc::now() - Duration::minutes(1);
        let end = Utc::now() + Duration::minutes(1);

        let points = monitoring
            .query_metric_range("requests", &labels, start, end)
            .await;
        assert_eq!(points.len(), 2);
    }

    // ===== 15 new tests =====

    #[test]
    fn test_serde_metric_type_roundtrip() {
        let variants = [
            MetricType::Counter,
            MetricType::Gauge,
            MetricType::Histogram,
            MetricType::Summary,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let back: MetricType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_serde_metric_data_point_roundtrip() {
        let mut labels = HashMap::new();
        labels.insert("region".to_string(), "us-east".to_string());
        let point = MetricDataPoint {
            timestamp: Utc::now(),
            value: 99.5,
            labels: labels.clone(),
            metadata: MetricMetadata {
                metric_type: MetricType::Histogram,
                description: "latency".to_string(),
                unit: "ms".to_string(),
                source: "agent".to_string(),
                agent_role: None,
            },
        };
        let json = serde_json::to_string(&point).unwrap();
        let back: MetricDataPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.value, 99.5);
        assert_eq!(back.labels, labels);
        assert_eq!(back.metadata.metric_type, MetricType::Histogram);
    }

    #[test]
    fn test_serde_metric_definition_roundtrip() {
        let def = MetricDefinition {
            name: "req_count".to_string(),
            metadata: MetricMetadata {
                metric_type: MetricType::Counter,
                description: "request count".to_string(),
                unit: "count".to_string(),
                source: "gateway".to_string(),
                agent_role: Some(AgentRole::Reviewer),
            },
            labels: vec!["method".to_string(), "path".to_string()],
            aggregation: MetricAggregation::Sum,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: MetricDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "req_count");
        assert_eq!(back.labels.len(), 2);
        assert!(back.metadata.agent_role.is_some());
    }

    #[test]
    fn test_serde_metric_aggregation_roundtrip() {
        let variants = [
            MetricAggregation::Sum,
            MetricAggregation::Average,
            MetricAggregation::Min,
            MetricAggregation::Max,
            MetricAggregation::Count,
            MetricAggregation::Percentile(0.99),
            MetricAggregation::Percentile(0.50),
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let back: MetricAggregation = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_serde_alert_severity_roundtrip() {
        let variants = [
            AlertSeverity::Info,
            AlertSeverity::Warning,
            AlertSeverity::Critical,
            AlertSeverity::Emergency,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let back: AlertSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_serde_alert_status_roundtrip() {
        let variants = [
            AlertStatus::Active,
            AlertStatus::Acknowledged,
            AlertStatus::Resolved,
            AlertStatus::Suppressed,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let back: AlertStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_serde_performance_metrics_roundtrip() {
        let pm = PerformanceMetrics {
            cpu_usage_percent: 55.5,
            memory_usage_mb: 2048.0,
            memory_usage_percent: 33.3,
            disk_io_read_mb: 10.0,
            disk_io_write_mb: 5.0,
            network_rx_mb: 100.0,
            network_tx_mb: 80.0,
            open_file_descriptors: 256,
            thread_count: 16,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&pm).unwrap();
        let back: PerformanceMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cpu_usage_percent, 55.5);
        assert_eq!(back.memory_usage_mb, 2048.0);
        assert_eq!(back.open_file_descriptors, 256);
        assert_eq!(back.thread_count, 16);
    }

    #[test]
    fn test_serde_monitoring_statistics_roundtrip() {
        let stats = MonitoringStatistics {
            total_time_series: 42,
            total_data_points: 1000,
            registered_metrics: 10,
            registered_alerts: 5,
            active_alerts: 2,
            resolved_alerts: 3,
            performance_samples: 100,
            current_alert_severity: AlertSeverity::Critical,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: MonitoringStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_time_series, 42);
        assert_eq!(back.total_data_points, 1000);
        assert_eq!(back.current_alert_severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_serde_alert_event_roundtrip() {
        let evt = AlertEvent {
            id: "alert_7".to_string(),
            alert_id: "high_cpu".to_string(),
            severity: AlertSeverity::Emergency,
            status: AlertStatus::Active,
            message: "CPU usage critical".to_string(),
            triggered_at: Utc::now(),
            acknowledged_at: None,
            resolved_at: None,
            metric_value: 99.9,
            labels: HashMap::new(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        let back: AlertEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "alert_7");
        assert_eq!(back.severity, AlertSeverity::Emergency);
        assert_eq!(back.metric_value, 99.9);
        assert!(back.acknowledged_at.is_none());
    }

    #[test]
    fn test_serde_alert_condition_roundtrip() {
        let cond = AlertCondition {
            metric_name: "memory_pct".to_string(),
            operator: AlertOperator::GreaterOrEqual,
            threshold: 90.0,
            duration_seconds: 120,
        };
        let json = serde_json::to_string(&cond).unwrap();
        let back: AlertCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metric_name, "memory_pct");
        assert_eq!(back.operator, AlertOperator::GreaterOrEqual);
        assert_eq!(back.threshold, 90.0);
        assert_eq!(back.duration_seconds, 120);
    }

    #[test]
    fn test_time_series_query_range_empty() {
        let series = TimeSeries::new("empty".to_string(), HashMap::new(), 10);
        let results = series.query_range(Utc::now(), Utc::now());
        assert!(results.is_empty());
    }

    #[test]
    fn test_series_key_deterministic_label_ordering() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let mut labels_a = HashMap::new();
        labels_a.insert("zone".to_string(), "a".to_string());
        labels_a.insert("host".to_string(), "srv1".to_string());

        let mut labels_b = HashMap::new();
        labels_b.insert("host".to_string(), "srv1".to_string());
        labels_b.insert("zone".to_string(), "a".to_string());

        let key_a = monitoring.series_key("cpu", &labels_a);
        let key_b = monitoring.series_key("cpu", &labels_b);
        assert_eq!(
            key_a, key_b,
            "series_key should be deterministic regardless of label insertion order"
        );
    }

    #[tokio::test]
    async fn test_acknowledge_nonexistent_alert_returns_error() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let result = monitoring.acknowledge_alert("ghost_alert").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_alert_returns_error() {
        let monitoring = MonitoringSystem::new(MonitoringConfig::default());
        let result = monitoring.resolve_alert("ghost_alert").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_alerts_disabled_yields_no_events() {
        let config = MonitoringConfig {
            enable_alerts: false,
            ..Default::default()
        };
        let monitoring = MonitoringSystem::new(config);

        let definition = MetricDefinition {
            name: "cpu".to_string(),
            metadata: MetricMetadata::default(),
            labels: vec![],
            aggregation: MetricAggregation::Average,
        };
        monitoring.register_metric(definition).await.unwrap();

        let alert = AlertDefinition {
            id: "alert1".to_string(),
            name: "CPU Alert".to_string(),
            description: "test".to_string(),
            severity: AlertSeverity::Critical,
            condition: AlertCondition {
                metric_name: "cpu".to_string(),
                operator: AlertOperator::GreaterThan,
                threshold: 10.0,
                duration_seconds: 0,
            },
            labels: HashMap::new(),
            enabled: true,
        };
        monitoring.create_alert(alert).await.unwrap();

        monitoring
            .record_metric("cpu", 99.0, HashMap::new())
            .await
            .unwrap();

        let events = monitoring.evaluate_alerts().await.unwrap();
        assert!(
            events.is_empty(),
            "No alerts should fire when enable_alerts is false"
        );
    }
}
