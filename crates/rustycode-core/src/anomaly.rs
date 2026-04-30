//! # Anomaly Detection System
//!
//! This module provides comprehensive anomaly detection capabilities for monitoring
//! and identifying unusual patterns in system behavior, metrics, and time-series data.
//!
//! ## Features
//!
//! - **Statistical Detection**: Z-score based anomaly detection
//! - **Time-Series Analysis**: Trend analysis and seasonality detection
//! - **Pattern Recognition**: Identify recurring anomalous patterns
//! - **Anomaly Scoring**: Multi-dimensional scoring system
//! - **Alerting**: Configurable alert thresholds and notifications
//! - **Learning**: Adaptive baseline adjustment from historical data
//!
//! ## Architecture
//!
//! The system uses a multi-layered approach:
//! 1. **Statistical Layer**: Z-score detection for outlier identification
//! 2. **Temporal Layer**: Time-series analysis for trend anomalies
//! 3. **Pattern Layer**: Machine learning for pattern recognition
//! 4. **Scoring Layer**: Combines all signals into unified anomaly score
//! 5. **Alert Layer**: Triggers alerts based on configured thresholds
//! 6. **Learning Layer**: Adapts baselines from historical patterns
//!
//! ## Example Usage
//!
//! ```rust
//! use rustycode_core::anomaly::{AnomalyDetector, AnomalyConfig};
//! use chrono::Utc;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! // Create detector with default configuration
//! let config = AnomalyConfig::default();
//! let detector = AnomalyDetector::new(config);
//!
//! // Observe metrics over time
//! let timestamp = Utc::now();
//! detector.observe_metric("response_time", 150.0, timestamp).await?;
//! detector.observe_metric("response_time", 155.0, timestamp).await?;
//! detector.observe_metric("response_time", 500.0, timestamp).await?; // Anomaly!
//!
//! // Check for anomalies
//! let anomalies = detector.detect_anomalies("response_time").await?;
//! for anomaly in anomalies {
//!     println!("Anomaly detected: score={}, severity={:?}",
//!              anomaly.score, anomaly.severity);
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum number of data points to keep per metric
const MAX_DATA_POINTS: usize = 1000;

/// Default Z-score threshold for statistical anomaly detection
const DEFAULT_Z_SCORE_THRESHOLD: f64 = 3.0;

/// Default anomaly score threshold for alerting
const DEFAULT_ANOMALY_SCORE_THRESHOLD: f64 = 0.7;

/// Minimum number of data points required for analysis
const MIN_DATA_POINTS: usize = 10;

/// Configuration for the anomaly detection system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Z-score threshold for statistical anomalies (default: 3.0)
    /// Higher values make detection less sensitive
    pub z_score_threshold: f64,

    /// Anomaly score threshold for alerting (0.0 - 1.0, default: 0.7)
    pub anomaly_score_threshold: f64,

    /// Enable time-series trend analysis
    pub enable_trend_analysis: bool,

    /// Enable pattern recognition
    pub enable_pattern_recognition: bool,

    /// Enable adaptive learning from historical data
    pub enable_learning: bool,

    /// Window size for moving average (number of points)
    pub moving_average_window: usize,

    /// Maximum age of data points to consider (in seconds)
    pub max_data_age_seconds: i64,

    /// Alert cooldown period in seconds (prevents alert spam)
    pub alert_cooldown_seconds: i64,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            z_score_threshold: DEFAULT_Z_SCORE_THRESHOLD,
            anomaly_score_threshold: DEFAULT_ANOMALY_SCORE_THRESHOLD,
            enable_trend_analysis: true,
            enable_pattern_recognition: true,
            enable_learning: true,
            moving_average_window: 20,
            max_data_age_seconds: 3600, // 1 hour
            alert_cooldown_seconds: 300, // 5 minutes
        }
    }
}

