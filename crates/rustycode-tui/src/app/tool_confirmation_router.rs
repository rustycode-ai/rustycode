//! Async tool confirmation router using oneshot channels.
//!
//! Ported from goose's `ToolConfirmationRouter` pattern. Provides a clean async
//! interface for tool approval: tool execution awaits confirmation while the TUI
//! delivers user decisions through oneshot channels.
//!
//! # Example
//!
//! ```ignore
//! let router = ToolConfirmationRouter::new();
//!
//! // Tool execution side (async)
//! let rx = router.register("tool_req_123".to_string()).await;
//! let confirmation = rx.await?; // blocks until user decides
//!
//! // TUI side (event handler)
//! router.deliver("tool_req_123".to_string(), ToolConfirmation::approved()).await;
//! ```

use std::collections::HashMap;
use tokio::sync::{oneshot, Mutex};
use tracing::warn;

/// User's decision on a tool approval request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolConfirmation {
    /// Approve this single execution.
    AllowOnce,
    /// Approve all future executions of this tool in the session.
    AlwaysAllow,
    /// Deny this single execution.
    DenyOnce,
    /// Deny all future executions of this tool in the session.
    AlwaysDeny,
}

impl ToolConfirmation {
    pub fn approved() -> Self {
        Self::AllowOnce
    }

    pub fn denied() -> Self {
        Self::DenyOnce
    }

    pub fn is_approved(&self) -> bool {
        matches!(self, Self::AllowOnce | Self::AlwaysAllow)
    }

    pub fn is_session_wide(&self) -> bool {
        matches!(self, Self::AlwaysAllow | Self::AlwaysDeny)
    }
}

/// Routes tool confirmation decisions between async tool execution and the TUI.
///
/// Uses oneshot channels so tool execution can `.await` user decisions without
/// polling. Stale entries (where the receiver was dropped due to task cancellation)
/// are automatically pruned on the next `register()` call.
pub struct ToolConfirmationRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<ToolConfirmation>>>,
}

impl ToolConfirmationRouter {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending approval request.
    ///
    /// Returns a `oneshot::Receiver` that will receive the user's decision.
    /// Stale entries (cancelled receivers) are pruned automatically.
    pub async fn register(&self, request_id: String) -> oneshot::Receiver<ToolConfirmation> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        // Prune stale entries (receivers that were dropped due to task cancellation)
        pending.retain(|_, sender| !sender.is_closed());
        pending.insert(request_id, tx);
        rx
    }

    /// Deliver a confirmation decision for a pending request.
    ///
    /// Returns `true` if the decision was delivered successfully,
    /// `false` if no one was waiting (task cancelled or unknown ID).
    pub async fn deliver(&self, request_id: String, confirmation: ToolConfirmation) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(&request_id) {
            if tx.send(confirmation).is_err() {
                warn!(
                    request_id = %request_id,
                    "Confirmation receiver was dropped (task cancelled)"
                );
                false
            } else {
                true
            }
        } else {
            warn!(
                request_id = %request_id,
                "No task waiting for confirmation"
            );
            false
        }
    }

    /// Get the number of pending approval requests.
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}

impl Default for ToolConfirmationRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_then_deliver() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;
        assert!(
            router
                .deliver("req_1".to_string(), ToolConfirmation::AllowOnce)
                .await
        );
        let confirmation = rx.await.unwrap();
        assert_eq!(confirmation, ToolConfirmation::AllowOnce);
    }

    #[tokio::test]
    async fn test_deliver_unknown_request() {
        let router = ToolConfirmationRouter::new();
        assert!(
            !router
                .deliver("unknown".to_string(), ToolConfirmation::AllowOnce)
                .await
        );
    }

    #[tokio::test]
    async fn test_cancelled_receiver() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;
        drop(rx); // simulate task cancellation
        assert!(
            !router
                .deliver("req_1".to_string(), ToolConfirmation::AllowOnce)
                .await
        );
    }

    #[tokio::test]
    async fn test_stale_entries_pruned_on_register() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;
        drop(rx); // stale entry

        assert_eq!(router.pending.lock().await.len(), 1);

        let _rx2 = router.register("req_2".to_string()).await;
        // req_1 pruned, only req_2 remains
        assert_eq!(router.pending.lock().await.len(), 1);
        assert!(router.pending.lock().await.contains_key("req_2"));
    }

    #[tokio::test]
    async fn test_concurrent_requests_out_of_order() {
        use std::sync::Arc;

        let router = Arc::new(ToolConfirmationRouter::new());

        let rx1 = router.register("req_1".to_string()).await;
        let rx2 = router.register("req_2".to_string()).await;

        // Deliver in reverse order
        assert!(
            router
                .deliver("req_2".to_string(), ToolConfirmation::DenyOnce)
                .await
        );
        assert_eq!(router.pending.lock().await.len(), 1);
        assert!(
            router
                .deliver("req_1".to_string(), ToolConfirmation::AlwaysAllow)
                .await
        );
        assert_eq!(router.pending.lock().await.len(), 0);

        let c1 = rx1.await.unwrap();
        assert_eq!(c1, ToolConfirmation::AlwaysAllow);
        let c2 = rx2.await.unwrap();
        assert_eq!(c2, ToolConfirmation::DenyOnce);
    }

    #[tokio::test]
    async fn test_confirmation_helpers() {
        assert!(ToolConfirmation::approved().is_approved());
        assert!(!ToolConfirmation::denied().is_approved());
        assert!(ToolConfirmation::AlwaysAllow.is_session_wide());
        assert!(ToolConfirmation::AlwaysDeny.is_session_wide());
        assert!(!ToolConfirmation::AllowOnce.is_session_wide());
    }

    #[tokio::test]
    async fn test_pending_count() {
        let router = ToolConfirmationRouter::new();
        assert_eq!(router.pending_count().await, 0);

        let _rx1 = router.register("req_1".to_string()).await;
        assert_eq!(router.pending_count().await, 1);

        let _rx2 = router.register("req_2".to_string()).await;
        assert_eq!(router.pending_count().await, 2);

        router
            .deliver("req_1".to_string(), ToolConfirmation::AllowOnce)
            .await;
        assert_eq!(router.pending_count().await, 1);
    }
}
