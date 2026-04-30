#![allow(clippy::unwrap_used)]
// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Test to verify Arc<Mutex<>> fixes are working correctly

use rustycode_storage::Storage;
use tempfile::tempdir;

#[tokio::test]
async fn test_storage_creation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // This should work without blocking issues
    let storage = Storage::open(&db_path);
    assert!(storage.is_ok());

    let storage = storage.unwrap();

    // Verify we can perform basic operations
    let result = storage.get_events(10);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_event_subscriber_creation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_subscriber.db");

    let storage = Storage::open(&db_path).unwrap();
    let bus = std::sync::Arc::new(rustycode_bus::EventBus::new());

    // This should work with tokio::sync::Mutex
    let subscriber = rustycode_storage::EventSubscriber::new(storage, bus);
    assert!(!subscriber.is_running());

    // Start and stop should work without deadlock
    let result = subscriber.start().await;
    assert!(result.is_ok());

    assert!(subscriber.is_running());

    let result = subscriber.stop().await;
    assert!(result.is_ok());

    assert!(!subscriber.is_running());
}
