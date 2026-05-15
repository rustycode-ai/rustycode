use std::collections::HashMap;
use std::sync::{mpsc, LazyLock, Mutex};
use std::time::Instant;

struct PendingEntry {
    sender: mpsc::Sender<bool>,
    created_at: Instant,
}

static GLOBAL_CONFIRMATIONS: LazyLock<Mutex<HashMap<String, PendingEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const STALE_THRESHOLD_SECS: u64 = 300;

pub fn register(request_id: String) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel();
    let mut map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.insert(
        request_id,
        PendingEntry {
            sender: tx,
            created_at: Instant::now(),
        },
    );
    rx
}

pub fn deliver(request_id: &str, decision: bool) -> bool {
    let mut map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = map.remove(request_id) {
        if entry.sender.send(decision).is_err() {
            tracing::debug!(request_id = %request_id, "confirmation receiver dropped before delivery");
            false
        } else {
            true
        }
    } else {
        false
    }
}

pub fn pending_list() -> Vec<String> {
    prune_stale();
    let map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.keys().cloned().collect()
}

pub fn pending_count() -> usize {
    prune_stale();
    let map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.len()
}

pub fn prune_stale() {
    let mut map = GLOBAL_CONFIRMATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let before = map.len();
    map.retain(|id, entry| {
        if entry.created_at.elapsed().as_secs() > STALE_THRESHOLD_SECS {
            tracing::debug!(request_id = %id, "pruning stale confirmation entry");
            false
        } else {
            true
        }
    });
    let pruned = before - map.len();
    if pruned > 0 {
        tracing::debug!(count = pruned, "pruned stale confirmation entries");
    }
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
        deliver("list-a", true);
        deliver("list-b", true);
    }

    #[test]
    fn pending_count_accurate() {
        let before = pending_count();
        let _rx = register("count-test".to_string());
        let after = pending_count();
        assert!(after > before);
        deliver("count-test", true);
        assert!(pending_count() < after);
    }
}
