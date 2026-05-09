// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Integration tests for the event bus system

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreadable_literal,
    clippy::uninlined_format_args,
    clippy::significant_drop_tightening,
    clippy::redundant_clone,
    clippy::unused_async
)]
//! These tests demonstrate real-world usage patterns and verify
//! the event bus works correctly in complex scenarios.

use rustycode_bus::{
    ContextAssembledEvent, EventBus, HookPhase, SessionStartedEvent, SubscriptionHandle,
    ToolExecutedEvent,
};
use rustycode_protocol::{ContextPlan, SessionId};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_end_to_end_event_flow() {
    let bus = Arc::new(EventBus::new());

    // Subscribe to session events
    let bus_clone1 = Arc::clone(&bus);
    let handle1 = tokio::spawn(async move {
        let (_id, mut rx) = bus_clone1.subscribe("session.*").await.unwrap();
        let mut count = 0;
        while let Ok(event) = rx.recv().await {
            count += 1;
            assert!(event.event_type().starts_with("session."));
            if count >= 2 {
                break;
            }
        }
        count
    });

    // Subscribe to context events
    let bus_clone2 = Arc::clone(&bus);
    let handle2 = tokio::spawn(async move {
        let (_id, mut rx) = bus_clone2.subscribe("context.*").await.unwrap();
        let mut count = 0;
        while let Ok(event) = rx.recv().await {
            count += 1;
            assert!(event.event_type().starts_with("context."));
            if count >= 1 {
                break;
            }
        }
        count
    });

    // Give subscribers time to start
    sleep(Duration::from_millis(50)).await;

    // Publish events
    bus.publish(SessionStartedEvent::new(
        SessionId::new(),
        "Task 1".to_string(),
        "Detail 1".to_string(),
    ))
    .await
    .unwrap();

    bus.publish(ContextAssembledEvent::new(
        SessionId::new(),
        ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        },
        "Context ready".to_string(),
    ))
    .await
    .unwrap();

    bus.publish(SessionStartedEvent::new(
        SessionId::new(),
        "Task 2".to_string(),
        "Detail 2".to_string(),
    ))
    .await
    .unwrap();

    // Wait for subscribers with timeout
    let count1 = tokio::time::timeout(Duration::from_secs(2), handle1)
        .await
        .unwrap()
        .unwrap();
    let count2 = tokio::time::timeout(Duration::from_secs(2), handle2)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(count1, 2);
    assert_eq!(count2, 1);
}

#[tokio::test]
async fn test_concurrent_subscribers() {
    let bus = Arc::new(EventBus::new());

    let mut handles = vec![];

    // Create 10 concurrent subscribers
    for _ in 0..10 {
        let bus_clone = Arc::clone(&bus);
        let handle = tokio::spawn(async move {
            let (_id, mut rx) = bus_clone.subscribe("*").await.unwrap();
            let mut count = 0;
            while rx.recv().await.is_ok() {
                count += 1;
                if count >= 3 {
                    break;
                }
            }
            count
        });
        handles.push(handle);
    }

    // Give subscribers time to start
    sleep(Duration::from_millis(100)).await;

    // Publish 3 events
    for _ in 0..3 {
        bus.publish(SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test detail".to_string(),
        ))
        .await
        .unwrap();
    }

    // All subscribers should receive all 3 events (with timeout)
    for handle in handles {
        let count = tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 3);
    }

    let metrics = bus.metrics();
    assert_eq!(metrics.events_published, 3);
    assert_eq!(metrics.events_delivered, 30); // 3 events * 10 subscribers
}

