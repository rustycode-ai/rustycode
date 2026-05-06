# rustycode-ringbuf

Bounded SPSC (single-producer single-consumer) ring buffer for the RustyCode workspace.

## Overview

A bounded ring buffer where a single producer thread writes data and a single
consumer thread reads it. The buffer splits into `Producer` and `Consumer`
handles backed by `Arc`-shared storage so each half can be moved across threads
independently.

## Usage

```rust
use rustycode_ringbuf::RingBuffer;

let (tx, rx) = RingBuffer::<u32>::new(4);

// Producer side
tx.push(42).expect("pushed");

// Consumer side
let val = rx.pop().expect("popped");
assert_eq!(val, 42);
```
