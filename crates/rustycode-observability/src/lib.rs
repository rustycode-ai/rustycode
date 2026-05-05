#![allow(
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::manual_string_new,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::uninlined_format_args
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::float_cmp,))]

pub mod context;
pub mod diagnostics;
pub mod logging;
pub mod metrics;
pub mod metrics_store;
pub mod rule_tracer;

// Re-export primary types
pub use context::{create_context, ExecutionContext, SharedContext};
pub use diagnostics::{CheckStatus, DiagnosticCheck, DiagnosticReport, DiagnosticSuite};
pub use logging::{
    clear_log_context, log_context, init_logging, set_log_context, LogContext, LogLevel,
    GLOBAL_LOG_CONTEXT,
};
pub use metrics::{Counter, Gauge, Histogram, HistogramStats, SessionMetrics};
pub use metrics_store::MetricsStore;
pub use rule_tracer::{RuleTracer, TraceEntry, TraceLevel};