#[tokio::test]
async fn test_hook_error_handling() {
    let bus = EventBus::new();

    let hook_called = Arc::new(AtomicUsize::new(0));
    let hook_called_clone = Arc::clone(&hook_called);

    // Register a hook that always fails
    bus.register_hook(HookPhase::PrePublish, move |_event| {
        hook_called_clone.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>("Hook failed".into())
    })
    .await;

    // Publish should succeed despite hook error
    let result = bus
        .publish(SessionStartedEvent::new(
            SessionId::new(),
            "Test".to_string(),
            "Test".to_string(),
        ))
        .await;

    // Publish should still succeed (hooks don't block)
    assert!(result.is_ok());
    assert_eq!(hook_called.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_subscription_cleanup() {
    let bus = Arc::new(EventBus::new());

    let (id, _rx) = bus.subscribe("session.*").await.unwrap();
    let handle = SubscriptionHandle::new(id, Arc::clone(&bus));

    // Verify subscription exists
    let metrics_before = bus.metrics();
    assert_eq!(metrics_before.subscriber_count, 1);

    // Drop the handle
    drop(handle);

    // Give time for async cleanup
    sleep(Duration::from_millis(200)).await;

    // Verify subscription was removed
    let metrics_after = bus.metrics();
    assert_eq!(metrics_after.subscriber_count, 0);
}

#[tokio::test]
async fn test_event_downcasting() {
    let bus = EventBus::new();

    let (_id, mut rx) = bus.subscribe("session.started").await.unwrap();

    let session_id = SessionId::new();
    let task = "Test task".to_string();

    bus.publish(SessionStartedEvent::new(
        session_id.clone(),
        task.clone(),
        "Test detail".to_string(),
    ))
    .await
    .unwrap();

    let event = rx.recv().await.unwrap();

    // Verify we can downcast to the correct type
    if let Some(session_event) = event.as_any().downcast_ref::<SessionStartedEvent>() {
        assert_eq!(session_event.session_id, session_id);
        assert_eq!(session_event.task, task);
    } else {
        panic!("Failed to downcast event");
    }
}

#[tokio::test]
async fn test_multiple_event_types() {
    let bus = Arc::new(EventBus::new());

    let session_count = Arc::new(AtomicUsize::new(0));
    let context_count = Arc::new(AtomicUsize::new(0));
    let tool_count = Arc::new(AtomicUsize::new(0));

    let (_session_id, mut session_rx) = bus.subscribe("session.*").await.unwrap();
    let (_context_id, mut context_rx) = bus.subscribe("context.*").await.unwrap();
    let (_tool_id, mut tool_rx) = bus.subscribe("tool.*").await.unwrap();

    let session_count_clone1 = Arc::clone(&session_count);
    let session_handle = tokio::spawn(async move {
        if session_rx.recv().await.is_ok() {
            session_count_clone1.fetch_add(1, Ordering::SeqCst);
        }
    });

    let context_count_clone2 = Arc::clone(&context_count);
    let context_handle = tokio::spawn(async move {
        if context_rx.recv().await.is_ok() {
            context_count_clone2.fetch_add(1, Ordering::SeqCst);
        }
    });

    let tool_count_clone3 = Arc::clone(&tool_count);
    let tool_handle = tokio::spawn(async move {
        if tool_rx.recv().await.is_ok() {
            tool_count_clone3.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Publish different event types
    bus.publish(SessionStartedEvent::new(
        SessionId::new(),
        "Task".to_string(),
        "Detail".to_string(),
    ))
    .await
    .unwrap();

    bus.publish(ContextAssembledEvent::new(
        SessionId::new(),
        ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        },
        "Context".to_string(),
    ))
    .await
    .unwrap();

    bus.publish(ToolExecutedEvent::new(
        SessionId::new(),
        "Read".to_string(),
        json!({ "path": "/test" }),
        true,
        "Success".to_string(),
        None,
    ))
    .await
    .unwrap();

    // Give time for events to be processed
    sleep(Duration::from_millis(100)).await;

    session_handle.await.unwrap();
    context_handle.await.unwrap();
    tool_handle.await.unwrap();

    assert_eq!(session_count.load(Ordering::SeqCst), 1);
    assert_eq!(context_count.load(Ordering::SeqCst), 1);
    assert_eq!(tool_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_wildcard_pattern_matching() {
    let bus = EventBus::new();

    // Test various wildcard patterns
    let patterns = vec!["session.*", "*.started", "*.event", "*"];

    for pattern in patterns {
        let result = bus.subscribe(pattern).await;
        assert!(result.is_ok(), "Pattern '{pattern}' should be valid");
    }
}

#[tokio::test]
async fn test_high_throughput() {
    let bus = Arc::new(EventBus::new());

    let (_id, _rx) = bus.subscribe("*").await.unwrap();

    let start = std::time::Instant::now();

    // Publish 1000 events
    for i in 0..1000 {
        bus.publish(SessionStartedEvent::new(
            SessionId::new(),
            format!("Task {i}"),
            format!("Detail {i}"),
        ))
        .await
        .unwrap();
    }

    let duration = start.elapsed();

    // Should complete in reasonable time (< 1 second)
    assert!(duration.as_secs() < 1, "Too slow: {duration:?}");

    let metrics = bus.metrics();
    assert_eq!(metrics.events_published, 1000);
}
