//! Telemetry module - Observability and event tracking

pub mod lifecycle;
pub mod limiter;
pub mod observation;
pub mod streaming;

// Re-export key types for backward compatibility
pub use lifecycle::{
    CompositeHandler, EndPayload, HookResult, LifecycleEvent, LifecycleHandler,
    LifecycleHookResult, LifecycleHooks, NoOpHandler, RequestPayload, ResponsePayload,
    StartPayload, ToolCallEndPayload, ToolCallStartPayload, TracingHandler, UsagePayload,
};
pub use limiter::{MetricData, RateLimitedTelemetry, TelemetryEvent};
pub use observation::{
    flatten_metadata, ConsoleBatchManager, InMemoryBatchManager, ObservationLayer, SpanData,
    SpanTracker,
};
pub use streaming::{create_stream_channel, StreamChunk, StreamReceiver, StreamSender};
