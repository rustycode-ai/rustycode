#![allow(
    clippy::significant_drop_tightening,
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    clippy::format_push_string,
    clippy::unused_self,
    clippy::literal_string_with_formatting_args,
    clippy::or_fun_call,
    clippy::option_if_let_else
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::redundant_clone,
        clippy::cast_precision_loss,
        clippy::float_cmp,
        clippy::too_many_lines,
        clippy::match_same_arms,
        clippy::ignored_unit_patterns,
        clippy::unused_peekable
    )
)]

//! Terminal Connector Abstraction for rustycode
//!
//! This crate provides a unified interface for interacting with different
//! terminal multiplexers and terminal applications (tmux, iTerm2, etc.)
//! enabling AI agents to create and manage multi-pane workflows.
//!
//! # Supported Connectors
//!
//! - **Tmux** - Full-featured terminal multiplexer with session/pane management (RECOMMENDED)
//! - **iTerm2 (Native)** - iTerm2 via Unix socket + Protobuf (fastest iTerm2 option)
//! - **iTerm2 (`AppleScript`)** - macOS terminal with `AppleScript` (slow, limited)
//! - **iTerm2 (it2 CLI)** - iTerm2 via it2 Python CLI (slower, more features)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │              TerminalConnector (trait)              │
//! ├─────────────────────────────────────────────────────┤
//! │  - create_session()                                 │
//! │  - split_pane()                                     │
//! │  - send_keys()                                      │
//! │  - capture_output()                                 │
//! │  - close_session()                                  │
//! └─────────────────────────────────────────────────────┘
//!                         ▲
//!           ┌─────────────┴─────────────┐
//!           │                           │
//!    ┌──────┴──────            ┌────────────┐
//!    │ TmuxConnector│            │iTermConnector│
//!    └─────────────┘            └─────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use rustycode_connector::{DetectedConnector, TerminalConnector, SplitDirection};
//!
//! // Auto-detect available connector
//! if let Some(mut conn) = DetectedConnector::detect() {
//!     // Create a new session
//!     let session = conn.create_session("my-work").unwrap();
//!
//!     // Split into panes
//!     conn.split_pane(&session, 0, SplitDirection::Horizontal).unwrap();
//!
//!     // Send commands to panes
//!     conn.send_keys(&session, 0, "cargo build").unwrap();
//!     conn.send_keys(&session, 1, "cargo test").unwrap();
//!
//!     // Capture output
//!     let output = conn.capture_output(&session, 0).unwrap();
//!     println!("Pane 0 output: {}", output);
//! }
//! ```

pub mod detection;
pub mod install;
pub mod it2;
pub mod iterm;
pub mod tmux;

#[cfg(target_os = "macos")]
pub mod iterm2_native;

#[cfg(not(target_os = "macos"))]
pub mod iterm2_native {
    use crate::{
        ConnectorError, ConnectorResult, PaneContent, SessionInfo, SplitDirection,
        TerminalConnector, TerminalSessionId,
    };

    #[derive(Debug, Clone, Default)]
    pub struct ITerm2NativeConnector;

    impl ITerm2NativeConnector {
        pub const fn new() -> Self {
            Self
        }

        pub fn check_available() -> bool {
            false
        }
    }

    impl TerminalConnector for ITerm2NativeConnector {
        fn name(&self) -> &'static str {
            "iTerm2-Native"
        }

        fn is_available(&self) -> bool {
            false
        }

