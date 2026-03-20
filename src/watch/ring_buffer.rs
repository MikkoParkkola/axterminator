//! A fixed-capacity ring buffer for small typed items.
//!
//! This is used exclusively for the watch event text log — a bounded,
//! in-memory record of the last N events emitted by the watchers.
//! Binary sensor data (audio samples, camera frames) is **not** stored
//! here; it is processed in-place and immediately discarded.
//!
//! ## Behaviour
//!
//! When the buffer is full, `push` silently evicts the oldest entry before
//! inserting the new one.  The capacity is measured in item count, not bytes,
//! because all items are short strings (< 1 KB each).
//!
//! ## Examples
//!
//! ```rust
//! use axterminator::watch::ring_buffer::RingBuffer;
//!
//! let mut buf: RingBuffer<String> = RingBuffer::new(3);
//! buf.push("a".to_string());
//! buf.push("b".to_string());
//! buf.push("c".to_string());
//! buf.push("d".to_string()); // evicts "a"
//! assert_eq!(buf.len(), 3);
//! assert_eq!(buf.latest(), Some(&"d".to_string()));
//! ```

use std::collections::VecDeque;

/// A fixed-capacity FIFO ring buffer that evicts the oldest entry when full.
///
/// Type parameter `T` must be `Sized`.  Items are owned — the buffer takes
/// full ownership on push and gives it back on drain.
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a ring buffer that holds at most `capacity` items.
    ///
    /// # Panics
    ///
    /// Panics when `capacity == 0`.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "RingBuffer capacity must be > 0");
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Insert `item`, evicting the oldest entry when the buffer is full.
    pub fn push(&mut self, item: T) {
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    /// Return a reference to the most recently pushed item, or `None` when empty.
    #[must_use]
    pub fn latest(&self) -> Option<&T> {
        self.buffer.back()
    }

    /// Drain all items in insertion order (oldest first).
    pub fn drain_all(&mut self) -> Vec<T> {
        self.buffer.drain(..).collect()
    }

    /// Number of items currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Return `true` when no items are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Maximum number of items this buffer can hold.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_starts_empty() {
        // GIVEN: freshly created buffer
        let buf: RingBuffer<u32> = RingBuffer::new(4);
        // THEN: empty, len 0
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert!(buf.latest().is_none());
    }

    #[test]
    fn ring_buffer_push_below_capacity_retains_all_items() {
        // GIVEN: capacity 4, push 3 items
        let mut buf = RingBuffer::new(4);
        buf.push(1u32);
        buf.push(2);
        buf.push(3);
        // THEN: all retained
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.latest(), Some(&3));
    }

    #[test]
    fn ring_buffer_push_at_capacity_evicts_oldest() {
        // GIVEN: capacity 3, push 4 items
        let mut buf = RingBuffer::new(3);
        buf.push("a");
        buf.push("b");
        buf.push("c");
        buf.push("d"); // evicts "a"
                       // THEN: len is still 3, oldest is "b"
        assert_eq!(buf.len(), 3);
        let items = buf.drain_all();
        assert_eq!(items, vec!["b", "c", "d"]);
    }

    #[test]
    fn ring_buffer_drain_all_empties_the_buffer() {
        // GIVEN: buffer with 2 items
        let mut buf = RingBuffer::new(10);
        buf.push(10u32);
        buf.push(20);
        // WHEN: drained
        let items = buf.drain_all();
        // THEN: items returned, buffer empty
        assert_eq!(items, vec![10, 20]);
        assert!(buf.is_empty());
    }

    #[test]
    fn ring_buffer_capacity_reported_correctly() {
        // GIVEN: capacity 7
        let buf: RingBuffer<String> = RingBuffer::new(7);
        assert_eq!(buf.capacity(), 7);
    }

    #[test]
    fn ring_buffer_latest_tracks_most_recent_push() {
        // GIVEN: push sequence that overflows
        let mut buf = RingBuffer::new(2);
        buf.push(100u32);
        buf.push(200);
        buf.push(300); // evicts 100
                       // THEN: latest is most recent
        assert_eq!(buf.latest(), Some(&300));
    }

    #[test]
    fn ring_buffer_eviction_count_matches_overflow_count() {
        // GIVEN: capacity 3, insert 10 items
        let mut buf = RingBuffer::new(3);
        for i in 0u32..10 {
            buf.push(i);
        }
        // THEN: exactly 3 items remain (items 7, 8, 9)
        assert_eq!(buf.len(), 3);
        let items = buf.drain_all();
        assert_eq!(items, vec![7, 8, 9]);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn ring_buffer_zero_capacity_panics() {
        let _: RingBuffer<u8> = RingBuffer::new(0);
    }

    #[test]
    fn ring_buffer_memory_stays_bounded_under_sustained_load() {
        // GIVEN: capacity 100, push 10_000 items
        let mut buf = RingBuffer::new(100);
        for i in 0u32..10_000 {
            buf.push(i);
        }
        // THEN: never exceeds capacity
        assert_eq!(buf.len(), 100);
        // AND: contains the last 100 items
        let items = buf.drain_all();
        assert_eq!(items[0], 9_900);
        assert_eq!(items[99], 9_999);
    }
}
