//! Simple synchronous confirmation bridge between background tasks and the TUI.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex, LazyLock};

static GLOBAL_CONFIRMATIONS: LazyLock<Mutex<HashMap<String, mpsc::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_deliver_approve() {
        let rx = register("test-approve".to_string());
        assert!(deliver("test-approve", true));
        assert_eq!(rx.recv(), Ok(true));
    }

    #[test]
    fn register_and_deliver_reject() {
        let rx = register("test-reject".to_string());
        assert!(deliver("test-reject", false));
        assert_eq!(rx.recv(), Ok(false));
    }

    #[test]
    fn deliver_unknown_returns_false() {
        assert!(!deliver("nonexistent", true));
    }

    #[test]
    fn pending_list_tracks_registrations() {
        let _rx1 = register("list-a".to_string());
        let _rx2 = register("list-b".to_string());
        let pending = pending_list();
        assert!(pending.contains(&"list-a".to_string()));
        assert!(pending.contains(&"list-b".to_string()));
        // Clean up
        deliver("list-a", true);
        deliver("list-b", true);
    }

    #[test]
    fn pending_count_accurate() {
        let initial = pending_count();
        let _rx = register("count-test".to_string());
        assert_eq!(pending_count(), initial + 1);
        deliver("count-test", true);
        assert_eq!(pending_count(), initial);
    }
}