        fn create_session(&mut self, _name: &str) -> ConnectorResult<TerminalSessionId> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn close_session(&mut self, _session: &TerminalSessionId) -> ConnectorResult<()> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn session_info(&self, _session: &TerminalSessionId) -> ConnectorResult<SessionInfo> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn list_sessions(&self) -> ConnectorResult<Vec<SessionInfo>> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn split_pane(
            &mut self,
            _session: &TerminalSessionId,
            _pane_index: usize,
            _direction: SplitDirection,
        ) -> ConnectorResult<usize> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn send_keys(
            &mut self,
            _session: &TerminalSessionId,
            _pane_index: usize,
            _keys: &str,
        ) -> ConnectorResult<()> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn capture_output(
            &self,
            _session: &TerminalSessionId,
            _pane_index: usize,
        ) -> ConnectorResult<PaneContent> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn set_pane_title(
            &mut self,
            _session: &TerminalSessionId,
            _pane_index: usize,
            _title: &str,
        ) -> ConnectorResult<()> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn select_pane(
            &mut self,
            _session: &TerminalSessionId,
            _pane_index: usize,
        ) -> ConnectorResult<()> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn kill_pane(
            &mut self,
            _session: &TerminalSessionId,
            _pane_index: usize,
        ) -> ConnectorResult<()> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }

        fn wait_for_output(
            &self,
            _session: &TerminalSessionId,
            _pane_index: usize,
            _pattern: &str,
            _timeout_secs: Option<u64>,
        ) -> ConnectorResult<PaneContent> {
            Err(ConnectorError::NotAvailable(
                "iTerm2 native connector is only available on macOS".into(),
            ))
        }
    }
}

use std::error::Error;
use std::fmt;

pub use detection::{
    capability_summary, detect_terminal, installation_help, ConnectorAvailability, TerminalType,
};
pub use install::{
    all_connectors, check_connector, print_connector_status, ConnectorInfo, InstallStatus,
};
pub use it2::It2Connector;
pub use iterm::ITermConnector;
pub use iterm2_native::ITerm2NativeConnector;
/// Re-export connector implementations
pub use tmux::CapturePaneOptions;
pub use tmux::TmuxConnector;

/// Result type for connector operations
pub type ConnectorResult<T> = Result<T, ConnectorError>;

/// Error types for terminal connector operations
#[derive(Debug)]
#[non_exhaustive]
pub enum ConnectorError {
    /// The requested connector is not available (binary not found, API unavailable)
    NotAvailable(String),
    /// Failed to create a session
    SessionCreateFailed(String),
    /// Session not found
    SessionNotFound(String),
    /// Failed to split pane
    SplitFailed(String),
    /// Failed to send keys
    SendKeysFailed(String),
    /// Failed to capture output
    CaptureFailed(String),
    /// Timeout waiting for output
    Timeout(String),
    /// Permission denied
    PermissionDenied(String),
    /// Unknown pane index
    UnknownPane(String),
    /// Other error
    Other(String),
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAvailable(msg) => write!(f, "Connector not available: {msg}"),
            Self::SessionCreateFailed(msg) => {
                write!(f, "Session creation failed: {msg}")
            }
            Self::SessionNotFound(msg) => write!(f, "Session not found: {msg}"),
            Self::SplitFailed(msg) => write!(f, "Split failed: {msg}"),
            Self::SendKeysFailed(msg) => write!(f, "Send keys failed: {msg}"),
            Self::CaptureFailed(msg) => write!(f, "Capture failed: {msg}"),
            Self::Timeout(msg) => write!(f, "Timeout: {msg}"),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {msg}"),
            Self::UnknownPane(msg) => write!(f, "Unknown pane: {msg}"),
            Self::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl Error for ConnectorError {}

/// Session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalSessionId(pub String);

impl fmt::Display for TerminalSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Direction for splitting panes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SplitDirection {
    /// Split horizontally (side by side)
    Horizontal,
    /// Split vertically (stacked)
    Vertical,
}

/// Captured content from a pane
#[derive(Debug, Clone)]
pub struct PaneContent {
    /// Raw text content
    pub text: String,
    /// Cursor position (row, col) if available
    pub cursor: Option<(usize, usize)>,
    /// Pane dimensions (rows, cols)
    pub dimensions: Option<(usize, usize)>,
}

impl PaneContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cursor: None,
            dimensions: None,
        }
    }
}

impl fmt::Display for PaneContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// Information about a pane
#[derive(Debug, Clone)]
pub struct PaneInfo {
    /// Unique pane identifier
    pub id: String,
    /// Pane index within the session
    pub index: usize,
    /// Current working directory
    pub cwd: Option<String>,
    /// Command running in the pane (if detectable)
    pub command: Option<String>,
    /// Whether the pane is active
    pub is_active: bool,
}

/// Information about a session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session identifier
    pub id: TerminalSessionId,
    /// Session name
    pub name: String,
    /// List of panes in the session
    pub panes: Vec<PaneInfo>,
    /// Whether the session is currently active
    pub is_active: bool,
}

