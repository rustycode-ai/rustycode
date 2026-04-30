//! Graceful Shutdown Signal Handling
//!
//! Provides cross-platform shutdown signal handling for async applications.
//! On Unix, listens for both Ctrl+C (SIGINT) and SIGTERM. On Windows,
//! only Ctrl+C is available.
//!
//! Inspired by goose's `signal.rs` in `goose-cli`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::shutdown::{shutdown_signal, ShutdownGuard};
//!
//! // Use with tokio::select! for graceful shutdown
//! tokio::select! {
//!     result = do_work() => result,
//!     _ = shutdown_signal() => {
//!         println!("Shutting down gracefully...");
//!     }
//! }
//!
//! // Or use the guard for scoped cancellation
//! let guard = ShutdownGuard::new();
//! tokio::select! {
//!     _ = guard.wait() => println!("Cancelled"),
//!     _ = tokio::time::sleep(Duration::from_secs(60)) => println!("Done"),
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Returns a future that resolves when a shutdown signal is received.
///
/// On Unix, listens for both SIGINT (Ctrl+C) and SIGTERM.
/// On Windows, only SIGINT (Ctrl+C) is supported.
///
/// This is designed to be used with `tokio::select!` for graceful shutdown:
///
/// ```ignore
/// tokio::select! {
///     result = server.run() => result,
///     _ = shutdown_signal() => {
///         println!("Shutting down...");
///     }
/// }
/// ```
pub fn shutdown_signal() -> Pin<Box<dyn Future<Output = ()> + Send>> {
    #[cfg(unix)]
    {
        Box::pin(async move {
            let ctrl_c = async {
                tokio::signal::ctrl_c()
                    .await
                    .expect("failed to install Ctrl+C handler");
            };

            let terminate = async {
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to install SIGTERM handler")
                    .recv()
                    .await;
            };

            tokio::select! {
                _ = ctrl_c => {},
                _ = terminate => {},
            }
        })
    }

    #[cfg(not(unix))]
    {
        Box::pin(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        })
    }
}

/// A guard that tracks whether a shutdown signal has been received.
///
/// Useful for propagating cancellation to multiple tasks without
/// passing around a `CancellationToken`.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::shutdown::ShutdownGuard;
///
/// let guard = ShutdownGuard::new();
/// let guard_clone = guard.clone();
///
/// // In one task: wait for shutdown
/// tokio::spawn(async move {
///     guard_clone.wait().await;
///     println!("Shutting down!");
/// });
///
/// // In another task: trigger shutdown
/// guard.trigger();
/// ```
#[derive(Clone)]
pub struct ShutdownGuard {
    triggered: Arc<AtomicBool>,
}

impl ShutdownGuard {
    /// Create a new shutdown guard (not yet triggered).
    pub fn new() -> Self {
        Self {
            triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if shutdown has been triggered.
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// Trigger the shutdown signal.
    pub fn trigger(&self) {
        self.triggered.store(true, Ordering::SeqCst);
    }

    /// Wait until the shutdown signal is triggered.
    ///
    /// This polls the flag with a small sleep interval. For production use,
    /// prefer `tokio::select!` with `shutdown_signal()` for immediate response.
    pub async fn wait(&self) {
        while !self.triggered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

impl Default for ShutdownGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_guard_new_not_triggered() {
        let guard = ShutdownGuard::new();
        assert!(!guard.is_triggered());
    }

    #[test]
    fn test_shutdown_guard_trigger() {
        let guard = ShutdownGuard::new();
        guard.trigger();
        assert!(guard.is_triggered());
    }

    #[test]
    fn test_shutdown_guard_clone_shares_state() {
        let guard = ShutdownGuard::new();
        let clone = guard.clone();
        clone.trigger();
        assert!(guard.is_triggered());
        assert!(clone.is_triggered());
    }

    #[test]
    fn test_shutdown_guard_default() {
        let guard = ShutdownGuard::default();
        assert!(!guard.is_triggered());
    }

    #[tokio::test]
    async fn test_shutdown_guard_wait_with_trigger() {
        let guard = ShutdownGuard::new();
        let guard_clone = guard.clone();

        // Trigger in a separate task after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            guard_clone.trigger();
        });

        // Wait should return once triggered
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), guard.wait()).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_signal_is_send() {
        fn assert_send<T: Send>(_t: T) {}
        let signal = shutdown_signal();
        assert_send(signal);
    }
}
