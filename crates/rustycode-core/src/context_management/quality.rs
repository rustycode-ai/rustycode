// ── Quality Metrics ────────────────────────────────────────────────────────────

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Trend indicator for quality metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum QualityTrend {
    /// Quality is improving
    Improving,
    /// Quality is stable
    Stable,
    /// Quality is declining
    Declining,
}

/// Tracks context quality metrics over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Total assemblies performed
    pub total_assemblies: usize,
    /// Total compressions performed
    pub total_compressions: usize,
    /// Total tokens saved through compression
    pub total_tokens_saved: usize,
    /// Average context quality (0.0 to 1.0)
    pub average_quality: f64,
    /// Quality trend (improving, stable, declining)
    pub quality_trend: QualityTrend,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

impl QualityMetrics {
    /// Create new quality metrics.
    pub fn new() -> Self {
        Self {
            total_assemblies: 0,
            total_compressions: 0,
            total_tokens_saved: 0,
            average_quality: 1.0,
            quality_trend: QualityTrend::Stable,
            last_updated: Utc::now(),
        }
    }

    /// Update metrics with a new assembly.
    pub fn record_assembly(&mut self, quality: f64) {
        self.total_assemblies = self.total_assemblies.saturating_add(1);

        // Update moving average
        let alpha = 0.1;
        self.average_quality = alpha * quality + (1.0 - alpha) * self.average_quality;

        self.update_trend_with(quality);
        self.last_updated = Utc::now();
    }

    /// Update metrics with a new compression.
    pub fn record_compression(&mut self, tokens_saved: usize) {
        self.total_compressions = self.total_compressions.saturating_add(1);
        self.total_tokens_saved = self.total_tokens_saved.saturating_add(tokens_saved);
        self.last_updated = Utc::now();
    }

    /// Update quality trend based on the most recent quality value.
    fn update_trend_with(&mut self, current_quality: f64) {
        // Detect trend from current quality input directly
        if current_quality > 0.8 {
            self.quality_trend = QualityTrend::Improving;
        } else if current_quality < 0.4 {
            self.quality_trend = QualityTrend::Declining;
        } else {
            self.quality_trend = QualityTrend::Stable;
        }
    }

    /// Get token savings rate (tokens saved per compression).
    pub fn savings_rate(&self) -> f64 {
        if self.total_compressions == 0 {
            0.0
        } else {
            self.total_tokens_saved as f64 / self.total_compressions as f64
        }
    }
}

impl Default for QualityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_metrics_creation() {
        let metrics = QualityMetrics::new();
        assert_eq!(metrics.total_assemblies, 0);
        assert_eq!(metrics.total_compressions, 0);
        assert_eq!(metrics.average_quality, 1.0);
    }

    #[test]
    fn test_quality_metrics_record_assembly() {
        let mut metrics = QualityMetrics::new();
        metrics.record_assembly(0.8);

        assert_eq!(metrics.total_assemblies, 1);
        assert!(metrics.average_quality < 1.0); // Should change
    }

    #[test]
    fn test_quality_metrics_record_compression() {
        let mut metrics = QualityMetrics::new();
        metrics.record_compression(1000);

        assert_eq!(metrics.total_compressions, 1);
        assert_eq!(metrics.total_tokens_saved, 1000);
    }

    #[test]
    fn test_quality_metrics_savings_rate() {
        let mut metrics = QualityMetrics::new();
        metrics.record_compression(1000);
        metrics.record_compression(500);

        let rate = metrics.savings_rate();
        assert_eq!(rate, 750.0); // (1000 + 500) / 2
    }

    #[test]
    fn test_quality_metrics_trend() {
        let mut metrics = QualityMetrics::new();

        // High quality assemblies
        metrics.record_assembly(0.9);
        metrics.record_assembly(0.95);

        assert_eq!(metrics.quality_trend, QualityTrend::Improving);

        // Many low quality assemblies to overcome exponential moving average
        for _ in 0..10 {
            metrics.record_assembly(0.1);
        }

        assert_eq!(metrics.quality_trend, QualityTrend::Declining);
    }
}
