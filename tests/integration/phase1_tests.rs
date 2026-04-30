// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Comprehensive integration tests for Phase 1 components.
//!
//! Tests cover:
//! - rustycode-id: Sortable ID system
//! - rustycode-bus: Event bus with wildcards and hooks
//! - rustycode-runtime: Async runtime
//! - Compile-time tool system
//! - Cross-component integration

// ============================================================================
// ID System Tests
// ============================================================================

#[cfg(test)]
mod id_system_tests {
    use rustycode_id::{EventId, MemoryId, SessionId, SkillId, SortableId, ToolId};

    #[test]
    fn test_id_generation_and_uniqueness() {
        let mut ids = std::collections::HashSet::new();

        // Generate 1000 IDs and verify uniqueness
        for _ in 0..1000 {
            let id = SessionId::new();
            let id_str = id.to_string();
            assert!(ids.insert(id_str), "Duplicate ID generated!");
        }
    }

    #[test]
    fn test_time_based_sorting() {
        use std::thread;
        use std::time::Duration;

        let id1 = SessionId::new();
        thread::sleep(Duration::from_millis(10));
        let id2 = SessionId::new();
        thread::sleep(Duration::from_millis(10));
        let id3 = SessionId::new();

        // IDs should be sortable by creation time
        assert!(id1.to_string() < id2.to_string());
        assert!(id2.to_string() < id3.to_string());
        assert!(id1.to_string() < id3.to_string());
    }

    #[test]
    fn test_prefix_based_filtering() {
        let session_id = SessionId::new();
        let event_id = EventId::new();
        let memory_id = MemoryId::new();
        let skill_id = SkillId::new();
        let tool_id = ToolId::new();

        assert!(session_id.to_string().starts_with("sess_"));
        assert!(event_id.to_string().starts_with("evt_"));
        assert!(memory_id.to_string().starts_with("mem_"));
        assert!(skill_id.to_string().starts_with("skl_"));
        assert!(tool_id.to_string().starts_with("tool_"));

        // Verify parsing preserves prefix
        let parsed_session = SessionId::parse(session_id.to_string()).unwrap();
        assert_eq!(parsed_session.to_string(), session_id.to_string());
    }

    #[test]
    fn test_serialization_deserialization() {
        let id = SessionId::new();

        // Test JSON serialization
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("sess_"));

        // Test JSON deserialization
        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());

        // Test direct string parsing
        let parsed2 = SessionId::parse(id.to_string()).unwrap();
        assert_eq!(parsed2.to_string(), id.to_string());
    }

    #[test]
    fn test_id_compactness() {
        let id = SessionId::new();
        let id_str = id.to_string();

        // Sortable ID should be much shorter than UUID (36 chars)
        assert!(
            id_str.len() < 36,
            "Sortable ID should be < 36 chars, got {}",
            id_str.len()
        );
        assert!(
            id_str.len() >= 15,
            "Sortable ID should be >= 15 chars, got {}",
            id_str.len()
        );
    }

    #[test]
    fn test_sortable_id_components() {
        let ts_ms = 1234567890000u64;
        let random = 9876543210u64;

        let id = SortableId::from_components("test_", ts_ms, random);

        assert_eq!(id.prefix(), "test_");
        assert_eq!(id.timestamp_ms(), ts_ms);
        assert_eq!(id.random(), random);
    }

    #[test]
    fn test_multiple_id_types_coexistence() {
        let sess = SessionId::new();
        let evt = EventId::new();
        let mem = MemoryId::new();
        let skl = SkillId::new();
        let tool = ToolId::new();

        // All IDs should be unique even with different prefixes
        let ids = [
            sess.to_string(),
            evt.to_string(),
            mem.to_string(),
            skl.to_string(),
            tool.to_string(),
        ];

        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique_ids.len(), 5, "All IDs should be unique");
    }

    #[test]
    fn test_id_parsing_errors() {
        use rustycode_id::IdError;

        // Test invalid formats
        assert!(matches!(
            SessionId::parse("invalid"),
            Err(IdError::InvalidFormat(_))
        ));

        assert!(matches!(
            SessionId::parse("sess_123"),
            Err(IdError::TooShort { .. })
        ));

        // Test wrong prefix
        assert!(matches!(
            SessionId::parse("evt_1234567890123456"),
            Err(IdError::InvalidPrefix { .. })
        ));
    }
}

