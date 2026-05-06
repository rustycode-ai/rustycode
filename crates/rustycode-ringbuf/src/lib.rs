// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Bounded SPSC (single-producer / single-consumer) ring buffer.
//!
//! Thread-safe using only `AtomicUsize` — no `Mutex` or `RwLock`.
//! The actual capacity is rounded up to the next power of two so that
//! index wrapping can use a bitmask instead of modular arithmetic.
//!
//! # Example
//!
//! ```
//! use rustycode_ringbuf::RingBuffer;
//!
//! let buf = RingBuffer::<u32>::new(4);
//!
//! assert!(buf.push(10));
//! assert!(buf.push(20));
//! assert_eq!(buf.pop(), Some(10));
//! assert_eq!(buf.pop(), Some(20));
//! assert_eq!(buf.pop(), None);
//! ```

// This crate requires unsafe for the ring-buffer slot protocol.
#![allow(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::indexing_slicing
    )
)]

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// RingBuffer
// ---------------------------------------------------------------------------

/// A bounded SPSC ring buffer.
///
/// A single producer calls [`push`] and a single consumer calls [`pop`].
/// Both methods are `&self` (shared reference) and thread-safe through
/// atomic operations on the head and tail indices.
pub struct RingBuffer<T> {
    /// Power-of-two sized backing storage of potentially-uninitialized slots.
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Mask for wrapping indices: `index & mask` maps into `buffer`.
    mask: usize,
    /// Read index — only the consumer updates this.
    head: AtomicUsize,
    /// Write index — only the producer updates this.
    tail: AtomicUsize,
}