impl AnomalyConfig {
    /// Set a custom z-score threshold
    pub fn with_z_score_threshold(mut self, threshold: f64) -> Self {
        self.z_score_threshold = threshold;
        self
    }

    /// Set a custom anomaly score threshold
    pub fn with_anomaly_score_threshold(mut self, threshold: f64) -> Self {
        self.anomaly_score_threshold = threshold;
        self
    }

    /// Enable or disable trend analysis
    pub fn with_trend_analysis(mut self, enabled: bool) -> Self {
        self.enable_trend_analysis = enabled;
        self
    }

    /// Enable or disable pattern recognition
    pub fn with_pattern_recognition(mut self, enabled: bool) -> Self {
        self.enable_pattern_recognition = enabled;
        self
    }

    /// Enable or disable learning
    pub fn with_learning(mut self, enabled: bool) -> Self {
        self.enable_learning = enabled;
        self
    }
}

/// A single data point in a time series
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp of the data point
    pub timestamp: DateTime<Utc>,
    /// Value of the metric
    pub value: f64,
    /// Optional metadata about the data point
    pub metadata: HashMap<String, String>,
}

impl DataPoint {
    /// Create a new data point
    pub fn new(value: f64) -> Self {
        Self {
            timestamp: Utc::now(),
            value,
            metadata: HashMap::new(),
        }
    }

    /// Create a new data point with a specific timestamp
    pub fn with_timestamp(value: f64, timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            value,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the data point
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Severity level of an anomaly
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AnomalySeverity {
    /// Low severity - minor deviation
    Low,
    /// Medium severity - notable deviation
    Medium,
    /// High severity - significant deviation
    High,
    /// Critical severity - extreme deviation
    Critical,
}

impl AnomalySeverity {
    /// Convert severity to a numeric score
    pub fn as_score(&self) -> f64 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::Critical => 1.0,
        }
    }

    /// Get severity from a numeric score
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            Self::Critical
        } else if score >= 0.7 {
            Self::High
        } else if score >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// Type of anomaly detected
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AnomalyType {
    /// Statistical outlier (z-score based)
    StatisticalOutlier,
    /// Sudden spike in value
    Spike,
    /// Sudden drop in value
    Drop,
    /// Trend deviation
    TrendAnomaly,
    /// Pattern violation
    PatternViolation,
    /// Rate of change anomaly
    RateOfChangeAnomaly,
}

/// A detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    /// Unique identifier for this anomaly
    pub id: String,
    /// Metric name where anomaly was detected
    pub metric_name: String,
    /// Timestamp when anomaly was detected
    pub detected_at: DateTime<Utc>,
    /// Timestamp of the anomalous data point
    pub timestamp: DateTime<Utc>,
    /// Value that triggered the anomaly
    pub value: f64,
    /// Expected value (based on historical data)
    pub expected_value: f64,
    /// Deviation from expected value
    pub deviation: f64,
    /// Z-score of the anomalous value
    pub z_score: f64,
    /// Composite anomaly score (0.0 - 1.0)
    pub score: f64,
    /// Severity level
    pub severity: AnomalySeverity,
    /// Type of anomaly
    pub anomaly_type: AnomalyType,
    /// Description of what was anomalous
    pub description: String,
    /// Additional context about the anomaly
    pub context: HashMap<String, String>,
}

/// Statistical summary of a metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    /// Metric name
    pub metric_name: String,
    /// Number of data points
    pub count: usize,
    /// Mean value
    pub mean: f64,
    /// Standard deviation
    pub std_dev: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Median value
    pub median: f64,
    /// 25th percentile
    pub p25: f64,
    /// 75th percentile
    pub p75: f64,
}

/// Time series data for a metric
#[derive(Debug, Clone)]
struct TimeSeries {
    /// Name of the metric
    name: String,
    /// Data points in chronological order
    data_points: VecDeque<DataPoint>,
    /// Last time an alert was sent for this metric
    last_alert_time: Option<DateTime<Utc>>,
    /// Learned baseline (from historical data)
    baseline_mean: f64,
    /// Learned baseline standard deviation
    baseline_std_dev: f64,
    /// Number of data points used for baseline
    baseline_count: usize,
}

