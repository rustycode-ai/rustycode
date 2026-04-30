use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Detailed execution state context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub trace_id: String,
    pub session_id: String,
    pub user_id: Option<String>,
    pub workspace_path: Option<String>,
    pub current_task: Option<String>,
}

impl ExecutionContext {
    pub const fn new(trace_id: String, session_id: String) -> Self {
        Self {
            trace_id,
            session_id,
            user_id: None,
            workspace_path: None,
            current_task: None,
        }
    }
}

/// Type alias for shared execution context
pub type SharedContext = Arc<RwLock<ExecutionContext>>;

/// Create a new shared context
pub fn create_context(trace_id: String, session_id: String) -> SharedContext {
    Arc::new(RwLock::new(ExecutionContext::new(trace_id, session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context_creation() {
        let context = ExecutionContext::new("trace-123".to_string(), "session-1".to_string());

        assert_eq!(context.trace_id, "trace-123");
        assert_eq!(context.session_id, "session-1");
        assert_eq!(context.user_id, None);
        assert_eq!(context.workspace_path, None);
        assert_eq!(context.current_task, None);
    }

    #[test]
    fn test_execution_context_with_optional_fields() {
        let mut context = ExecutionContext::new("trace-456".to_string(), "session-2".to_string());

        context.user_id = Some("user-1".to_string());
        context.workspace_path = Some("/home/user/workspace".to_string());
        context.current_task = Some("task-1".to_string());

        assert_eq!(context.user_id, Some("user-1".to_string()));
        assert_eq!(
            context.workspace_path,
            Some("/home/user/workspace".to_string())
        );
        assert_eq!(context.current_task, Some("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_create_shared_context() {
        let context = create_context("trace-789".to_string(), "session-3".to_string());

        let ctx_read = context.read().await;
        assert_eq!(ctx_read.trace_id, "trace-789");
        assert_eq!(ctx_read.session_id, "session-3");
    }

    #[tokio::test]
    async fn test_shared_context_mutation() {
        let context = create_context("trace-mut".to_string(), "session-mut".to_string());

        {
            let mut ctx_write = context.write().await;
            ctx_write.user_id = Some("user-mut".to_string());
        }

        let ctx_read = context.read().await;
        assert_eq!(ctx_read.user_id, Some("user-mut".to_string()));
    }

    #[test]
    fn test_execution_context_serialization() {
        let mut context = ExecutionContext::new("trace-ser".to_string(), "session-ser".to_string());
        context.user_id = Some("user-ser".to_string());

        let json = serde_json::to_string(&context).unwrap();
        let deserialized: ExecutionContext = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.trace_id, context.trace_id);
        assert_eq!(deserialized.session_id, context.session_id);
        assert_eq!(deserialized.user_id, context.user_id);
    }

    #[test]
    fn test_execution_context_debug() {
        let context = ExecutionContext::new("t1".to_string(), "s1".to_string());
        let debug = format!("{:?}", context);
        assert!(debug.contains("t1"));
        assert!(debug.contains("s1"));
        assert!(debug.contains("ExecutionContext"));
    }

    #[test]
    fn test_execution_context_clone() {
        let mut context = ExecutionContext::new("t1".to_string(), "s1".to_string());
        context.user_id = Some("u1".to_string());
        let cloned = context.clone();
        assert_eq!(cloned.trace_id, "t1");
        assert_eq!(cloned.user_id, Some("u1".to_string()));
    }

    #[test]
    fn test_execution_context_full_serialization() {
        let mut context =
            ExecutionContext::new("trace-full".to_string(), "session-full".to_string());
        context.user_id = Some("user-42".to_string());
        context.workspace_path = Some("/dev/project".to_string());
        context.current_task = Some("implement feature".to_string());

        let json = serde_json::to_string(&context).unwrap();
        let decoded: ExecutionContext = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.trace_id, "trace-full");
        assert_eq!(decoded.session_id, "session-full");
        assert_eq!(decoded.user_id, Some("user-42".to_string()));
        assert_eq!(decoded.workspace_path, Some("/dev/project".to_string()));
        assert_eq!(decoded.current_task, Some("implement feature".to_string()));
    }

    #[test]
    fn test_execution_context_empty_strings() {
        let context = ExecutionContext::new("".to_string(), "".to_string());
        assert!(context.trace_id.is_empty());
        assert!(context.session_id.is_empty());
        assert!(context.user_id.is_none());
    }

    #[tokio::test]
    async fn test_shared_context_concurrent_reads() {
        let ctx = create_context("trace-conc".to_string(), "session-conc".to_string());
        let r1 = ctx.read().await;
        let r2 = ctx.read().await;
        assert_eq!(r1.trace_id, r2.trace_id);
    }
}