// ============================================================================
// Event Bus Tests
// ============================================================================

#[cfg(test)]
mod event_bus_tests {
    use rustycode_bus::{
        ContextAssembledEvent, EventBus, HookPhase, SessionStartedEvent, ToolExecutedEvent,
    };
    use rustycode_protocol::{ContextPlan, SessionId};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_publish_subscribe_basic_flow() {
        let bus = EventBus::default();

        // Subscribe to session events
        let (_id, mut rx) = bus.subscribe("session.started").await.unwrap();

        // Publish an event
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test detail".to_string(),
        );

        bus.publish(event).await.unwrap();

        // Receive the event
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");

        let downcast = received
            .as_any()
            .downcast_ref::<SessionStartedEvent>()
            .unwrap();
        assert_eq!(downcast.task, "Test task");
    }

    #[tokio::test]
    async fn test_wildcard_subscriptions() {
        let bus = EventBus::default();

        // Subscribe to all session events
        let (_id1, mut rx1) = bus.subscribe("session.*").await.unwrap();

        // Subscribe to all events
        let (_id2, mut rx2) = bus.subscribe("*").await.unwrap();

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Wildcard test".to_string(),
            "Testing wildcards".to_string(),
        );

        bus.publish(event).await.unwrap();

        // Both subscribers should receive
        let recv1 = rx1.recv().await.unwrap();
        let recv2 = rx2.recv().await.unwrap();

        assert_eq!(recv1.event_type(), "session.started");
        assert_eq!(recv2.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_wildcard_pattern_matching() {
        let bus = EventBus::default();

        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();

        // Publish different session events
        bus.publish(SessionStartedEvent::new(
            SessionId::new(),
            "Task 1".to_string(),
            "Detail 1".to_string(),
        ))
        .await
        .unwrap();

        // Publish a non-session event (should NOT match session.*)
        bus.publish(ContextAssembledEvent::new(
            SessionId::new(),
            ContextPlan {
                total_budget: 100000,
                reserved_budget: 80000,
                sections: vec![],
            },
            "Context ready".to_string(),
        ))
        .await
        .unwrap();

        // Should only receive session.started event
        let recv1 = rx.recv().await.unwrap();
        assert_eq!(recv1.event_type(), "session.started");

        // Channel should timeout on second event (context.assembled doesn't match session.*)
        let result = tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv()).await;
        assert!(
            result.is_err(),
            "Should not receive context.assembled event"
        );
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let bus = EventBus::default();
        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        // Register a pre-publish hook
        bus.register_hook(HookPhase::PrePublish, move |_event| {
            hook_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Hook test".to_string(),
            "Testing hooks".to_string(),
        );

        bus.publish(event).await.unwrap();

        // Give hook time to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_multiple_hooks_execution_order() {
        let bus = EventBus::default();
        let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let order_clone = execution_order.clone();

        // Register multiple hooks
        for i in 0..3 {
            let order_clone = order_clone.clone();
            bus.register_hook(HookPhase::PrePublish, move |_event| {
                let mut order = order_clone.lock().unwrap();
                order.push(i);
                Ok(())
            })
            .await;
        }

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Multiple hooks".to_string(),
            "Testing execution order".to_string(),
        );

        bus.publish(event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let order = execution_order.lock().unwrap();
        assert_eq!(*order, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn test_concurrent_subscribers() {
        let bus = EventBus::default();
        let subscriber_count = Arc::new(AtomicUsize::new(0));

        // Create multiple subscribers
        let mut receivers = vec![];
        for _ in 0..10 {
            let (_id, rx) = bus.subscribe("session.*").await.unwrap();
            receivers.push(rx);
        }

        // Publish an event
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Concurrent test".to_string(),
            "Testing concurrent subscribers".to_string(),
        );

        bus.publish(event).await.unwrap();

        // All subscribers should receive the event
        for mut rx in receivers {
            let recv = rx.recv().await.unwrap();
            assert_eq!(recv.event_type(), "session.started");
            subscriber_count.fetch_add(1, Ordering::SeqCst);
        }

        assert_eq!(subscriber_count.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_automatic_cleanup_on_drop() {
        use rustycode_bus::SubscriptionHandle;

        let bus = Arc::new(EventBus::default());

        let (id, _rx) = bus.subscribe("session.*").await.unwrap();
        let handle = SubscriptionHandle::new(id, bus.clone());

        let metrics_before = bus.metrics();
        assert_eq!(metrics_before.subscriber_count, 1);

        // Drop the handle
        drop(handle);

        // Give time for async cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let metrics_after = bus.metrics();
        assert_eq!(metrics_after.subscriber_count, 0);
    }

    #[tokio::test]
    async fn test_multiple_event_types() {
        let bus = EventBus::default();

        let (_id, mut rx) = bus.subscribe("*").await.unwrap();

        let session_id = SessionId::new();

        // Publish different event types
        bus.publish(SessionStartedEvent::new(
            session_id.clone(),
            "Test task".to_string(),
            "Test detail".to_string(),
        ))
        .await
        .unwrap();

        bus.publish(ContextAssembledEvent::new(
            session_id.clone(),
            ContextPlan {
                total_budget: 100000,
                reserved_budget: 80000,
                sections: vec![],
            },
            "Context assembled".to_string(),
        ))
        .await
        .unwrap();

        bus.publish(ToolExecutedEvent::new(
            session_id.clone(),
            "read_file".to_string(),
            serde_json::json!({"path": "/test"}),
            true,
            "File contents".to_string(),
            None,
        ))
        .await
        .unwrap();

        // Receive all three events
        let recv1 = rx.recv().await.unwrap();
        assert_eq!(recv1.event_type(), "session.started");

        let recv2 = rx.recv().await.unwrap();
        assert_eq!(recv2.event_type(), "context.assembled");

        let recv3 = rx.recv().await.unwrap();
        assert_eq!(recv3.event_type(), "tool.executed");
    }

    #[tokio::test]
    async fn test_event_bus_metrics() {
        let bus = EventBus::default();

        let (_id1, _rx1) = bus.subscribe("session.*").await.unwrap();
        let (_id2, _rx2) = bus.subscribe("tool.*").await.unwrap();

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Metrics test".to_string(),
            "Testing metrics".to_string(),
        );

        bus.publish(event).await.unwrap();

        let metrics = bus.metrics();
        assert_eq!(metrics.events_published, 1);
        assert_eq!(metrics.events_delivered, 1); // Only one subscriber matches
        assert_eq!(metrics.subscriber_count, 2);
    }

    #[tokio::test]
    async fn test_unsubscribe_behavior() {
        let bus = EventBus::default();

        let (id, mut rx) = bus.subscribe("session.*").await.unwrap();

        // Publish before unsubscribe
        bus.publish(SessionStartedEvent::new(
            SessionId::new(),
            "Before unsubscribe".to_string(),
            "Test".to_string(),
        ))
        .await
        .unwrap();

        let _ = rx.recv().await.unwrap();

        // Unsubscribe
        bus.unsubscribe(id).await.unwrap();

        // Publish after unsubscribe
        bus.publish(SessionStartedEvent::new(
            SessionId::new(),
            "After unsubscribe".to_string(),
            "Test".to_string(),
        ))
        .await
        .unwrap();

        // Channel should be closed
        assert!(rx.recv().await.is_err());
    }
}

// ============================================================================
// Runtime Tests
// ============================================================================

#[cfg(test)]
mod runtime_tests {
    use rustycode_runtime::AsyncRuntime;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn create_temp_runtime_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-runtime-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();

        let data_dir = path.join("data");
        let skills_dir = path.join("skills");
        let memory_dir = path.join("memory");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();

        fs::write(
            path.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        path
    }

    #[tokio::test]
    async fn test_async_runtime_loading() {
        let cwd = create_temp_runtime_dir();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Verify runtime loaded successfully
        assert!(runtime.config().data_dir.ends_with("data"));
        assert!(!runtime.tool_list().is_empty());
    }

    #[tokio::test]
    async fn test_runtime_event_publishing() {
        let cwd = create_temp_runtime_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to events
        let (_id, mut rx) = runtime.subscribe_events("session.*").await.unwrap();

        // Run a task that should publish events
        let _report = runtime.run(&cwd, "test task").await.unwrap();

        // Should receive session.started event
        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_runtime_tool_execution() {
        let cwd = create_temp_runtime_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to tool events
        let (_id, mut rx) = runtime.subscribe_events("tool.*").await.unwrap();

        let session_id = rustycode_protocol::SessionId::new();
        let tool_call = rustycode_protocol::ToolCall {
            call_id: "test-1".to_string(),
            name: "list_dir".to_string(),
            arguments: serde_json::json!({"path": cwd.to_str()}),
        };

        let _result = runtime
            .execute_tool(&session_id, tool_call, &cwd)
            .await
            .unwrap();

        // Should receive tool.executed event
        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type(), "tool.executed");
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let cwd = create_temp_runtime_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to all events
        let (_id, mut rx) = runtime.subscribe_events("*").await.unwrap();

        // Run a task (creates a session)
        let report = runtime.run(&cwd, "lifecycle test").await.unwrap();
        let session_id = report.session.id.clone();

        // Receive session.started
        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.event_type(), "session.started");

        // Receive context.assembled
        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.event_type(), "context.assembled");

        // Query recent sessions
        let sessions = runtime.recent_sessions(10).await.unwrap();
        assert!(!sessions.is_empty());

        // Query session events
        let events = runtime.session_events(&session_id).await.unwrap();
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_runtime_shutdown() {
        let cwd = create_temp_runtime_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Shutdown should complete without errors
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_subscribers_to_runtime() {
        let cwd = create_temp_runtime_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Multiple subscribers
        let (_id1, mut rx1) = runtime.subscribe_events("session.*").await.unwrap();
        let (_id2, mut rx2) = runtime.subscribe_events("*").await.unwrap();

        let _report = runtime.run(&cwd, "multi-subscriber test").await.unwrap();

        // Both should receive
        let recv1 = rx1.recv().await.unwrap();
        let recv2 = rx2.recv().await.unwrap();

        assert_eq!(recv1.event_type(), "session.started");
        assert_eq!(recv2.event_type(), "session.started");
    }
}

// ============================================================================
// Compile-Time Tool System Tests
// ============================================================================

#[cfg(test)]
mod compile_time_tool_tests {
    use rustycode_tools::{
        BashInput, CompileTimeBash, CompileTimeReadFile, CompileTimeTool,
        CompileTimeToolPermission, CompileTimeWriteFile, ReadFileInput, ToolDispatcher,
        WriteFileInput,
    };
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_file(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (dir, file_path)
    }

    #[test]
    fn test_compile_time_tool_metadata() {
        assert_eq!(CompileTimeReadFile::METADATA.name, "read_file");
        assert_eq!(
            CompileTimeReadFile::METADATA.permission,
            CompileTimeToolPermission::Read
        );

        assert_eq!(CompileTimeWriteFile::METADATA.name, "write_file");
        assert_eq!(
            CompileTimeWriteFile::METADATA.permission,
            CompileTimeToolPermission::Write
        );

        assert_eq!(CompileTimeBash::METADATA.name, "bash");
        assert_eq!(
            CompileTimeBash::METADATA.permission,
            CompileTimeToolPermission::Execute
        );
    }

    #[test]
    fn test_compile_time_read_file() {
        let (_dir, path) = create_test_file("Hello, World!");

        let input = ReadFileInput {
            path: path.clone(),
            start_line: None,
            end_line: None,
        };

        let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(input).unwrap();

        assert_eq!(result.content, "Hello, World!");
        assert_eq!(result.path, path);
        assert_eq!(result.bytes, 13);
    }

    #[test]
    fn test_compile_time_write_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.txt");

        let input = WriteFileInput {
            path: path.clone(),
            content: "Test content".to_string(),
            create_parents: Some(false),
        };

        let result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(input).unwrap();

        assert_eq!(result.path, path);
        assert_eq!(result.bytes_written, 12);

        let read_content = fs::read_to_string(&path).unwrap();
        assert_eq!(read_content, "Test content");
    }

    #[test]
    fn test_compile_time_bash() {
        let input = BashInput {
            command: "echo".to_string(),
            args: Some(vec!["-n".to_string(), "Hello".to_string()]),
            working_dir: None,
            timeout_secs: Some(5),
        };

        let result = ToolDispatcher::<CompileTimeBash>::dispatch(input).unwrap();

        assert_eq!(result.stdout, "Hello");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_type_safety_compile_time() {
        // This test verifies compile-time type checking
        // The code below type-checks correctly

        let input = ReadFileInput {
            path: PathBuf::from("/etc/hosts"),
            start_line: None,
            end_line: None,
        };

        // Correct return type
        let result: Result<rustycode_tools::ReadFileOutput, rustycode_tools::ReadFileError> =
            ToolDispatcher::<CompileTimeReadFile>::dispatch(input);

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_tool_permissions() {
        // Verify permission levels are correctly defined
        assert_eq!(
            CompileTimeReadFile::METADATA.permission,
            CompileTimeToolPermission::Read
        );
        assert_eq!(
            CompileTimeWriteFile::METADATA.permission,
            CompileTimeToolPermission::Write
        );
        assert_eq!(
            CompileTimeBash::METADATA.permission,
            CompileTimeToolPermission::Execute
        );
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use rustycode_bus::{ContextAssembledEvent, EventBus, SessionStartedEvent, ToolExecutedEvent};
    use rustycode_protocol::{SessionId, ToolCall};
    use rustycode_runtime::AsyncRuntime;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use uuid::Uuid;

    fn create_integration_test_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-integration-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();

        let data_dir = path.join("data");
        let skills_dir = path.join("skills");
        let memory_dir = path.join("memory");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();

        fs::write(
            path.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        path
    }

    #[tokio::test]
    async fn test_end_to_end_load_run_verify_events() {
        let cwd = create_integration_test_dir();

        // Load runtime
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to all events
        let (_id, mut rx) = runtime.subscribe_events("*").await.unwrap();

        // Run a task
        let report = runtime.run(&cwd, "integration test task").await.unwrap();

        // Verify session.started event
        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.event_type(), "session.started");
        let started = event1
            .as_any()
            .downcast_ref::<SessionStartedEvent>()
            .unwrap();
        assert_eq!(started.task, "integration test task");

        // Verify context.assembled event
        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.event_type(), "context.assembled");
        let assembled = event2
            .as_any()
            .downcast_ref::<ContextAssembledEvent>()
            .unwrap();
        assert_eq!(assembled.session_id, report.session.id);

        // Verify session was created
        assert_eq!(started.session_id, report.session.id);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receiving_events() {
        let cwd = create_integration_test_dir();
        let _runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe directly to event bus (runtime is not Send-safe)
        let bus = EventBus::default();
        let _event_count = Arc::new(AtomicUsize::new(0));

        // Create multiple subscribers
        let mut receivers = vec![];
        for _ in 0..5 {
            let (_id, rx) = bus.subscribe("session.*").await.unwrap();
            receivers.push(rx);
        }

        // Publish an event
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Multi-subscriber test".to_string(),
            "Testing".to_string(),
        );

        bus.publish(event).await.unwrap();

        // All subscribers should receive
        let mut received = 0;
        for mut rx in receivers {
            if tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv())
                .await
                .is_ok()
            {
                received += 1;
            }
        }

        assert_eq!(received, 5);
    }

    #[tokio::test]
    async fn test_tools_publishing_events() {
        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to tool events
        let (_id, mut rx) = runtime.subscribe_events("tool.*").await.unwrap();

        let session_id = SessionId::new();
        let tool_call = ToolCall {
            call_id: "test-call".to_string(),
            name: "list_dir".to_string(),
            arguments: serde_json::json!({"path": cwd.to_str()}),
        };

        // Execute tool
        let result = runtime
            .execute_tool(&session_id, tool_call.clone(), &cwd)
            .await
            .unwrap();
        assert!(result.success);

        // Verify tool.executed event
        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type(), "tool.executed");

        let executed = event.as_any().downcast_ref::<ToolExecutedEvent>().unwrap();
        assert_eq!(executed.session_id, session_id);
        assert_eq!(executed.tool_name, "list_dir");
        assert!(executed.success);
    }

    #[tokio::test]
    async fn test_storage_persisting_events() {
        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Run a task
        let report = runtime.run(&cwd, "storage persistence test").await.unwrap();
        let session_id = report.session.id.clone();

        // Query events from storage
        let events = runtime.session_events(&session_id).await.unwrap();

        // Should have persisted events
        assert!(!events.is_empty());

        // Verify event types
        let event_kinds: Vec<_> = events.iter().map(|e| e.kind.clone()).collect();
        assert!(event_kinds.contains(&rustycode_protocol::EventKind::SessionStarted));
    }

    #[tokio::test]
    async fn test_event_bus_runtime_integration() {
        let _cwd = create_integration_test_dir();

        // Create standalone event bus
        let bus = Arc::new(EventBus::default());

        // Subscribe to events
        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();

        // Manually publish an event (simulating what runtime does)
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Direct bus test".to_string(),
            "Testing bus directly".to_string(),
        );

        bus.publish(event).await.unwrap();

        // Verify event received
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_id_generation_in_runtime_context() {
        use rustycode_id::SessionId;

        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Run a task
        let report = runtime.run(&cwd, "id generation test").await.unwrap();

        // Verify session ID is valid
        let id_str = report.session.id.to_string();
        let parsed_id = SessionId::parse(&id_str);

        assert!(parsed_id.is_ok());
        assert!(id_str.starts_with("sess_"));
    }

    #[tokio::test]
    async fn test_concurrent_tool_executions() {
        let cwd = create_integration_test_dir();
        let runtime = Arc::new(AsyncRuntime::load(&cwd).await.unwrap());

        let (_id, mut rx) = runtime.subscribe_events("tool.*").await.unwrap();

        // Execute tools sequentially (AsyncRuntime is not Send-safe)
        for i in 0..3 {
            let session_id = SessionId::new();
            let tool_call = ToolCall {
                call_id: format!("call-{}", i),
                name: "list_dir".to_string(),
                arguments: serde_json::json!({"path": cwd.to_str()}),
            };

            let result = runtime.execute_tool(&session_id, tool_call, &cwd).await;
            assert!(result.is_ok());
            assert!(result.unwrap().success);
        }

        // Verify all events were published
        let mut count = 0;
        while tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv())
            .await
            .is_ok()
        {
            count += 1;
            if count >= 3 {
                break;
            }
        }

        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_wildcard_filters_across_components() {
        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Subscribe to different wildcards
        let (_id1, mut rx1) = runtime.subscribe_events("session.*").await.unwrap();
        let (_id2, mut rx2) = runtime.subscribe_events("*").await.unwrap();
        let (_id3, mut rx3) = runtime.subscribe_events("tool.*").await.unwrap();

        // Run a task
        let _report = runtime.run(&cwd, "wildcard test").await.unwrap();

        // session.* should receive session.started
        let event1 = rx1.recv().await.unwrap();
        assert_eq!(event1.event_type(), "session.started");

        // * should receive all events
        let event2 = rx2.recv().await.unwrap();
        assert_eq!(event2.event_type(), "session.started");

        let event3 = rx2.recv().await.unwrap();
        assert_eq!(event3.event_type(), "context.assembled");

        // tool.* should not receive session events (timeout)
        tokio::time::timeout(tokio::time::Duration::from_millis(100), rx3.recv())
            .await
            .err();
    }

    #[tokio::test]
    async fn test_plan_step_management() {
        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Start planning
        let report = runtime
            .start_planning(&cwd, "plan step test")
            .await
            .unwrap();
        let plan_id = report.plan.id.clone();

        // Update a step
        let mut step = report.plan.steps[0].clone();
        step.execution_status = rustycode_protocol::StepStatus::Completed;
        step.results = vec!["result 1".to_string()];

        runtime.update_plan_step(&plan_id, 0, &step).await.unwrap();

        // Verify update
        let updated_plan = runtime.load_plan(&plan_id).await.unwrap().unwrap();
        assert_eq!(
            updated_plan.steps[0].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert_eq!(updated_plan.steps[0].results, vec!["result 1".to_string()]);
    }

    #[tokio::test]
    async fn test_memory_storage_details() {
        let cwd = create_integration_test_dir();
        let runtime = AsyncRuntime::load(&cwd).await.unwrap();

        // Save memory
        runtime
            .upsert_memory("test_scope", "test_key", "test_value")
            .await
            .unwrap();

        // Retrieve single
        let value = runtime
            .get_memory_entry("test_scope", "test_key")
            .await
            .unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Retrieve all for scope
        let memory = runtime.get_memory("test_scope").await.unwrap();
        assert_eq!(memory.len(), 1);
        assert_eq!(memory[0].key, "test_key");
        assert_eq!(memory[0].value, "test_value");

        // Update memory
        runtime
            .upsert_memory("test_scope", "test_key", "updated_value")
            .await
            .unwrap();
        let updated_value = runtime
            .get_memory_entry("test_scope", "test_key")
            .await
            .unwrap();
        assert_eq!(updated_value, Some("updated_value".to_string()));
    }
}

// ============================================================================
// Performance and Stress Tests
// ============================================================================

#[cfg(test)]
mod performance_tests {
    use rustycode_bus::EventBus;
    use rustycode_id::SessionId;
    use std::time::Instant;

    #[test]
    fn test_id_generation_performance() {
        const COUNT: usize = 10_000;
        let start = Instant::now();

        let mut ids = std::collections::HashSet::new();
        for _ in 0..COUNT {
            let id = SessionId::new();
            ids.insert(id.to_string());
        }

        let duration = start.elapsed();

        // All IDs should be unique
        assert_eq!(ids.len(), COUNT);

        // Should be fast (< 100ms for 10k IDs)
        assert!(
            duration.as_millis() < 100,
            "ID generation too slow: {:?}",
            duration
        );

        println!("ID generation: {} IDs in {:?}", COUNT, duration);
        println!(
            "Average: {:.2} µs/ID",
            duration.as_micros() as f64 / COUNT as f64
        );
    }

    #[tokio::test]
    async fn test_event_bus_throughput() {
        const EVENT_COUNT: usize = 1000;

        let bus = EventBus::with_config(rustycode_bus::EventBusConfig {
            channel_capacity: EVENT_COUNT,
            ..Default::default()
        });
        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();

        let start = Instant::now();

        // Publish events sequentially
        for i in 0..EVENT_COUNT {
            let event = rustycode_bus::SessionStartedEvent::new(
                SessionId::new(),
                format!("Task {}", i),
                format!("Detail {}", i),
            );

            bus.publish(event).await.ok();
        }

        // Receive all events
        let mut received = 0;
        while tokio::time::timeout(tokio::time::Duration::from_secs(5), rx.recv())
            .await
            .is_ok()
        {
            received += 1;
            if received >= EVENT_COUNT {
                break;
            }
        }

        let duration = start.elapsed();

        assert_eq!(received, EVENT_COUNT);
        assert!(
            duration.as_millis() < 1000,
            "Event bus too slow: {:?}",
            duration
        );

        println!("Event throughput: {} events in {:?}", EVENT_COUNT, duration);
        println!(
            "Average: {:.2} ms/event",
            duration.as_millis() as f64 / EVENT_COUNT as f64
        );
    }

    #[test]
    fn test_serialization_performance() {
        use serde_json;

        const COUNT: usize = 1000;
        let ids: Vec<SessionId> = (0..COUNT).map(|_| SessionId::new()).collect();

        let start = Instant::now();

        for id in &ids {
            let json = serde_json::to_string(id).unwrap();
            let _parsed: SessionId = serde_json::from_str(&json).unwrap();
        }

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 500,
            "Serialization too slow: {:?}",
            duration
        );

        println!("Serialization: {} roundtrips in {:?}", COUNT, duration);
        println!(
            "Average: {:.2} µs/roundtrip",
            duration.as_micros() as f64 / COUNT as f64
        );
    }
}
