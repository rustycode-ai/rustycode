//! Fixed-size circular buffer with automatic eviction of oldest items.
//!
//! Provides a rolling window of data — metrics, log entries, recent messages, etc.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_protocol::circular_buffer::CircularBuffer;
//!
//! let mut buf: CircularBuffer<i32> = CircularBuffer::new(3);
//! buf.push(1);
//! buf.push(2);
//! buf.push(3);
//! assert_eq!(buf.to_vec(), vec![1, 2, 3]);
//!
//! // Oldest item evicted
//! buf.push(4);
//! assert_eq!(buf.to_vec(), vec![2, 3, 4]);
//! ```

/// A fixed-size circular buffer that automatically evicts the oldest items
/// when the buffer is full.
#[derive(Debug, Clone)]
pub struct CircularBuffer<T> {
    buffer: Vec<Option<T>>,
    head: usize,
    len: usize,
    capacity: usize,
}

impl<T> CircularBuffer<T> {
    /// Create a new circular buffer with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "CircularBuffer capacity must be > 0");
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(None);
        }
        Self {
            buffer,
            head: 0,
            len: 0,
            capacity,
        }
    }

    /// Add an item to the buffer. If the buffer is full,
    /// the oldest item will be evicted.
    pub fn push(&mut self, item: T) {
        self.buffer[self.head] = Some(item);
        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len = self.len.saturating_add(1);
        }
    }

    /// Add multiple items to the buffer at once.
    pub fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.push(item);
        }
    }

    /// Get the most recent `count` items from the buffer,
    /// in insertion order (oldest first).
    ///
    /// Returns fewer items if the buffer contains less than `count`.
    pub fn recent(&self, count: usize) -> Vec<&T> {
        let available = count.min(self.len);
        let start = if self.len < self.capacity {
            0
        } else {
            self.head
        };

        let mut result = Vec::with_capacity(available);
        for i in 0..available {
            let index = (start + self.len - available + i) % self.capacity;
            if let Some(ref item) = self.buffer[index] {
                result.push(item);
            }
        }
        result
    }

    /// Get the most recent item, if any.
    pub fn last(&self) -> Option<&T> {
        if self.len == 0 {
            return None;
        }
        let index = (self.head + self.capacity - 1) % self.capacity;
        self.buffer[index].as_ref()
    }

    /// Get all items currently in the buffer, in insertion order
    /// (oldest to newest).
    pub fn to_vec(&self) -> Vec<&T> {
        if self.len == 0 {
            return Vec::new();
        }
        let start = if self.len < self.capacity {
            0
        } else {
            self.head
        };

        let mut result = Vec::with_capacity(self.len);
        for i in 0..self.len {
            let index = (start + i) % self.capacity;
            if let Some(ref item) = self.buffer[index] {
                result.push(item);
            }
        }
        result
    }

    /// Clear all items from the buffer.
    pub fn clear(&mut self) {
        for slot in &mut self.buffer {
            *slot = None;
        }
        self.head = 0;
        self.len = 0;
    }

    /// Get the current number of items in the buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Check if the buffer is at capacity.
    pub fn is_full(&self) -> bool {
        self.len == self.capacity
    }

    /// Get the capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let buf: CircularBuffer<i32> = CircularBuffer::new(5);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 5);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics() {
        let _: CircularBuffer<i32> = CircularBuffer::new(0);
    }

    #[test]
    fn push_and_read_back() {
        let mut buf = CircularBuffer::new(3);
        buf.push(1);
        buf.push(2);
        buf.push(3);
        assert_eq!(buf.to_vec(), vec![&1, &2, &3]);
    }

    #[test]
    fn evicts_oldest_when_full() {
        let mut buf = CircularBuffer::new(3);
        buf.push(1);
        buf.push(2);
        buf.push(3);
        buf.push(4); // evicts 1
        assert_eq!(buf.to_vec(), vec![&2, &3, &4]);
        assert_eq!(buf.len(), 3);
        assert!(buf.is_full());
    }

    #[test]
    fn recent_returns_last_n() {
        let mut buf = CircularBuffer::new(5);
        buf.extend([10, 20, 30, 40, 50]);
        let recent = buf.recent(3);
        assert_eq!(recent, vec![&30, &40, &50]);
    }

    #[test]
    fn recent_returns_fewer_if_not_enough() {
        let mut buf = CircularBuffer::new(5);
        buf.push(1);
        buf.push(2);
        let recent = buf.recent(5);
        assert_eq!(recent, vec![&1, &2]);
    }

    #[test]
    fn last_returns_newest() {
        let mut buf = CircularBuffer::new(3);
        buf.push(1);
        buf.push(2);
        assert_eq!(buf.last(), Some(&2));
    }

    #[test]
    fn last_returns_none_when_empty() {
        let buf: CircularBuffer<i32> = CircularBuffer::new(3);
        assert_eq!(buf.last(), None);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut buf = CircularBuffer::new(3);
        buf.push(1);
        buf.push(2);
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn wraps_around_multiple_times() {
        let mut buf = CircularBuffer::new(2);
        for i in 1..=10 {
            buf.push(i);
        }
        assert_eq!(buf.to_vec(), vec![&9, &10]);
    }

    #[test]
    fn extend_adds_multiple() {
        let mut buf = CircularBuffer::new(5);
        buf.extend([1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.to_vec(), vec![&1, &2, &3]);
    }

    #[test]
    fn extend_with_overflow() {
        let mut buf = CircularBuffer::new(3);
        buf.extend([1, 2, 3, 4, 5]);
        assert_eq!(buf.to_vec(), vec![&3, &4, &5]);
    }

    #[test]
    fn single_capacity_buffer() {
        let mut buf = CircularBuffer::new(1);
        buf.push(1);
        assert_eq!(buf.last(), Some(&1));
        buf.push(2);
        assert_eq!(buf.last(), Some(&2));
        assert_eq!(buf.to_vec(), vec![&2]);
    }
}
