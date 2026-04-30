//! Rate-Limited Telemetry Sender
//!
//! Prevents telemetry spam by batching and throttling telemetry events.
//! Events are sent through an unbounded channel and processed at a
//! configurable rate (default 400ms between sends).
//!
//! If the internal queue exceeds `MAX_QUEUE_LEN`, new events are dropped
//! to prevent unbounded memory growth under sustained load.
//!
//! Inspired by goose's `tracing/rate_limiter.rs`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::telemetry_limiter::{RateLimitedTelemetry, TelemetryEvent, SpanData};
//!
//! let sender = RateLimitedTelemetry::new(400);
//!
//! sender.send_span(SpanData {
//!     name: "tool_execution".to_string(),
//!     attributes: vec![("tool".to_string(), "bash".to_string())],
//!     duration: Some(Duration::from_millis(150)),
//! }).ok();
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// A telemetry span representing a timed operation.
#[derive(Debug, Clone)]
pub struct SpanData {
    /// Name of the span (e.g., "tool_execution", "llm_request")
    pub name: String,
    /// Key-value attributes attached to the span
    pub attributes: Vec<(String, String)>,
    /// Duration of the operation, if available
    pub duration: Option<Duration>,
}

/// A telemetry metric representing a numeric measurement.
#[derive(Debug, Clone)]
pub struct MetricData {
    /// Name of the metric (e.g., "tokens_used", "request_latency_ms")
    pub name: String,
    /// Numeric value
    pub value: f64,
    /// Key-value labels for dimensional metrics
    pub labels: Vec<(String, String)>,
}

/// Types of telemetry events that can be sent.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TelemetryEvent {
    /// A timed operation span
    Span(SpanData),
    /// A numeric metric measurement
    Metric(MetricData),
}

/// Maximum events queued before new events are dropped.
/// Prevents unbounded memory growth when events arrive faster than the
/// rate limiter can process them.
const MAX_QUEUE_LEN: usize = 1000;

/// Rate-limited telemetry sender.
///
/// Events are queued and processed at a maximum rate, preventing
/// telemetry from overwhelming the logging system or external services.
pub struct RateLimitedTelemetry {
    sender: mpsc::UnboundedSender<TelemetryEvent>,
    /// Number of events that were dropped due to queue overflow
    dropped: Arc<std::sync::atomic::AtomicU64>,
    /// Current queue depth (approximate, used for backpressure)
    queue_depth: Arc<std::sync::atomic::AtomicUsize>,
}

impl RateLimitedTelemetry {
    /// Create a new rate-limited telemetry sender.
    ///
    /// `rate_limit_ms` is the minimum time between processing consecutive
    /// events. Events arriving faster than this rate are still queued but
    /// processed with the specified delay between them.
    pub fn new(rate_limit_ms: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel::<TelemetryEvent>();
        let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let queue_depth = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let dropped_clone = dropped.clone();
        let queue_depth_clone = queue_depth.clone();
        tokio::spawn(async move {
            Self::process_events(receiver, rate_limit_ms, dropped_clone, queue_depth_clone).await;
        });

        Self {
            sender,
            dropped,
            queue_depth,
        }
    }

    /// Send a span telemetry event.
    /// Drops the event silently if the internal queue is at capacity.
    pub fn send_span(&self, span: SpanData) -> Result<(), mpsc::error::SendError<TelemetryEvent>> {
        self.send_event(TelemetryEvent::Span(span))
    }

    /// Send a metric telemetry event.
    /// Drops the event silently if the internal queue is at capacity.
    pub fn send_metric(
        &self,
        metric: MetricData,
    ) -> Result<(), mpsc::error::SendError<TelemetryEvent>> {
        self.send_event(TelemetryEvent::Metric(metric))
    }

    fn send_event(
        &self,
        event: TelemetryEvent,
    ) -> Result<(), mpsc::error::SendError<TelemetryEvent>> {
        let depth = self.queue_depth.load(std::sync::atomic::Ordering::Relaxed);
        if depth >= MAX_QUEUE_LEN {
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(()); // Drop the event to prevent OOM
        }
        self.queue_depth
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sender.send(event)
    }

    /// Get the number of events that were rate-limited (queued during throttle).
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if the sender channel is still open.
    pub fn is_open(&self) -> bool {
        !self.sender.is_closed()
    }

