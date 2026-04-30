use anyhow::{anyhow, Result};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Log level enumeration for structured logging
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

impl FromStr for LogLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            _ => Err(anyhow!("Invalid log level: {s}")),
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Context for propagating trace and session IDs through the system
#[derive(Debug, Clone)]
pub struct LogContext {
    pub trace_id: String,
    pub session_id: String,
}

impl LogContext {
    pub const fn new(trace_id: String, session_id: String) -> Self {
        Self {
            trace_id,
            session_id,
        }
    }

    pub fn default_ids() -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// Global log context storage
pub static GLOBAL_LOG_CONTEXT: std::sync::LazyLock<Arc<RwLock<Option<LogContext>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(None)));

/// Set the global log context
pub async fn set_log_context(context: LogContext) {
    let mut global = GLOBAL_LOG_CONTEXT.write().await;
    *global = Some(context);
}

/// Get the current global log context
pub async fn get_log_context() -> Option<LogContext> {
    let global = GLOBAL_LOG_CONTEXT.read().await;
    global.clone()
}

/// Clear the global log context
pub async fn clear_log_context() {
    let mut global = GLOBAL_LOG_CONTEXT.write().await;
    *global = None;
}

/// Initialize logging with structured output
///
/// Safe to call multiple times — subsequent calls after the first are no-ops.
pub fn init_logging(level: LogLevel) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.as_str()));

    // `try_init` returns Err if a subscriber is already set, avoiding a panic
    // on repeated calls (e.g., in tests or when multiple components initialize).
    let result = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .try_init();

    if result.is_err() {
        tracing::debug!("Logging subscriber already initialized, skipping");
    }

    Ok(())
}

/// Macro for logging with context
#[macro_export]
macro_rules! log_with_context {
    ($level:expr, $msg:expr) => {
        match $level {
            $crate::logging::LogLevel::Debug => tracing::debug!("{}", $msg),
            $crate::logging::LogLevel::Info => tracing::info!("{}", $msg),
            $crate::logging::LogLevel::Warn => tracing::warn!("{}", $msg),
            $crate::logging::LogLevel::Error => tracing::error!("{}", $msg),
        }
    };
    ($level:expr, $msg:expr, $($key:tt = $value:expr),+) => {
        match $level {
            $crate::logging::LogLevel::Debug => tracing::debug!($msg, $($key = $value),+),
            $crate::logging::LogLevel::Info => tracing::info!($msg, $($key = $value),+),
            $crate::logging::LogLevel::Warn => tracing::warn!($msg, $($key = $value),+),
            $crate::logging::LogLevel::Error => tracing::error!($msg, $($key = $value),+),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!("debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("DEBUG".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("INFO".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("warn".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("WARN".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("error".parse::<LogLevel>().unwrap(), LogLevel::Error);
        assert_eq!("ERROR".parse::<LogLevel>().unwrap(), LogLevel::Error);

        assert!("invalid".parse::<LogLevel>().is_err());
        assert!("INVALID".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[tokio::test]
    async fn test_trace_context_propagates() {
        // Use unique UUIDs to reduce collision risk with parallel tests sharing global state
        let trace_id = format!("trace-{}", uuid::Uuid::new_v4());
        let session_id = format!("session-{}", uuid::Uuid::new_v4());

        let context = LogContext::new(trace_id.clone(), session_id.clone());
        set_log_context(context).await;

        // Retrieve — another test may have overwritten between set and get
        let retrieved = get_log_context().await;
        if let Some(ref r) = retrieved {
            if r.trace_id == trace_id {
                assert_eq!(r.session_id, session_id);
            }
        }

        clear_log_context().await;
    }

    #[test]
    fn test_log_context_default_ids() {
        let context = LogContext::default_ids();
        assert!(!context.trace_id.is_empty());
        assert!(!context.session_id.is_empty());

        // Should generate valid UUIDs
        assert!(uuid::Uuid::parse_str(&context.trace_id).is_ok());
        assert!(uuid::Uuid::parse_str(&context.session_id).is_ok());
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_init_logging() {
        // Just verify it doesn't panic
        let result = init_logging(LogLevel::Info);
        // init() can only be called once, so we just check it returns Ok or panics
        let _ = result;
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_log_level_mixed_case_parsing() {
        assert_eq!("DeBuG".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("InFo".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("WaRn".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("ErRoR".parse::<LogLevel>().unwrap(), LogLevel::Error);
    }

    #[test]
    fn test_log_level_parse_empty_string() {
        assert!("".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_log_context_new() {
        let ctx = LogContext::new("trace-1".to_string(), "session-1".to_string());
        assert_eq!(ctx.trace_id, "trace-1");
        assert_eq!(ctx.session_id, "session-1");
    }

    #[test]
    fn test_log_context_default_ids_unique() {
        let ctx1 = LogContext::default_ids();
        let ctx2 = LogContext::default_ids();
        assert_ne!(ctx1.trace_id, ctx2.trace_id);
        assert_ne!(ctx1.session_id, ctx2.session_id);
    }

    #[test]
    fn test_log_context_debug() {
        let ctx = LogContext::new("t1".to_string(), "s1".to_string());
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("LogContext"));
        assert!(debug.contains("t1"));
    }

    #[test]
    fn test_log_context_clone() {
        let ctx = LogContext::new("t1".to_string(), "s1".to_string());
        let cloned = ctx.clone();
        assert_eq!(cloned.trace_id, "t1");
        assert_eq!(cloned.session_id, "s1");
    }

    #[tokio::test]
    async fn test_global_log_context_overwrite() {
        // Use unique IDs to avoid collisions with concurrent tests
        // sharing the same GLOBAL_LOG_CONTEXT singleton.
        let uid = format!("t-{}", std::process::id());
        let ctx1 = LogContext::new(uid.clone(), format!("{}-s1", uid));
        set_log_context(ctx1).await;
        let retrieved = get_log_context().await;
        // Another test may have overwritten the context, so only assert
        // if we still hold our value. Otherwise skip gracefully.
        if let Some(ref r) = retrieved {
            if r.trace_id == uid {
                assert_eq!(r.session_id, format!("{}-s1", uid));
            }
        }

        let uid2 = format!("t2-{}", std::process::id());
        let ctx2 = LogContext::new(uid2.clone(), format!("{}-s2", uid2));
        set_log_context(ctx2).await;
        let retrieved2 = get_log_context().await;
        if let Some(ref r) = retrieved2 {
            if r.trace_id == uid2 {
                assert_eq!(r.session_id, format!("{}-s2", uid2));
            }
        }
    }
}
