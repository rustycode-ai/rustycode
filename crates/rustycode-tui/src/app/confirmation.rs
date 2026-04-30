//! Simple synchronous confirmation bridge between background tasks and the TUI.
//!
//! This lightweight helper provides a global registry where background code can
//! register a confirmation request and block waiting for the user's decision.
//! The TUI polls pending requests and displays an in-UI modal; when the user
//! decides the TUI calls `deliver` to resolve the waiting request.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{mpsc, Mutex};

static GLOBAL_CONFIRMATIONS: Lazy<Mutex<HashMap<String, mpsc::Sender<bool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a new confirmation request. Returns a blocking `Receiver<bool>` that
/// the caller can `recv()` on (use `spawn_blocking` if calling from async).
pub fn register(request_id: String) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel();
    let mut map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.insert(request_id, tx);
    rx
}

/// Deliver a boolean decision for a pending request. Returns true if the
/// decision was delivered, false if no pending request matched the id.
pub fn deliver(request_id: &str, decision: bool) -> bool {
    let mut map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = map.remove(request_id) {
        tx.send(decision).is_ok()
    } else {
        false
    }
}

/// List pending request ids (non-blocking).
pub fn pending_list() -> Vec<String> {
    let map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.keys().cloned().collect()
}

/// Number of pending confirmation requests.
pub fn pending_count() -> usize {
    let map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.len()
}