    async fn process_events(
        mut receiver: mpsc::UnboundedReceiver<TelemetryEvent>,
        rate_limit_ms: u64,
        dropped: Arc<std::sync::atomic::AtomicU64>,
        queue_depth: Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let rate_limit_duration = Duration::from_millis(rate_limit_ms);
        let mut last_send = Instant::now()
            .checked_sub(rate_limit_duration)
            .unwrap_or_else(Instant::now);

        while let Some(event) = receiver.recv().await {
            // Decrement queue depth now that we're processing this event
            queue_depth.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            let elapsed = last_send.elapsed();
            if elapsed < rate_limit_duration {
                let sleep_duration = rate_limit_duration - elapsed;
                dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tokio::time::sleep(sleep_duration).await;
            }

            match &event {
                TelemetryEvent::Span(span) => {
                    tracing::debug!(
                        name = %span.name,
                        attributes = ?span.attributes,
                        duration_ms = span.duration.map(|d| d.as_millis() as u64),
                        "telemetry_span"
                    );
                }
                TelemetryEvent::Metric(metric) => {
                    tracing::debug!(
                        name = %metric.name,
                        value = metric.value,
                        labels = ?metric.labels,
                        "telemetry_metric"
                    );
                }
            }

            last_send = Instant::now();
        }

        tracing::debug!("Rate-limited telemetry sender shutting down");
    }
}

impl Default for RateLimitedTelemetry {
    fn default() -> Self {
        Self::new(400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_span_success() {
        let sender = RateLimitedTelemetry::new(100);
        let result = sender.send_span(SpanData {
            name: "test".to_string(),
            attributes: vec![],
            duration: None,
        });
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_metric_success() {
        let sender = RateLimitedTelemetry::new(100);
        let result = sender.send_metric(MetricData {
            name: "test_metric".to_string(),
            value: 42.0,
            labels: vec![],
        });
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_open_after_creation() {
        let sender = RateLimitedTelemetry::new(100);
        assert!(sender.is_open());
    }

    #[tokio::test]
    async fn test_default_rate_limit() {
        let sender = RateLimitedTelemetry::default();
        assert!(sender.is_open());
    }

    #[tokio::test]
    async fn test_dropped_count_initially_zero() {
        let sender = RateLimitedTelemetry::new(100);
        assert_eq!(sender.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_multiple_spans_are_queued() {
        let sender = RateLimitedTelemetry::new(50);

        for i in 0..5 {
            sender
                .send_span(SpanData {
                    name: format!("span_{}", i),
                    attributes: vec![],
                    duration: None,
                })
                .unwrap();
        }

        // Give the processor time to handle events
        tokio::time::sleep(Duration::from_millis(300)).await;

        // All should be processed (not dropped, just delayed)
        assert!(sender.is_open());
    }

    #[test]
    fn test_span_data_debug() {
        let span = SpanData {
            name: "test".to_string(),
            attributes: vec![("key".to_string(), "value".to_string())],
            duration: Some(Duration::from_millis(100)),
        };
        let debug_str = format!("{:?}", span);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("key"));
    }

    #[test]
    fn test_metric_data_debug() {
        let metric = MetricData {
            name: "latency".to_string(),
            value: 42.5,
            labels: vec![("endpoint".to_string(), "/api/v1".to_string())],
        };
        let debug_str = format!("{:?}", metric);
        assert!(debug_str.contains("latency"));
        assert!(debug_str.contains("42.5"));
    }

    #[test]
    fn test_telemetry_event_variants() {
        let span_event = TelemetryEvent::Span(SpanData {
            name: "test".to_string(),
            attributes: vec![],
            duration: None,
        });
        let metric_event = TelemetryEvent::Metric(MetricData {
            name: "test".to_string(),
            value: 1.0,
            labels: vec![],
        });

        match span_event {
            TelemetryEvent::Span(s) => assert_eq!(s.name, "test"),
            TelemetryEvent::Metric(_) => panic!("Expected Span"),
        }

        match metric_event {
            TelemetryEvent::Metric(m) => assert_eq!(m.name, "test"),
            TelemetryEvent::Span(_) => panic!("Expected Metric"),
        }
    }
}
