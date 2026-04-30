//! Permission system for tool execution
//!
//! This module provides:
//! - Permission request creation
//! - Risk level detection
//! - Interactive permission prompts
//! - Permission scope management

use serde::{Deserialize, Serialize};

/// Permission request for tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Operation being requested (e.g., "read", "write", "execute")
    pub operation: String,
    /// Path being accessed
    pub path: String,
    /// Reason for the request
    pub reason: String,
    /// Scope of the permission
    pub scope: PermissionScope,
    /// Risk level of the operation
    pub risk: RiskLevel,
}

impl PermissionRequest {
    /// Create a new permission request
    pub fn new(
        operation: impl Into<String>,
        path: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let operation = operation.into();
        let path = path.into();
        let reason = reason.into();
        let risk = Self::detect_risk(&operation, std::path::Path::new(&path));
        Self {
            operation,
            path,
            reason,
            scope: PermissionScope::OneTime,
            risk,
        }
    }

    /// Detect risk level based on operation and path
    fn detect_risk(operation: &str, path: &std::path::Path) -> RiskLevel {
        let is_write = matches!(operation, "write" | "delete" | "execute" | "modify");
        let path_str = path.to_string_lossy();
        let is_sensitive = path_str.contains("/etc")
            || path_str.contains(".ssh")
            || path_str.contains("password")
            || path_str.contains("secret")
            || path_str.contains("key");

        match (is_write, is_sensitive) {
            (true, true) => RiskLevel::High,
            (true, false) => RiskLevel::Medium,
            (false, true) => RiskLevel::Medium,
            (false, false) => RiskLevel::Low,
        }
    }

    /// Set the permission scope
    pub fn with_scope(mut self, scope: PermissionScope) -> Self {
        self.scope = scope;
        self
    }

    /// Format a permission prompt for this request
    pub fn format_prompt(&self) -> String {
        let emoji = match self.risk {
            RiskLevel::Low => "📁",
            RiskLevel::Medium => "⚠️",
            RiskLevel::High => "🔒",
        };

        format!(
            "{} {} operation: {}\n  Path: {}\n  Reason: {}\n  Risk: {:?}",
            emoji, self.operation, self.reason, self.path, self.risk, self.scope
        )
    }
}

/// Scope of a permission grant
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum PermissionScope {
    /// One-time grant for this operation only
    OneTime,
    /// Grant for this session
    Session,
    /// Grant permanently
    Permanent,
}

/// Risk level of an operation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Medium => write!(f, "Medium"),
            RiskLevel::High => write!(f, "High"),
        }
    }
}

/// A stored approval rule with wildcard pattern support.
///
/// Patterns follow glob-style matching:
/// - `*` matches any sequence of characters
/// - `prefix*` matches anything starting with `prefix`
/// - `*suffix` matches anything ending with `suffix`
/// - `*middle*` matches anything containing `middle`
/// - literal strings match exactly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    /// Glob pattern for the path (e.g., "src/**/*.rs", "*.toml")
    pub path_pattern: String,
    /// Operation this covers ("read", "write", "execute", or "*" for any)
    pub operation: String,
    /// When this approval was granted
    pub approved_at: chrono::DateTime<chrono::Utc>,
}

impl PartialEq for ApprovalRule {
    fn eq(&self, other: &Self) -> bool {
        // Deduplicate by (path_pattern, operation), ignoring timestamp
        self.path_pattern == other.path_pattern && self.operation == other.operation
    }
}

impl Eq for ApprovalRule {}

impl std::hash::Hash for ApprovalRule {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path_pattern.hash(state);
        self.operation.hash(state);
    }
}

impl ApprovalRule {
    /// Create a new approval rule
    pub fn new(path_pattern: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            path_pattern: path_pattern.into(),
            operation: operation.into(),
            approved_at: chrono::Utc::now(),
        }
    }

    /// Check if a given (path, operation) pair matches this rule.
    pub fn matches(&self, path: &str, operation: &str) -> bool {
        // Operation check: exact match or wildcard
        if self.operation != "*" && self.operation != operation {
            return false;
        }

        wildcard_match(&self.path_pattern, path)
    }
}

