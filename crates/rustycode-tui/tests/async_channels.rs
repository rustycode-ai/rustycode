#![allow(
    clippy::manual_assert,
    clippy::manual_string_new,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for async channel system

use rustycode_tui::app::async_::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_stream_chunk_channel() {
    let mut channel = BoundedChannel::<StreamChunk>::new(10);

    // Send multiple chunks
    for i in 0..5 {
        channel
            .try_send(StreamChunk::Text(format!("Chunk {}", i)))
            .unwrap();
    }
    channel.try_send(StreamChunk::Done).unwrap();

    // Receive and verify
    let mut count = 0;
    loop {
        match channel.try_recv() {
            Some(StreamChunk::Text(_)) => count += 1,
            Some(StreamChunk::Done) => break,
            None => panic!("Expected data"),
            _ => {}
        }
    }

    assert_eq!(count, 5);
}

#[test]
fn test_tool_result_channel() {
    let mut channel = BoundedChannel::<ToolResult>::new(5);

    // Send tool results
    channel
        .try_send(ToolResult::new(
            "1",
            "read_file",
            ToolOutput::Success("File contents".to_string()),
        ))
        .unwrap();

    channel
        .try_send(ToolResult::new(
            "2",
            "bash",
            ToolOutput::Error(ToolExecutionError::Other("Command failed".to_string())),
        ))
        .unwrap();

    // Receive and verify
    let result1 = channel.try_recv().unwrap();
    assert_eq!(result1.id, "1");
    assert_eq!(result1.name, "read_file");

    let result2 = channel.try_recv().unwrap();
    assert_eq!(result2.id, "2");
    assert_eq!(result2.name, "bash");
}

#[test]
fn test_command_result_channel() {
    let mut channel = BoundedChannel::<CommandResult>::new(5);

    // Send command result
    channel
        .try_send(CommandResult::new("echo test", Some(0), "test\n", ""))
        .unwrap();

    // Receive and verify
    let result = channel.try_recv().unwrap();
    assert_eq!(result.command, "echo test");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout, "test\n");
}

#[test]
fn test_workspace_update_channel() {
    let mut channel = BoundedChannel::<WorkspaceUpdate>::new(10);

    // Send updates
    channel
        .try_send(WorkspaceUpdate::ScanProgress {
            scanned: 50,
            total: 100,
        })
        .unwrap();

    channel
        .try_send(WorkspaceUpdate::ScanComplete {
            file_count: 200,
            dir_count: 50,
        })
        .unwrap();

    // Receive and verify
    match channel.try_recv().unwrap() {
        WorkspaceUpdate::ScanProgress { scanned, total } => {
            assert_eq!(scanned, 50);
            assert_eq!(total, 100);
        }
        _ => panic!("Expected ScanProgress"),
    }

    match channel.try_recv().unwrap() {
        WorkspaceUpdate::ScanComplete {
            file_count,
            dir_count,
        } => {
            assert_eq!(file_count, 200);
            assert_eq!(dir_count, 50);
        }
        _ => panic!("Expected ScanComplete"),
    }
}

#[test]
fn test_backpressure_handling() {
    let channel = BoundedChannel::<StreamChunk>::new(2);

    // Fill the channel
    channel
        .try_send(StreamChunk::Text("1".to_string()))
        .unwrap();
    channel
        .try_send(StreamChunk::Text("2".to_string()))
        .unwrap();

    // Channel should be full
    let result = channel.try_send(StreamChunk::Text("3".to_string()));
    assert_eq!(result, Err(ChannelError::Full));
    assert_eq!(channel.dropped_count(), 1);

    // Reset and verify
    channel.reset_dropped_count();
    assert_eq!(channel.dropped_count(), 0);
}

#[test]
fn test_concurrent_access() {
    let mut channel = BoundedChannel::<StreamChunk>::new(100);

    // Spawn producer thread
    let tx = channel.clone_sender();
    thread::spawn(move || {
        for i in 0..20 {
            tx.send(StreamChunk::Text(format!("Item {}", i))).unwrap();
        }
        tx.send(StreamChunk::Done).unwrap();
    });

    // Consume in main thread
    let mut count = 0;
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    loop {
        match channel.try_recv() {
            Some(StreamChunk::Text(_)) => count += 1,
            Some(StreamChunk::Done) => break,
            None => {
                if start.elapsed() > timeout {
                    panic!("Timeout waiting for data");
                }
                thread::sleep(Duration::from_millis(10));
            }
            _ => {}
        }
    }

    assert_eq!(count, 20);
}

#[test]
fn test_clone_sender() {
    let mut channel = BoundedChannel::<StreamChunk>::new(10);

    // Clone multiple senders
    let tx1 = channel.clone_sender();
    let tx2 = channel.clone_sender();

    // Send from different threads
    thread::spawn(move || {
        tx1.send(StreamChunk::Text("From thread 1".to_string()))
            .unwrap();
    });

    thread::spawn(move || {
        tx2.send(StreamChunk::Text("From thread 2".to_string()))
            .unwrap();
    });

    // Receive both
    let mut items = Vec::new();
    let start = std::time::Instant::now();
    while items.len() < 2 && start.elapsed() < Duration::from_secs(1) {
        if let Some(item) = channel.try_recv() {
            items.push(item);
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(items.len(), 2);
}

#[test]
fn test_take_receiver() {
    let mut channel = BoundedChannel::<StreamChunk>::new(10);

    // Send before taking receiver
    channel
        .try_send(StreamChunk::Text("Before".to_string()))
        .unwrap();

    // Take receiver
    let rx = channel.take_receiver().unwrap();
    assert!(!channel.has_receiver());

    // Receive using taken receiver
    let received = rx.try_recv().unwrap();
    assert_eq!(received, StreamChunk::Text("Before".to_string()));
}

#[test]
fn test_state_snapshot() {
    use std::sync::Mutex;

    let state = Mutex::new(vec![1, 2, 3, 4, 5]);

    // Successful snapshot
    let snapshot = Vec::try_snapshot(&state);
    assert_eq!(snapshot, Some(vec![1, 2, 3, 4, 5]));

    // Failed snapshot (locked)
    let _lock = state.lock().unwrap();
    let snapshot = Vec::try_snapshot(&state);
    assert_eq!(snapshot, None);
}

#[test]
fn test_snapshot_wrapper() {
    let snapshot = Snapshot::new(vec![1, 2, 3]);

    assert_eq!(snapshot.get(), &vec![1, 2, 3]);
    assert_eq!(snapshot.clone_data(), vec![1, 2, 3]);
    assert_eq!(snapshot.into_inner(), vec![1, 2, 3]);
}