/// The main trait for terminal connectors
///
/// Implement this trait for each terminal/multiplexer backend
/// (tmux, iTerm2, `WezTerm`, etc.)
pub trait TerminalConnector: Send + Sync {
    /// Human-readable name of the connector
    fn name(&self) -> &'static str;

    /// Check if this connector is available in the current environment
    fn is_available(&self) -> bool;

    /// Create a new session with the given name
    fn create_session(&mut self, name: &str) -> ConnectorResult<TerminalSessionId>;

    /// Close a session
    fn close_session(&mut self, session: &TerminalSessionId) -> ConnectorResult<()>;

    /// Get information about a session
    fn session_info(&self, session: &TerminalSessionId) -> ConnectorResult<SessionInfo>;

    /// List all sessions managed by this connector
    fn list_sessions(&self) -> ConnectorResult<Vec<SessionInfo>>;

    /// Split a pane in the specified direction
    /// Returns the index of the new pane
    fn split_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        direction: SplitDirection,
    ) -> ConnectorResult<usize>;

    /// Send keys/text to a specific pane
    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &str,
    ) -> ConnectorResult<()>;

    /// Capture the current content of a pane
    fn capture_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<PaneContent>;

    /// Set the title/name of a pane
    fn set_pane_title(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()>;

    /// Select/activate a specific pane
    fn select_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<()>;

    /// Kill a specific pane
    fn kill_pane(&mut self, session: &TerminalSessionId, pane_index: usize) -> ConnectorResult<()>;

    /// Wait for output in a pane (with optional timeout)
    fn wait_for_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        pattern: &str,
        timeout_secs: Option<u64>,
    ) -> ConnectorResult<PaneContent>;
}

/// Auto-detected terminal connector
///
/// This struct holds the detected connector and provides
/// convenient access to connector functionality
pub struct DetectedConnector {
    /// The detected connector type
    pub connector_type: TerminalType,
    /// The connector instance (as a trait object)
    pub connector: Box<dyn TerminalConnector>,
}

impl DetectedConnector {
    /// Auto-detect and create the best available connector
    pub fn detect() -> Option<Self> {
        detection::find_best_connector()
    }

    /// Get the connector name
    pub fn name(&self) -> &'static str {
        self.connector.name()
    }
}

impl std::ops::Deref for DetectedConnector {
    type Target = dyn TerminalConnector;

    fn deref(&self) -> &Self::Target {
        self.connector.as_ref()
    }
}

impl std::ops::DerefMut for DetectedConnector {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connector.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connector_error_display() {
        assert_eq!(
            ConnectorError::NotAvailable("tmux not found".into()).to_string(),
            "Connector not available: tmux not found"
        );
        assert_eq!(
            ConnectorError::SessionCreateFailed("timeout".into()).to_string(),
            "Session creation failed: timeout"
        );
        assert_eq!(
            ConnectorError::SessionNotFound("abc".into()).to_string(),
            "Session not found: abc"
        );
        assert_eq!(
            ConnectorError::Timeout("30s".into()).to_string(),
            "Timeout: 30s"
        );
    }

    #[test]
    fn test_session_id_display() {
        let id = TerminalSessionId("my-session".into());
        assert_eq!(id.to_string(), "my-session");
    }