/// Simple wildcard pattern matching.
///
/// Supports:
/// - `*` → matches any sequence of characters (including `/`)
/// - literal characters match themselves
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let pl = p.len();
    let tl = t.len();

    // DP table: dp[i][j] = pattern[0..i] matches text[0..j]
    let mut dp = vec![vec![false; tl + 1]; pl + 1];
    dp[0][0] = true;

    // Leading '*' can match empty string
    for i in 1..=pl {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pl {
        for j in 1..=tl {
            if p[i - 1] == '*' {
                // '*' matches zero chars (dp[i-1][j]) or extends match (dp[i][j-1])
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == t[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[pl][tl]
}

/// Permission manager for handling permission requests with wildcard approvals.
pub struct PermissionManager {
    /// Interactive mode (prompts user)
    interactive: bool,
    /// Auto-approve low-risk operations
    auto_approve_low: bool,
    /// Session-persistent approval rules
    approvals: parking_lot::Mutex<Vec<ApprovalRule>>,
}

impl PermissionManager {
    /// Create a new permission manager
    pub fn new(interactive: bool, auto_approve_low: bool) -> Self {
        Self {
            interactive,
            auto_approve_low,
            approvals: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// Check if a (path, operation) pair is pre-approved by any stored rule.
    pub fn is_pre_approved(&self, path: &str, operation: &str) -> bool {
        let approvals = self.approvals.lock();
        approvals.iter().any(|rule| rule.matches(path, operation))
    }

    /// Store an approval rule for the session.
    pub fn approve(&self, path_pattern: impl Into<String>, operation: impl Into<String>) {
        let rule = ApprovalRule::new(path_pattern, operation);
        let mut approvals = self.approvals.lock();
        // Avoid duplicates
        if !approvals.contains(&rule) {
            approvals.push(rule);
        }
    }

    /// Remove all approval rules matching the given pattern.
    pub fn revoke(&self, path_pattern: &str) {
        let mut approvals = self.approvals.lock();
        approvals.retain(|r| r.path_pattern != path_pattern);
    }

    /// List all active approval rules.
    pub fn list_approvals(&self) -> Vec<ApprovalRule> {
        self.approvals.lock().clone()
    }

    /// Request permission for an operation.
    ///
    /// Checks pre-approved rules first, then falls back to risk-based logic.
    pub async fn request_permission(&self, request: &PermissionRequest) -> Result<bool, String> {
        // 1. Check session-persistent approvals
        if self.is_pre_approved(&request.path, &request.operation) {
            return Ok(true);
        }

        // 2. Auto-approve low-risk operations if enabled
        if self.auto_approve_low && request.risk == RiskLevel::Low {
            return Ok(true);
        }

        // 3. In non-interactive mode, approve medium/low risk
        if !self.interactive {
            return Ok(matches!(request.risk, RiskLevel::Low | RiskLevel::Medium));
        }

        // 4. Interactive mode - deny high-risk by default
        Ok(request.risk != RiskLevel::High)
    }

    /// Format a permission prompt
    pub fn format_prompt(&self, request: &PermissionRequest) -> String {
        let emoji = match request.risk {
            RiskLevel::Low => "📁",
            RiskLevel::Medium => "⚠️",
            RiskLevel::High => "🔒",
        };

        format!(
            "{} {} operation: {}\n  Path: {}\n  Reason: {}",
            emoji, request.operation, request.reason, request.path, request.risk
        )
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new(true, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_request_creation() {
        let request = PermissionRequest::new("read", "/project/src/main.rs", "View code");
        assert_eq!(request.operation, "read");
        assert_eq!(request.path, "/project/src/main.rs");
        assert_eq!(request.reason, "View code");
    }

    #[test]
    fn test_risk_detection() {
        // Low risk: read normal file
        let request = PermissionRequest::new("read", "/project/src/main.rs", "View code");
        assert_eq!(request.risk, RiskLevel::Low);

        // Medium risk: write normal file
        let request = PermissionRequest::new("write", "/project/src/main.rs", "Edit code");
        assert_eq!(request.risk, RiskLevel::Medium);

        // High risk: write sensitive file
        let request = PermissionRequest::new("write", "/etc/passwd", "Edit system file");
        assert_eq!(request.risk, RiskLevel::High);
    }

    #[test]
    fn test_permission_scope() {
        let request = PermissionRequest::new("read", "/project/src/main.rs", "View code")
            .with_scope(PermissionScope::Session);
        assert_eq!(request.scope, PermissionScope::Session);
    }

    #[tokio::test]
    async fn test_permission_manager() {
        let manager = PermissionManager::new(false, true);
        let request = PermissionRequest::new("read", "/project/src/main.rs", "View code");

        let result = manager.request_permission(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap()); // Auto-approved (low risk)
    }

    #[test]
    fn test_format_prompt() {
        let manager = PermissionManager::default();
        let request = PermissionRequest::new("read", "/project/src/main.rs", "View code");

        let prompt = manager.format_prompt(&request);
        assert!(prompt.contains("read operation"));
        assert!(prompt.contains("/project/src/main.rs"));
        assert!(prompt.contains("View code"));
    }

    // ── Wildcard matching tests ──────────────────────────────────────────
    #[test]
    fn test_wildcard_exact_match() {
        assert!(wildcard_match("main.rs", "main.rs"));
        assert!(!wildcard_match("main.rs", "lib.rs"));
    }

    #[test]
    fn test_wildcard_star_any() {
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("*", "/deep/nested/path.rs"));
    }

    #[test]
    fn test_wildcard_prefix() {
        assert!(wildcard_match("src/*", "src/main.rs"));
        assert!(wildcard_match("src/*", "src/lib.rs"));
        assert!(!wildcard_match("src/*", "lib/main.rs"));
    }

    #[test]
    fn test_wildcard_suffix() {
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(wildcard_match("*.rs", "src/lib.rs"));
        assert!(!wildcard_match("*.rs", "src/lib.toml"));
    }

    #[test]
    fn test_wildcard_middle() {
        assert!(wildcard_match("src/*/mod.rs", "src/core/mod.rs"));
        assert!(wildcard_match("*main*", "src/main.rs"));
        assert!(wildcard_match("*main*", "main"));
    }

    #[test]
    fn test_wildcard_multiple_stars() {
        assert!(wildcard_match("*.rs", "foo.rs"));
        assert!(wildcard_match("src/**/*.rs", "src/a/b/c.rs"));
    }

    // ── Approval rule tests ──────────────────────────────────────────────
    #[test]
    fn test_approval_rule_matches_exact() {
        let rule = ApprovalRule::new("src/main.rs", "write");
        assert!(rule.matches("src/main.rs", "write"));
        assert!(!rule.matches("src/lib.rs", "write"));
        assert!(!rule.matches("src/main.rs", "read"));
    }

    #[test]
    fn test_approval_rule_matches_wildcard_path() {
        let rule = ApprovalRule::new("src/*", "write");
        assert!(rule.matches("src/main.rs", "write"));
        assert!(rule.matches("src/lib.rs", "write"));
        assert!(!rule.matches("test/main.rs", "write"));
    }

    #[test]
    fn test_approval_rule_matches_wildcard_operation() {
        let rule = ApprovalRule::new("*.toml", "*");
        assert!(rule.matches("Cargo.toml", "read"));
        assert!(rule.matches("Cargo.toml", "write"));
        assert!(!rule.matches("main.rs", "read"));
    }

    #[test]
    fn test_approval_rule_matches_extension() {
        let rule = ApprovalRule::new("*.rs", "read");
        assert!(rule.matches("main.rs", "read"));
        assert!(rule.matches("src/lib.rs", "read"));
        assert!(!rule.matches("Cargo.toml", "read"));
    }

    #[tokio::test]
    async fn test_pre_approved_path() {
        let manager = PermissionManager::new(true, false);
        manager.approve("*.rs", "write");

        // Should be pre-approved even though it's medium risk
        let request = PermissionRequest::new("write", "src/main.rs", "Edit code");
        let result = manager.request_permission(&request).await;
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_not_approved_different_op() {
        let manager = PermissionManager::new(true, false);
        manager.approve("*.rs", "read");

        // Different operation should not be pre-approved
        assert!(!manager.is_pre_approved("src/main.rs", "write"));
    }

    #[test]
    fn test_revoke_approval() {
        let manager = PermissionManager::new(true, true);
        manager.approve("*.rs", "write");
        assert!(manager.is_pre_approved("main.rs", "write"));

        manager.revoke("*.rs");
        assert!(!manager.is_pre_approved("main.rs", "write"));
    }

    #[test]
    fn test_list_approvals() {
        let manager = PermissionManager::new(true, true);
        manager.approve("*.rs", "write");
        manager.approve("*.toml", "read");

        let rules = manager.list_approvals();
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_no_duplicate_approvals() {
        let manager = PermissionManager::new(true, true);
        manager.approve("*.rs", "write");
        manager.approve("*.rs", "write"); // duplicate

        assert_eq!(manager.list_approvals().len(), 1);
    }
}

// ── Tool Confirmation Router ────────────────────────────────────────────────
//
// Ported from goose's tool_confirmation_router.rs. Provides async routing
// of tool permission confirmations between the tool execution pipeline
// and the UI/permission handler.

/// Decision made for a tool permission request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConfirmationDecision {
    /// Allow this specific invocation
    AllowOnce,
    /// Deny this specific invocation
    DenyOnce,
    /// Allow all similar operations for the rest of the session
    AllowSession,
}

/// A pending permission confirmation with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfirmation {
    /// Unique request ID
    pub request_id: String,
    /// Tool name being confirmed
    pub tool_name: String,
    /// The decision
    pub decision: ConfirmationDecision,
}

/// Routes tool permission confirmations between async tasks.
///
/// This provides a clean mechanism for:
/// - Tool executors to register pending confirmation requests
/// - UI/permission handlers to deliver confirmation decisions
/// - Automatic cleanup of stale entries (dropped receivers)
///
/// Inspired by goose's `ToolConfirmationRouter`.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::permission::ToolConfirmationRouter;
///
/// let router = ToolConfirmationRouter::new();
///
/// // In tool executor:
/// let rx = router.register("req-1".to_string()).await;
/// let confirmation = rx.await.unwrap();
///
/// // In UI handler:
/// router.deliver("req-1", ToolConfirmation {
///     request_id: "req-1".to_string(),
///     tool_name: "bash".to_string(),
///     decision: ConfirmationDecision::AllowOnce,
/// }).await;
/// ```
pub struct ToolConfirmationRouter {
    pending: tokio::sync::Mutex<
        std::collections::HashMap<String, tokio::sync::oneshot::Sender<ToolConfirmation>>,
    >,
}

impl ToolConfirmationRouter {
    /// Create a new confirmation router
    pub fn new() -> Self {
        Self {
            pending: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Register a pending confirmation request.
    ///
    /// Returns a oneshot receiver that will receive the confirmation
    /// when `deliver` is called. Stale entries (dropped receivers) are
    /// pruned on each register call.
    pub async fn register(
        &self,
        request_id: String,
    ) -> tokio::sync::oneshot::Receiver<ToolConfirmation> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut pending = self.pending.lock().await;
        // Prune stale entries (receivers already dropped)
        pending.retain(|_, sender| !sender.is_closed());
        pending.insert(request_id, tx);
        rx
    }

    /// Deliver a confirmation decision for a pending request.
    ///
    /// Returns `true` if the confirmation was delivered successfully,
    /// `false` if the request was not found or the receiver was dropped.
    pub async fn deliver(&self, request_id: &str, confirmation: ToolConfirmation) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(request_id) {
            if tx.send(confirmation).is_err() {
                log::warn!(
                    "Confirmation receiver dropped for request {request_id} (task cancelled)"
                );
                false
            } else {
                true
            }
        } else {
            log::warn!("No task waiting for confirmation: {request_id}");
            false
        }
    }

    /// Get the number of pending confirmations (excluding stale entries)
    pub async fn pending_count(&self) -> usize {
        let mut pending = self.pending.lock().await;
        pending.retain(|_, sender| !sender.is_closed());
        pending.len()
    }

    /// Cancel all pending confirmations
    pub async fn cancel_all(&self) {
        self.pending.lock().await.clear();
    }
}

impl Default for ToolConfirmationRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod confirmation_tests {
    use super::*;

    #[tokio::test]
    async fn test_register_then_deliver() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;

        assert!(
            router
                .deliver(
                    "req_1",
                    ToolConfirmation {
                        request_id: "req_1".to_string(),
                        tool_name: "bash".to_string(),
                        decision: ConfirmationDecision::AllowOnce,
                    }
                )
                .await
        );

        let confirmation = rx.await.unwrap();
        assert_eq!(confirmation.decision, ConfirmationDecision::AllowOnce);
        assert_eq!(confirmation.tool_name, "bash");
    }

    #[tokio::test]
    async fn test_deliver_unknown_request() {
        let router = ToolConfirmationRouter::new();
        assert!(
            !router
                .deliver(
                    "unknown",
                    ToolConfirmation {
                        request_id: "unknown".to_string(),
                        tool_name: "bash".to_string(),
                        decision: ConfirmationDecision::DenyOnce,
                    }
                )
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
                .deliver(
                    "req_1",
                    ToolConfirmation {
                        request_id: "req_1".to_string(),
                        tool_name: "bash".to_string(),
                        decision: ConfirmationDecision::AllowOnce,
                    }
                )
                .await
        );
    }

    #[tokio::test]
    async fn test_stale_entries_pruned_on_register() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;
        drop(rx); // stale entry

        // Register another — should prune stale req_1
        let _rx2 = router.register("req_2".to_string()).await;
        assert_eq!(router.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_concurrent_out_of_order() {
        let router = std::sync::Arc::new(ToolConfirmationRouter::new());

        let rx1 = router.register("req_1".to_string()).await;
        let rx2 = router.register("req_2".to_string()).await;

        // Deliver in reverse order
        assert!(
            router
                .deliver(
                    "req_2",
                    ToolConfirmation {
                        request_id: "req_2".to_string(),
                        tool_name: "write_file".to_string(),
                        decision: ConfirmationDecision::DenyOnce,
                    }
                )
                .await
        );
        assert_eq!(router.pending_count().await, 1);

        assert!(
            router
                .deliver(
                    "req_1",
                    ToolConfirmation {
                        request_id: "req_1".to_string(),
                        tool_name: "bash".to_string(),
                        decision: ConfirmationDecision::AllowOnce,
                    }
                )
                .await
        );
        assert_eq!(router.pending_count().await, 0);

        let c1 = rx1.await.unwrap();
        assert_eq!(c1.decision, ConfirmationDecision::AllowOnce);
        let c2 = rx2.await.unwrap();
        assert_eq!(c2.decision, ConfirmationDecision::DenyOnce);
    }

    #[tokio::test]
    async fn test_cancel_all() {
        let router = ToolConfirmationRouter::new();
        let _rx1 = router.register("req_1".to_string()).await;
        let _rx2 = router.register("req_2".to_string()).await;

        router.cancel_all().await;
        assert_eq!(router.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_allow_decision() {
        let router = ToolConfirmationRouter::new();
        let rx = router.register("req_1".to_string()).await;

        router
            .deliver(
                "req_1",
                ToolConfirmation {
                    request_id: "req_1".to_string(),
                    tool_name: "read_file".to_string(),
                    decision: ConfirmationDecision::AllowSession,
                },
            )
            .await;

        let c = rx.await.unwrap();
        assert_eq!(c.decision, ConfirmationDecision::AllowSession);
    }
}
