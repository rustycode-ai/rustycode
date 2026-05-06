//! Undo state.
//!
//! Groups message-position and file-content undo stacks so undo concerns
//! have a single owner.

use std::collections::VecDeque;

/// State for undo operations: message-position undo and file-content undo.
#[derive(Debug)]
pub struct UndoState {
    /// Stack of (message_index, scroll_offset) snapshots for message undo.
    pub(crate) message_stack: VecDeque<(usize, usize)>,
    /// Stack of file-content batches for file edit undo.
    pub(crate) file_stack: Vec<Vec<(String, String)>>,
}

impl UndoState {
    pub fn new() -> Self {
        Self {
            message_stack: VecDeque::with_capacity(crate::app::MAX_UNDO_ENTRIES),
            file_stack: Vec::new(),
        }
    }

    /// Push a message-position snapshot, evicting the oldest if at capacity.
    pub(crate) fn push_message(&mut self, msg_idx: usize, scroll: usize) {
        if self.message_stack.len() >= crate::app::MAX_UNDO_ENTRIES {
            self.message_stack.pop_front();
        }
        self.message_stack.push_back((msg_idx, scroll));
    }

    /// Pop the most recent message-position snapshot.
    pub(crate) fn pop_message(&mut self) -> Option<(usize, usize)> {
        self.message_stack.pop_back()
    }

    /// Push a batch of file edits onto the file undo stack.
    pub(crate) fn push_file_batch(&mut self, batch: Vec<(String, String)>) {
        self.file_stack.push(batch);
        while self.file_stack.len() > crate::app::MAX_FILE_UNDO_BATCHES {
            self.file_stack.remove(0);
        }
    }

    /// Pop the most recent file undo batch.
    pub(crate) fn pop_file_batch(&mut self) -> Option<Vec<(String, String)>> {
        self.file_stack.pop()
    }

    /// Clear all undo state.
    pub(crate) fn clear(&mut self) {
        self.message_stack.clear();
        self.file_stack.clear();
    }
}

impl Default for UndoState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_message_roundtrip() {
        let mut state = UndoState::new();
        assert!(state.pop_message().is_none());
        state.push_message(5, 100);
        state.push_message(10, 200);
        assert_eq!(state.pop_message(), Some((10, 200)));
        assert_eq!(state.pop_message(), Some((5, 100)));
        assert!(state.pop_message().is_none());
    }

    #[test]
    fn message_stack_evicts_oldest_at_capacity() {
        let mut state = UndoState::new();
        for i in 0..crate::app::MAX_UNDO_ENTRIES + 2 {
            state.push_message(i, i * 10);
        }
        // Should have evicted the first 2 entries
        assert_eq!(state.message_stack.len(), crate::app::MAX_UNDO_ENTRIES);
        let first = *state.message_stack.front().unwrap();
        assert_eq!(first.0, 2); // index 0 and 1 were evicted
    }

    #[test]
    fn push_pop_file_batch_roundtrip() {
        let mut state = UndoState::new();
        assert!(state.pop_file_batch().is_none());
        state.push_file_batch(vec![("a.rs".into(), "old_a".into())]);
        state.push_file_batch(vec![("b.rs".into(), "old_b".into())]);
        let batch = state.pop_file_batch().unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].0, "b.rs");
    }

    #[test]
    fn file_stack_evicts_oldest_at_capacity() {
        let mut state = UndoState::new();
        for i in 0..crate::app::MAX_FILE_UNDO_BATCHES + 3 {
            state.push_file_batch(vec![(format!("f{i}"), format!("content{i}"))]);
        }
        assert_eq!(state.file_stack.len(), crate::app::MAX_FILE_UNDO_BATCHES);
        let first = &state.file_stack[0];
        assert!(first[0].0.starts_with("f3")); // first 3 were evicted
    }

    #[test]
    fn clear_resets_both_stacks() {
        let mut state = UndoState::new();
        state.push_message(1, 2);
        state.push_file_batch(vec![("x".into(), "y".into())]);
        state.clear();
        assert!(state.pop_message().is_none());
        assert!(state.pop_file_batch().is_none());
    }
}