    #[test]
    fn test_session_id_equality() {
        let a = TerminalSessionId("x".into());
        let b = TerminalSessionId("x".into());
        let c = TerminalSessionId("y".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_pane_content_new() {
        let content = PaneContent::new("hello world");
        assert_eq!(content.text, "hello world");
        assert!(content.cursor.is_none());
        assert!(content.dimensions.is_none());
    }

    #[test]
    fn test_pane_content_display() {
        let content = PaneContent::new("output text");
        assert_eq!(content.to_string(), "output text");
    }

    #[test]
    fn test_split_direction_equality() {
        assert_eq!(SplitDirection::Horizontal, SplitDirection::Horizontal);
        assert_ne!(SplitDirection::Horizontal, SplitDirection::Vertical);
    }

    #[test]
    fn test_connector_error_is_std_error() {
        let err: Box<dyn Error> = Box::new(ConnectorError::Other("test".into()));
        assert_eq!(err.to_string(), "Error: test");
    }

    #[test]
    fn test_connector_error_all_variants_display() {
        assert!(ConnectorError::NotAvailable("x".into())
            .to_string()
            .contains("not available"));
        assert!(ConnectorError::SessionCreateFailed("x".into())
            .to_string()
            .contains("creation failed"));
        assert!(ConnectorError::SessionNotFound("x".into())
            .to_string()
            .contains("not found"));
        assert!(ConnectorError::SplitFailed("x".into())
            .to_string()
            .contains("Split failed"));
        assert!(ConnectorError::SendKeysFailed("x".into())
            .to_string()
            .contains("Send keys failed"));
        assert!(ConnectorError::CaptureFailed("x".into())
            .to_string()
            .contains("Capture failed"));
        assert!(ConnectorError::Timeout("x".into())
            .to_string()
            .contains("Timeout"));
        assert!(ConnectorError::PermissionDenied("x".into())
            .to_string()
            .contains("Permission denied"));
        assert!(ConnectorError::UnknownPane("x".into())
            .to_string()
            .contains("Unknown pane"));
        assert!(ConnectorError::Other("x".into())
            .to_string()
            .contains("Error: x"));
    }

    #[test]
    fn test_pane_content_with_cursor_and_dimensions() {
        let content = PaneContent {
            text: "hello".to_string(),
            cursor: Some((5, 10)),
            dimensions: Some((24, 80)),
        };
        assert_eq!(content.cursor, Some((5, 10)));
        assert_eq!(content.dimensions, Some((24, 80)));
        assert_eq!(content.to_string(), "hello");
    }

    #[test]
    fn test_pane_info_construction() {
        let info = PaneInfo {
            id: "pane-0".to_string(),
            index: 0,
            cwd: Some("/home/user".to_string()),
            command: Some("vim".to_string()),
            is_active: true,
        };
        assert_eq!(info.id, "pane-0");
        assert_eq!(info.index, 0);
        assert!(info.is_active);
    }

    #[test]
    fn test_session_info_construction() {
        let info = SessionInfo {
            id: TerminalSessionId("sess-1".into()),
            name: "work".to_string(),
            panes: vec![PaneInfo {
                id: "0".to_string(),
                index: 0,
                cwd: None,
                command: None,
                is_active: true,
            }],
            is_active: true,
        };
        assert_eq!(info.name, "work");
        assert_eq!(info.panes.len(), 1);
    }

    #[test]
    fn test_session_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TerminalSessionId("a".into()));
        set.insert(TerminalSessionId("b".into()));
        set.insert(TerminalSessionId("a".into()));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_connector_result_ok() {
        let result: ConnectorResult<String> = Ok("success".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_connector_result_err() {
        let result: ConnectorResult<String> = Err(ConnectorError::Timeout("30s".into()));
        assert!(result.is_err());
    }

    #[test]
    fn test_detected_connector_detect() {
        // detect() may or may not find a connector depending on environment
        let result = DetectedConnector::detect();
        // Just verify it doesn't panic
        if let Some(detected) = result {
            assert!(!detected.name().is_empty());
        }
    }

    #[test]
    fn test_connector_error_debug() {
        let err = ConnectorError::NotAvailable("test".into());
        let debug = format!("{err:?}");
        assert!(debug.contains("NotAvailable"));
    }

    #[test]
    fn test_connector_error_source() {
        let err: Box<dyn Error> = Box::new(ConnectorError::Timeout("30s".into()));
        let source = err.source();
        assert!(source.is_none()); // No nested source
    }

    #[test]
    fn test_session_id_from_string() {
        let id = TerminalSessionId("my-session-123".into());
        assert_eq!(id.0, "my-session-123");
    }

    #[test]
    fn test_session_id_clone() {
        let id = TerminalSessionId("original".into());
        let cloned = id.clone();
        assert_eq!(id, cloned);
    }

    #[test]
    fn test_pane_content_new_empty() {
        let content = PaneContent::new("");
        assert!(content.text.is_empty());
        assert!(content.cursor.is_none());
        assert!(content.dimensions.is_none());
        assert_eq!(content.to_string(), "");
    }

    #[test]
    fn test_pane_content_new_from_string() {
        let content = PaneContent::new(String::from("hello"));
        assert_eq!(content.text, "hello");
    }

    #[test]
    fn test_pane_content_clone() {
        let content = PaneContent {
            text: "data".to_string(),
            cursor: Some((1, 2)),
            dimensions: Some((10, 20)),
        };
        let cloned = content;
        assert_eq!(cloned.text, "data");
        assert_eq!(cloned.cursor, Some((1, 2)));
        assert_eq!(cloned.dimensions, Some((10, 20)));
    }

    #[test]
    fn test_pane_content_debug() {
        let content = PaneContent::new("test");
        let debug = format!("{content:?}");
        assert!(debug.contains("PaneContent"));
    }

    #[test]
    fn test_pane_info_with_none_fields() {
        let info = PaneInfo {
            id: "pane-x".to_string(),
            index: 5,
            cwd: None,
            command: None,
            is_active: false,
        };
        assert!(info.cwd.is_none());
        assert!(info.command.is_none());
        assert!(!info.is_active);
        assert_eq!(info.index, 5);
    }

    #[test]
    fn test_session_info_with_multiple_panes() {
        let panes: Vec<PaneInfo> = (0..5)
            .map(|i| PaneInfo {
                id: format!("pane-{i}"),
                index: i,
                cwd: if i % 2 == 0 {
                    Some("/home".to_string())
                } else {
                    None
                },
                command: None,
                is_active: i == 0,
            })
            .collect();
        let info = SessionInfo {
            id: TerminalSessionId("multi".into()),
            name: "multi-pane".to_string(),
            panes,
            is_active: true,
        };
        assert_eq!(info.panes.len(), 5);
        assert!(info.panes[0].is_active);
        assert!(!info.panes[1].is_active);
        assert!(info.panes[2].cwd.is_some());
    }

    #[test]
    fn test_connector_result_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ConnectorResult<()>>();
    }

    #[test]
    fn test_split_direction_debug() {
        assert!(format!("{:?}", SplitDirection::Horizontal).contains("Horizontal"));
        assert!(format!("{:?}", SplitDirection::Vertical).contains("Vertical"));
    }

    #[test]
    fn test_split_direction_copy() {
        let a = SplitDirection::Horizontal;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_split_direction_clone() {
        let a = SplitDirection::Vertical;
        let b = a;
        assert_eq!(a, b);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for connector lib
    // =========================================================================

    // 1. ConnectorError display preserves message
    #[test]
    fn connector_error_display_preserves_message() {
        let err = ConnectorError::NotAvailable("gone".into());
        assert!(err.to_string().contains("gone"));

        let err = ConnectorError::PermissionDenied("denied".into());
        assert!(err.to_string().contains("denied"));
    }

    // 2. ConnectorError debug for all variants
    #[test]
    fn connector_error_debug_all_variants() {
        let variants: Vec<ConnectorError> = vec![
            ConnectorError::NotAvailable("a".into()),
            ConnectorError::SessionCreateFailed("b".into()),
            ConnectorError::SessionNotFound("c".into()),
            ConnectorError::SplitFailed("d".into()),
            ConnectorError::SendKeysFailed("e".into()),
            ConnectorError::CaptureFailed("f".into()),
            ConnectorError::Timeout("g".into()),
            ConnectorError::PermissionDenied("h".into()),
            ConnectorError::UnknownPane("i".into()),
            ConnectorError::Other("j".into()),
        ];
        for v in &variants {
            let debug = format!("{v:?}");
            assert!(!debug.is_empty());
        }
    }

    // 3. ConnectorError with unicode messages
    #[test]
    fn connector_error_unicode_messages() {
        let err = ConnectorError::NotAvailable("未找到 🚫".into());
        assert!(err.to_string().contains("未找到"));
        assert!(err.to_string().contains("🚫"));
    }

    // 4. ConnectorError with empty strings
    #[test]
    fn connector_error_empty_messages() {
        let err = ConnectorError::Other(String::new());
        assert_eq!(err.to_string(), "Error: ");

        let err = ConnectorError::NotAvailable(String::new());
        assert!(err.to_string().contains("Connector not available"));
    }

    // 5. PaneContent with unicode text
    #[test]
    fn pane_content_unicode_text() {
        let content = PaneContent::new("Hello 世界 🌍 مرحبا");
        assert_eq!(content.to_string(), "Hello 世界 🌍 مرحبا");
    }

    // 6. PaneContent clone independence
    #[test]
    fn pane_content_clone_independence() {
        let mut content = PaneContent::new("original");
        let cloned = content.clone();
        content.text = "modified".to_string();
        assert_eq!(cloned.text, "original");
    }

    // 7. PaneInfo clone equal
    #[test]
    fn pane_info_clone_equal() {
        let info = PaneInfo {
            id: "p1".to_string(),
            index: 3,
            cwd: Some("/tmp".to_string()),
            command: Some("ls".to_string()),
            is_active: true,
        };
        let cloned = info.clone();
        assert_eq!(cloned.id, info.id);
        assert_eq!(cloned.index, info.index);
        assert_eq!(cloned.cwd, info.cwd);
    }

    // 8. PaneInfo debug format
    #[test]
    fn pane_info_debug_format() {
        let info = PaneInfo {
            id: "dbg-pane".to_string(),
            index: 0,
            cwd: None,
            command: None,
            is_active: false,
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("dbg-pane"));
        assert!(debug.contains("PaneInfo"));
    }

    // 9. SessionInfo clone equal
    #[test]
    fn session_info_clone_equal() {
        let info = SessionInfo {
            id: TerminalSessionId("s1".into()),
            name: "test-session".to_string(),
            panes: vec![],
            is_active: true,
        };
        let cloned = info.clone();
        assert_eq!(cloned.id, info.id);
        assert_eq!(cloned.name, info.name);
        assert_eq!(cloned.is_active, info.is_active);
    }

    // 10. SessionInfo debug format
    #[test]
    fn session_info_debug_format() {
        let info = SessionInfo {
            id: TerminalSessionId("dbg-sess".into()),
            name: "debug-session".to_string(),
            panes: vec![],
            is_active: false,
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("debug-session"));
        assert!(debug.contains("SessionInfo"));
    }

    // 11. TerminalSessionId debug format
    #[test]
    fn session_id_debug_format() {
        let id = TerminalSessionId("my-debug-id".into());
        let debug = format!("{id:?}");
        assert!(debug.contains("my-debug-id"));
    }

    // 12. SplitDirection all variants equality
    #[test]
    fn split_direction_all_variants() {
        assert_eq!(SplitDirection::Horizontal, SplitDirection::Horizontal);
        assert_eq!(SplitDirection::Vertical, SplitDirection::Vertical);
        assert_ne!(SplitDirection::Horizontal, SplitDirection::Vertical);
    }

    // 13. TerminalSessionId with complex name
    #[test]
    fn session_id_complex_name() {
        let id = TerminalSessionId("session-name_with.mixed-chars:123".into());
        assert_eq!(id.to_string(), "session-name_with.mixed-chars:123");
    }

    // 14. ConnectorError implements std Error trait
    #[test]
    fn connector_error_is_std_error() {
        fn check_error(e: &dyn Error) -> String {
            e.to_string()
        }
        let err = ConnectorError::Timeout("5s".into());
        let msg = check_error(&err);
        assert!(msg.contains("Timeout"));
    }

    // 15. PaneContent with large text
    #[test]
    fn pane_content_large_text() {
        let big = "x".repeat(100_000);
        let content = PaneContent::new(big);
        assert_eq!(content.text.len(), 100_000);
        assert_eq!(content.to_string().len(), 100_000);
    }

    // 16. Multiple ConnectorErrors display
    #[test]
    fn multiple_connector_errors_display() {
        let msgs = [
            ConnectorError::NotAvailable("a".into()).to_string(),
            ConnectorError::SessionCreateFailed("b".into()).to_string(),
            ConnectorError::Timeout("c".into()).to_string(),
        ];
        assert!(msgs[0].contains("not available"));
        assert!(msgs[1].contains("creation failed"));
        assert!(msgs[2].contains("Timeout"));
    }
}