// SAFETY: The SPSC contract guarantees that `push` and `pop` never access the
// same slot concurrently.  `push` writes to slots[tail] exclusively;
// `pop` reads from slots[head] exclusively.  Atomic `head`/`tail` updates
// provide the necessary happens-before relationships.
unsafe impl<T: Send> Send for RingBuffer<T> {}
unsafe impl<T: Send> Sync for RingBuffer<T> {}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with at least `capacity` slots.
    ///
    /// The actual capacity is rounded up to the next power of two.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        let actual = capacity.next_power_of_two();
        let mask = actual - 1;

        // Allocate uninitialised storage.  Slots are written by `push` before
        // they are ever read by `pop`, so this is safe.
        let buffer: Vec<UnsafeCell<MaybeUninit<T>>> = (0..actual)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();

        Self {
            buffer: buffer.into_boxed_slice(),
            mask,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Returns the actual capacity (always a power of two).
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Push `item` into the buffer.
    ///
    /// Returns `true` on success, `false` if the buffer is full.
    pub fn push(&self, item: T) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        let len = tail.wrapping_sub(head);

        if len > self.mask {
            return false;
        }

        // SAFETY: tail is owned by the producer (only `push` updates it).
        // `tail - head <= mask` guarantees the slot is not in use by `pop`.
        let slot = unsafe { self.buffer.get_unchecked(tail & self.mask) };
        unsafe { (*slot.get()).write(item) };

        // Publish the new tail so the consumer can see the item.
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    /// Pop the oldest item from the buffer, or `None` if empty.
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        // SAFETY: head is owned by the consumer (only `pop` updates it).
        // `head != tail` guarantees the slot was written by a preceding `push`.
        let slot = unsafe { self.buffer.get_unchecked(head & self.mask) };
        let value = unsafe { (*slot.get()).assume_init_read() };

        // Publish the new head so the producer can reclaim the slot.
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// Returns the number of items currently in the buffer.
    ///
    /// This is a best-effort snapshot and may be stale if the other side is
    /// concurrent.
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Returns `true` if the buffer contains no items.
    ///
    /// Same concurrency caveat as [`len`](Self::len).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        // Drop any remaining items to avoid leaks.
        let head = *self.head.get_mut();
        let tail = *self.tail.get_mut();
        for i in head..tail {
            let slot = &self.buffer[i & self.mask];
            // SAFETY: slots from head..tail contain valid, initialised items.
            unsafe { (*slot.get()).assume_init_drop() };
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // -- basic functionality --

    #[test]
    fn new_capacity_is_power_of_two() {
        let buf = RingBuffer::<u8>::new(1);
        assert_eq!(buf.capacity(), 1);

        let buf = RingBuffer::<u8>::new(3);
        assert_eq!(buf.capacity(), 4);

        let buf = RingBuffer::<u8>::new(5);
        assert_eq!(buf.capacity(), 8);
    }

    #[test]
    fn push_then_pop() {
        let buf = RingBuffer::<u32>::new(4);
        assert!(buf.push(10));
        assert!(buf.push(20));
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.pop(), Some(10));
        assert_eq!(buf.pop(), Some(20));
        assert!(buf.is_empty());
        assert_eq!(buf.pop(), None);
    }

    #[test]
    fn push_returns_false_when_full() {
        let buf = RingBuffer::<u8>::new(2);
        assert!(buf.push(1));
        assert!(buf.push(2));
        // capacity is 2, so the third push must fail.
        assert!(!buf.push(3));
    }

    #[test]
    fn pop_returns_none_when_empty() {
        let buf = RingBuffer::<u8>::new(4);
        assert_eq!(buf.pop(), None);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    // -- wrapping indices --

    #[test]
    fn index_wrap_around() {
        let buf = RingBuffer::<u8>::new(2);
        // Push/pop many times to force head/tail to wrap past usize::MAX.
        for i in 0..30u8 {
            assert!(buf.push(i), "push({i}) should succeed");
            assert_eq!(buf.pop(), Some(i), "pop should yield {i}");
        }
    }

    // -- reuse after draining --

    #[test]
    fn reuse_after_full_drain() {
        let buf = RingBuffer::<u8>::new(2);
        buf.push(1);
        buf.push(2);
        assert_eq!(buf.pop(), Some(1));
        assert_eq!(buf.pop(), Some(2));
        // Slots are now free.
        assert!(buf.push(3));
        assert!(buf.push(4));
        assert!(!buf.push(5));
        assert_eq!(buf.pop(), Some(3));
        assert_eq!(buf.pop(), Some(4));
    }

    // -- drop correctness --

    #[test]
    fn remaining_items_dropped() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Tracker(Rc<Cell<usize>>);
        impl Drop for Tracker {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let counter = Rc::new(Cell::new(0usize));

        {
            let buf = RingBuffer::new(4);
            buf.push(Tracker(counter.clone()));
            buf.push(Tracker(counter.clone()));
            // Don't pop — let Drop clean up.
        }

        assert_eq!(counter.get(), 2, "both items must be dropped");
    }

    // -- zero-capacity panic --

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics() {
        let _ = RingBuffer::<u8>::new(0);
    }

    // -- concurrent SPSC test --

    #[test]
    fn concurrent_spsc() {
        const N: u64 = 10_000;
        let buf = Arc::new(RingBuffer::<u64>::new(64));

        let producer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..N {
                    while !buf.push(i) {
                        std::thread::yield_now();
                    }
                }
            })
        };

        let consumer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                let mut received = Vec::with_capacity(N as usize);
                while received.len() < N as usize {
                    match buf.pop() {
                        Some(v) => received.push(v),
                        None => std::thread::yield_now(),
                    }
                }
                received
            })
        };

        producer.join().unwrap();
        let received = consumer.join().unwrap();
        let expected: Vec<u64> = (0..N).collect();
        assert_eq!(received, expected);
    }

    // -- concurrent: verify ordering is preserved --

    #[test]
    fn concurrent_ordering_preserved() {
        const N: u64 = 5_000;
        let buf = Arc::new(RingBuffer::<u64>::new(8));

        let producer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..N {
                    while !buf.push(i) {
                        std::thread::yield_now();
                    }
                }
            })
        };

        let consumer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                let mut prev = None;
                let mut count = 0u64;
                loop {
                    match buf.pop() {
                        Some(v) => {
                            if let Some(p) = prev {
                                assert!(v > p, "out-of-order: prev={p}, current={v}");
                            }
                            prev = Some(v);
                            count += 1;
                            if count == N {
                                return;
                            }
                        }
                        None => std::thread::yield_now(),
                    }
                }
            })
        };

        producer.join().unwrap();
        consumer.join().unwrap();
    }

    // -- concurrent with string payload --

    #[test]
    fn concurrent_strings() {
        const N: usize = 2_000;
        let buf = Arc::new(RingBuffer::<String>::new(16));

        let producer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..N {
                    let mut msg = format!("msg-{i}");
                    while !buf.push(msg) {
                        msg = format!("msg-{i}");
                        std::thread::yield_now();
                    }
                }
            })
        };

        let consumer = {
            let buf = Arc::clone(&buf);
            std::thread::spawn(move || {
                let mut received = Vec::with_capacity(N);
                while received.len() < N {
                    match buf.pop() {
                        Some(s) => received.push(s),
                        None => std::thread::yield_now(),
                    }
                }
                received
            })
        };

        producer.join().unwrap();
        let received = consumer.join().unwrap();
        assert_eq!(received.len(), N);
        for (i, s) in received.iter().enumerate() {
            assert_eq!(s, &format!("msg-{i}"));
        }
    }

    // -- len/is_empty consistency --

    #[test]
    fn len_matches_pushes_and_pops() {
        let buf = RingBuffer::<u8>::new(8);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);

        for i in 0..8u8 {
            assert!(buf.push(i));
            assert_eq!(buf.len(), usize::from(i) + 1);
        }
        assert!(!buf.is_empty());

        for i in 0..8u8 {
            assert_eq!(buf.pop(), Some(i));
            assert_eq!(buf.len(), 7 - usize::from(i));
        }
        assert!(buf.is_empty());
    }
}