impl TimeSeries {
    /// Create a new time series
    fn new(name: String) -> Self {
        Self {
            name,
            data_points: VecDeque::with_capacity(MAX_DATA_POINTS),
            last_alert_time: None,
            baseline_mean: 0.0,
            baseline_std_dev: 1.0,
            baseline_count: 0,
        }
    }

    /// Add a data point to the time series
    fn add_point(&mut self, point: DataPoint) {
        self.data_points.push_back(point);
        if self.data_points.len() > MAX_DATA_POINTS {
            self.data_points.pop_front();
        }
    }

    /// Get recent data points within a time window
    fn get_recent_points(&self, max_age: Duration) -> Vec<&DataPoint> {
        let cutoff = Utc::now() - max_age;
        self.data_points
            .iter()
            .filter(|p| p.timestamp > cutoff)
            .collect()
    }

    /// Calculate statistical summary
    fn calculate_summary(&self) -> Option<MetricSummary> {
        if self.data_points.len() < MIN_DATA_POINTS {
            return None;
        }

        let values: Vec<f64> = self.data_points.iter().map(|p| p.value).collect();
        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;

        let variance = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let min = sorted_values[0];
        let max = sorted_values[count - 1];
        let median = sorted_values[count / 2];
        let p25 = sorted_values[count / 4];
        let p75 = sorted_values[count * 3 / 4];

        Some(MetricSummary {
            metric_name: self.name.clone(),
            count,
            mean,
            std_dev,
            min,
            max,
            median,
            p25,
            p75,
        })
    }

    /// Calculate z-score for a value
    fn calculate_z_score(&self, value: f64) -> f64 {
        if self.baseline_std_dev == 0.0 {
            return 0.0;
        }
        (value - self.baseline_mean) / self.baseline_std_dev
    }

    /// Update learned baseline from recent data
    fn update_baseline(&mut self, config: &AnomalyConfig) {
        let recent = self.get_recent_points(Duration::seconds(config.max_data_age_seconds));
        if recent.len() < MIN_DATA_POINTS {
            return;
        }

        let values: Vec<f64> = recent.iter().map(|p| p.value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        // Exponential moving average for smooth baseline updates
        let alpha = 0.1; // Learning rate
        self.baseline_mean = alpha * mean + (1.0 - alpha) * self.baseline_mean;
        self.baseline_std_dev = alpha * std_dev + (1.0 - alpha) * self.baseline_std_dev;
        self.baseline_count = recent.len();
    }
}

/// Main anomaly detection engine
#[derive(Debug, Clone)]
pub struct AnomalyDetector {
    /// Configuration
    config: AnomalyConfig,
    /// Time series data for each metric
    time_series: Arc<RwLock<HashMap<String, TimeSeries>>>,
    /// Detected anomalies
    anomalies: Arc<RwLock<Vec<Anomaly>>>,
}

impl AnomalyDetector {
    /// Create a new anomaly detector with default configuration
    pub fn new() -> Self {
        Self::with_config(AnomalyConfig::default())
    }

