//! Undo state.
//!
//! Groups message-position and file-content undo stacks so undo concerns
//! have a single owner.

use std::collections::VecDeque;

/// State for undo operations: message-position undo and file-content undo.
#[derive(Debug)]
#[non_exhaustive]
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
        while self.file_stack.len() > 20 {
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