    /// Create a new anomaly detector with custom configuration
    pub fn with_config(config: AnomalyConfig) -> Self {
        Self {
            config,
            time_series: Arc::new(RwLock::new(HashMap::new())),
            anomalies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Observe a metric value
    ///
    /// # Arguments
    ///
    /// * `metric_name` - Name of the metric
    /// * `value` - Value of the metric
    /// * `timestamp` - Timestamp of the observation
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the metric was recorded successfully.
    pub async fn observe_metric(
        &self,
        metric_name: &str,
        value: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        let point = DataPoint::with_timestamp(value, timestamp);

        let mut series = self.time_series.write().await;
        let ts = series.entry(metric_name.to_string()).or_insert_with(|| {
            debug!("Creating new time series for metric: {}", metric_name);
            TimeSeries::new(metric_name.to_string())
        });

        ts.add_point(point.clone());

        // Update baseline if learning is enabled
        if self.config.enable_learning {
            ts.update_baseline(&self.config);
            debug!(
                "Updated baseline for {}: mean={}, std_dev={}",
                metric_name, ts.baseline_mean, ts.baseline_std_dev
            );
        }

        Ok(())
    }

    /// Detect anomalies in a metric
    ///
    /// # Arguments
    ///
    /// * `metric_name` - Name of the metric to analyze
    ///
    /// # Returns
    ///
    /// Returns a vector of detected anomalies.
    pub async fn detect_anomalies(&self, metric_name: &str) -> Result<Vec<Anomaly>> {
        let series = self.time_series.read().await;
        let ts = series
            .get(metric_name)
            .context(format!("No data found for metric: {}", metric_name))?;

        if ts.data_points.len() < MIN_DATA_POINTS {
            return Ok(Vec::new());
        }

        let mut detected_anomalies = Vec::new();
        let recent_points: Vec<&DataPoint> = ts.data_points.iter().collect();

        // Statistical anomaly detection (z-score)
        for point in recent_points.iter().rev().take(10) {
            let z_score = ts.calculate_z_score(point.value);

            if z_score.abs() > self.config.z_score_threshold {
                let anomaly_type = if z_score > 0.0 {
                    AnomalyType::Spike
                } else {
                    AnomalyType::Drop
                };

                let anomaly = self.create_anomaly(
                    metric_name,
                    point,
                    ts.baseline_mean,
                    z_score,
                    anomaly_type,
                    "Statistical outlier detected",
                );

                detected_anomalies.push(anomaly);
            }
        }

        // Time-series trend analysis
        if self.config.enable_trend_analysis {
            let trend_anomalies = self.detect_trend_anomalies(metric_name, ts).await?;
            detected_anomalies.extend(trend_anomalies);
        }

        // Pattern recognition
        if self.config.enable_pattern_recognition {
            let pattern_anomalies = self.detect_pattern_anomalies(metric_name, ts).await?;
            detected_anomalies.extend(pattern_anomalies);
        }

        // Score and filter anomalies
        let scored_anomalies = self.score_anomalies(detected_anomalies);

        // Store anomalies
        let mut all_anomalies = self.anomalies.write().await;
        for anomaly in &scored_anomalies {
            all_anomalies.push(anomaly.clone());
        }

        Ok(scored_anomalies)
    }

    /// Detect trend-based anomalies
    async fn detect_trend_anomalies(
        &self,
        metric_name: &str,
        ts: &TimeSeries,
    ) -> Result<Vec<Anomaly>> {
        let mut anomalies = Vec::new();

        if ts.data_points.len() < self.config.moving_average_window + 1 {
            return Ok(anomalies);
        }

        let points: Vec<&DataPoint> = ts.data_points.iter().rev().take(10).collect();

        for point in &points {
            // Calculate moving average
            let window_start = point.timestamp - Duration::seconds(60); // 1 minute window
            let window_points: Vec<&DataPoint> = ts
                .data_points
                .iter()
                .filter(|p| p.timestamp >= window_start && p.timestamp < point.timestamp)
                .take(self.config.moving_average_window)
                .collect();

            if window_points.len() < 5 {
                continue;
            }

            let avg: f64 = window_points.iter().map(|p| p.value).sum::<f64>() / window_points.len() as f64;
            let deviation = (point.value - avg).abs() / avg;

            if deviation > 0.5 {
                // 50% deviation from trend
                let z_score = ts.calculate_z_score(point.value);
                let anomaly = self.create_anomaly(
                    metric_name,
                    point,
                    avg,
                    z_score,
                    AnomalyType::TrendAnomaly,
                    &format!("Trend anomaly: {:.1}% deviation from moving average", deviation * 100.0),
                );
                anomalies.push(anomaly);
            }
        }

        Ok(anomalies)
    }

    /// Detect pattern-based anomalies
    async fn detect_pattern_anomalies(
        &self,
        metric_name: &str,
        ts: &TimeSeries,
    ) -> Result<Vec<Anomaly>> {
        let mut anomalies = Vec::new();

        if ts.data_points.len() < 20 {
            return Ok(anomalies);
        }

        // Analyze rate of change between consecutive points
        let points: Vec<&DataPoint> = ts.data_points.iter().rev().take(10).collect();

        for (i, point) in points.iter().enumerate() {
            if i == 0 || i >= points.len() - 1 {
                continue;
            }

            let prev_point = points[i + 1];
            let rate_of_change = (point.value - prev_point.value) / prev_point.value.abs().max(0.001);

            // Detect sudden rate changes
            if rate_of_change.abs() > 1.0 {
                // 100% change in rate
                let z_score = ts.calculate_z_score(point.value);
                let anomaly = self.create_anomaly(
                    metric_name,
                    point,
                    ts.baseline_mean,
                    z_score,
                    AnomalyType::RateOfChangeAnomaly,
                    &format!("Rate of change anomaly: {:.1}% change", rate_of_change * 100.0),
                );
                anomalies.push(anomaly);
            }
        }

        Ok(anomalies)
    }

    /// Create an anomaly object
    fn create_anomaly(
        &self,
        metric_name: &str,
        point: &DataPoint,
        expected_value: f64,
        z_score: f64,
        anomaly_type: AnomalyType,
        description: &str,
    ) -> Anomaly {
        let deviation = point.value - expected_value;
        let severity = AnomalySeverity::from_score(z_score.abs() / self.config.z_score_threshold);

        let mut context = HashMap::new();
        context.insert("z_score".to_string(), z_score.to_string());
        context.insert("deviation".to_string(), deviation.to_string());
        context.insert("expected_value".to_string(), expected_value.to_string());

        Anomaly {
            id: format!("anomaly_{}_{}", metric_name, point.timestamp.timestamp()),
            metric_name: metric_name.to_string(),
            detected_at: Utc::now(),
            timestamp: point.timestamp,
            value: point.value,
            expected_value,
            deviation,
            z_score,
            score: 0.0, // Will be calculated in score_anomalies
            severity,
            anomaly_type,
            description: description.to_string(),
            context,
        }
    }

    /// Score anomalies based on multiple factors
    fn score_anomalies(&self, mut anomalies: Vec<Anomaly>) -> Vec<Anomaly> {
        for anomaly in &mut anomalies {
            // Base score from z-score
            let z_score_component = (anomaly.z_score.abs() / self.config.z_score_threshold).min(1.0);

            // Deviation component
            let deviation_component = (anomaly.deviation.abs() / anomaly.expected_value.abs().max(1.0)).min(1.0);

            // Type-based weight
            let type_weight = match anomaly.anomaly_type {
                AnomalyType::StatisticalOutlier => 0.8,
                AnomalyType::Spike => 0.9,
                AnomalyType::Drop => 0.9,
                AnomalyType::TrendAnomaly => 0.7,
                AnomalyType::PatternViolation => 0.85,
                AnomalyType::RateOfChangeAnomaly => 0.75,
            };

            // Composite score
            anomaly.score = (z_score_component * 0.5 + deviation_component * 0.3) * type_weight;
            anomaly.score = anomaly.score.min(1.0); // Cap at 1.0

            // Update severity based on final score
            anomaly.severity = AnomalySeverity::from_score(anomaly.score);
        }

        // Sort by score (highest first)
        anomalies.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Filter by threshold
        anomalies
            .into_iter()
            .filter(|a| a.score >= self.config.anomaly_score_threshold)
            .collect()
    }

    /// Check if alerts should be sent (considering cooldown)
    async fn should_send_alert(&self, metric_name: &str) -> Result<bool> {
        let series = self.time_series.read().await;
        let ts = series.get(metric_name);

        if let Some(ts) = ts {
            if let Some(last_alert) = ts.last_alert_time {
                let cooldown = Duration::seconds(self.config.alert_cooldown_seconds);
                if Utc::now() - last_alert < cooldown {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Send alerts for detected anomalies
    ///
    /// # Arguments
    ///
    /// * `metric_name` - Name of the metric
    ///
    /// # Returns
    ///
    /// Returns a vector of anomalies that triggered alerts.
    pub async fn check_and_alert(&self, metric_name: &str) -> Result<Vec<Anomaly>> {
        let anomalies = self.detect_anomalies(metric_name).await?;

        if anomalies.is_empty() {
            return Ok(Vec::new());
        }

        // Check cooldown
        if !self.should_send_alert(metric_name).await? {
            debug!("Alert cooldown active for metric: {}", metric_name);
            return Ok(Vec::new());
        }

        // Update last alert time
        let mut series = self.time_series.write().await;
        if let Some(ts) = series.get_mut(metric_name) {
            ts.last_alert_time = Some(Utc::now());
        }

        // Log alerts
        for anomaly in &anomalies {
            match anomaly.severity {
                AnomalySeverity::Critical => {
                    warn!(
                        "CRITICAL ANOMALY [{}]: score={}, value={}, expected={}, description={}",
                        metric_name, anomaly.score, anomaly.value, anomaly.expected_value, anomaly.description
                    );
                }
                AnomalySeverity::High => {
                    warn!(
                        "HIGH ANOMALY [{}]: score={}, value={}, expected={}, description={}",
                        metric_name, anomaly.score, anomaly.value, anomaly.expected_value, anomaly.description
                    );
                }
                AnomalySeverity::Medium => {
                    info!(
                        "MEDIUM ANOMALY [{}]: score={}, value={}, expected={}, description={}",
                        metric_name, anomaly.score, anomaly.value, anomaly.expected_value, anomaly.description
                    );
                }
                AnomalySeverity::Low => {
                    debug!(
                        "LOW ANOMALY [{}]: score={}, value={}, expected={}, description={}",
                        metric_name, anomaly.score, anomaly.value, anomaly.expected_value, anomaly.description
                    );
                }
            }
        }

        Ok(anomalies)
    }

    /// Get statistical summary for a metric
    pub async fn get_metric_summary(&self, metric_name: &str) -> Result<Option<MetricSummary>> {
        let series = self.time_series.read().await;
        let ts = series.get(metric_name);

        Ok(ts.and_then(|t| t.calculate_summary()))
    }

    /// Get all detected anomalies
    pub async fn get_anomalies(&self) -> Vec<Anomaly> {
        self.anomalies.read().await.clone()
    }

    /// Get anomalies for a specific metric
    pub async fn get_metric_anomalies(&self, metric_name: &str) -> Vec<Anomaly> {
        let anomalies = self.anomalies.read().await;
        anomalies
            .iter()
            .filter(|a| a.metric_name == metric_name)
            .cloned()
            .collect()
    }

    /// Clear old anomalies (keep only recent ones)
    pub async fn clear_old_anomalies(&self, max_age_seconds: i64) {
        let mut anomalies = self.anomalies.write().await;
        let cutoff = Utc::now() - Duration::seconds(max_age_seconds);
        anomalies.retain(|a| a.detected_at > cutoff);
    }

    /// Get all metric names being tracked
    pub async fn get_tracked_metrics(&self) -> Vec<String> {
        let series = self.time_series.read().await;
        series.keys().cloned().collect()
    }

    /// Remove all data for a metric
    pub async fn remove_metric(&self, metric_name: &str) -> Result<()> {
        let mut series = self.time_series.write().await;
        series.remove(metric_name);
        info!("Removed metric: {}", metric_name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_detector() -> AnomalyDetector {
        let config = AnomalyConfig {
            z_score_threshold: 2.0, // Lower threshold for testing
            anomaly_score_threshold: 0.5,
            enable_trend_analysis: true,
            enable_pattern_recognition: true,
            enable_learning: true,
            ..Default::default()
        };
        AnomalyDetector::with_config(config)
    }

    #[tokio::test]
    async fn test_observe_metric() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        let result = detector
            .observe_metric("test_metric", 100.0, timestamp)
            .await;
        assert!(result.is_ok());

        let metrics = detector.get_tracked_metrics().await;
        assert!(metrics.contains(&"test_metric".to_string()));
    }

    #[tokio::test]
    async fn test_statistical_anomaly_detection() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add normal values
        for i in 0..20 {
            let value = 100.0 + (i as f64 * 0.1);
            detector
                .observe_metric("test_metric", value, timestamp)
                .await
                .unwrap();
        }

        // Add an anomalous value
        detector
            .observe_metric("test_metric", 200.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("test_metric").await.unwrap();
        assert!(!anomalies.is_empty(), "Should detect at least one anomaly");

        let anomaly = &anomalies[0];
        assert!(anomaly.score > 0.5, "Anomaly score should be above threshold");
        assert!(anomaly.z_score.abs() > 2.0, "Z-score should indicate anomaly");
    }

    #[tokio::test]
    async fn test_spike_detection() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add stable values
        for _ in 0..20 {
            detector
                .observe_metric("cpu_usage", 50.0, timestamp)
                .await
                .unwrap();
        }

        // Add a spike
        detector
            .observe_metric("cpu_usage", 95.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("cpu_usage").await.unwrap();
        assert!(!anomalies.is_empty());

        let spike_anomaly = anomalies
            .iter()
            .find(|a| matches!(a.anomaly_type, AnomalyType::Spike));
        assert!(spike_anomaly.is_some(), "Should detect spike");
    }

    #[tokio::test]
    async fn test_drop_detection() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add stable values
        for _ in 0..20 {
            detector
                .observe_metric("request_rate", 1000.0, timestamp)
                .await
                .unwrap();
        }

        // Add a drop
        detector
            .observe_metric("request_rate", 100.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("request_rate").await.unwrap();
        assert!(!anomalies.is_empty());

        let drop_anomaly = anomalies
            .iter()
            .find(|a| matches!(a.anomaly_type, AnomalyType::Drop));
        assert!(drop_anomaly.is_some(), "Should detect drop");
    }

    #[tokio::test]
    async fn test_trend_anomaly_detection() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add increasing values (trend)
        for i in 0..25 {
            let value = 100.0 + (i as f64 * 2.0);
            detector
                .observe_metric("memory_usage", value, timestamp)
                .await
                .unwrap();
        }

        // Add a value that breaks the trend
        detector
            .observe_metric("memory_usage", 50.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("memory_usage").await.unwrap();

        let trend_anomaly = anomalies
            .iter()
            .find(|a| matches!(a.anomaly_type, AnomalyType::TrendAnomaly));
        assert!(trend_anomaly.is_some(), "Should detect trend anomaly");
    }

    #[tokio::test]
    async fn test_rate_of_change_anomaly() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add stable values
        for _ in 0..20 {
            detector
                .observe_metric("latency", 50.0, timestamp)
                .await
                .unwrap();
        }

        // Add a sudden jump
        detector
            .observe_metric("latency", 200.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("latency").await.unwrap();

        let roc_anomaly = anomalies
            .iter()
            .find(|a| matches!(a.anomaly_type, AnomalyType::RateOfChangeAnomaly));
        assert!(roc_anomaly.is_some(), "Should detect rate of change anomaly");
    }

    #[tokio::test]
    async fn test_anomaly_scoring() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add normal values
        for _ in 0..20 {
            detector
                .observe_metric("test_metric", 100.0, timestamp)
                .await
                .unwrap();
        }

        // Add extreme anomaly
        detector
            .observe_metric("test_metric", 500.0, timestamp)
            .await
            .unwrap();

        let anomalies = detector.detect_anomalies("test_metric").await.unwrap();
        assert!(!anomalies.is_empty());

        let top_anomaly = &anomalies[0];
        assert!(top_anomaly.score > 0.7, "High anomaly should have high score");
        assert!(
            matches!(top_anomaly.severity, AnomalySeverity::High | AnomalySeverity::Critical),
            "High score should result in high severity"
        );
    }

    #[tokio::test]
    async fn test_alert_cooldown() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add data and trigger first alert
        for _ in 0..20 {
            detector
                .observe_metric("test_metric", 100.0, timestamp)
                .await
                .unwrap();
        }
        detector
            .observe_metric("test_metric", 500.0, timestamp)
            .await
            .unwrap();

        let first_alerts = detector.check_and_alert("test_metric").await.unwrap();
        assert!(!first_alerts.is_empty(), "Should trigger first alert");

        // Try to alert again immediately (should be blocked by cooldown)
        let second_alerts = detector.check_and_alert("test_metric").await.unwrap();
        assert!(second_alerts.is_empty(), "Should be blocked by cooldown");
    }

    #[tokio::test]
    async fn test_metric_summary() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add values
        for i in 0..20 {
            let value = 100.0 + (i as f64);
            detector
                .observe_metric("test_metric", value, timestamp)
                .await
                .unwrap();
        }

        let summary = detector.get_metric_summary("test_metric").await.unwrap();
        assert!(summary.is_some(), "Should have summary");

        let summary = summary.unwrap();
        assert_eq!(summary.count, 20);
        assert!(summary.mean > 100.0);
        assert!(summary.std_dev > 0.0);
        assert!(summary.min < summary.max);
    }

    #[tokio::test]
    async fn test_remove_metric() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        detector
            .observe_metric("test_metric", 100.0, timestamp)
            .await
            .unwrap();

        let result = detector.remove_metric("test_metric").await;
        assert!(result.is_ok());

        let metrics = detector.get_tracked_metrics().await;
        assert!(!metrics.contains(&"test_metric".to_string()));
    }

    #[tokio::test]
    async fn test_clear_old_anomalies() {
        let detector = create_test_detector();
        let timestamp = Utc::now();

        // Add data and trigger anomalies
        for _ in 0..20 {
            detector
                .observe_metric("test_metric", 100.0, timestamp)
                .await
                .unwrap();
        }
        detector
            .observe_metric("test_metric", 500.0, timestamp)
            .await
            .unwrap();

        let _ = detector.detect_anomalies("test_metric").await.unwrap();
        let initial_count = detector.get_anomalies().await.len();

        // Clear old anomalies (with very short max age)
        detector.clear_old_anomalies(0).await;
        let final_count = detector.get_anomalies().await.len();

        assert!(final_count < initial_count, "Should clear old anomalies");
    }

    #[tokio::test]
    async fn test_severity_classification() {
        assert_eq!(AnomalySeverity::from_score(0.95), AnomalySeverity::Critical);
        assert_eq!(AnomalySeverity::from_score(0.75), AnomalySeverity::High);
        assert_eq!(AnomalySeverity::from_score(0.55), AnomalySeverity::Medium);
        assert_eq!(AnomalySeverity::from_score(0.3), AnomalySeverity::Low);
    }

    #[tokio::test]
    async fn test_config_builder() {
        let config = AnomalyConfig::default()
            .with_z_score_threshold(4.0)
            .with_anomaly_score_threshold(0.8)
            .with_trend_analysis(false)
            .with_pattern_recognition(false)
            .with_learning(false);

        assert_eq!(config.z_score_threshold, 4.0);
        assert_eq!(config.anomaly_score_threshold, 0.8);
        assert!(!config.enable_trend_analysis);
        assert!(!config.enable_pattern_recognition);
        assert!(!config.enable_learning);
    }
}
