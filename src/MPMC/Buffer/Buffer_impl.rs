use std::mem::size_of;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release, AcqRel};

use super::Buffer::{RingBuffer, SlotHeader};

impl RingBuffer {
    /// Create a ring buffer view over an existing memory region.
    /// `capacity` must be a power of two and represents number of slots.
    pub fn new(buffer: *mut u8, capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "capacity must be power of two");
        Self {
            buffer,
            capacity,
            mask: capacity - 1,
            tail_cursor: crossbeam_utils::CachePadded::new(std::sync::atomic::AtomicU64::new(0)),
            head_cursor: crossbeam_utils::CachePadded::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Size in bytes of one slot header stride in memory.
    #[inline]
    pub fn slot_stride() -> usize { size_of::<SlotHeader>() }

    /// Initialize per-slot sequence numbers to k for k in 0..capacity.
    /// Safety: caller guarantees the underlying memory is at least capacity * slot_stride bytes.
    pub unsafe fn init_slots(&self) {
        for k in 0..self.capacity { self.slot_header_mut(k).sequence.store(k as u64, Relaxed); }
    }

    #[inline]
    unsafe fn slot_header_mut(&self, index: usize) -> &mut SlotHeader {
        let base = self.buffer.add(index * Self::slot_stride());
        &mut *(base as *mut SlotHeader)
    }

    /// Enqueue reserves a slot and publishes written length into the header.
    /// Returns the index on success, or None if the ring appears full at the moment.
    pub fn enqueue(&self, length: u64) -> Option<usize> {
        loop {
            let tail = self.tail_cursor.load(Relaxed);
            let idx = (tail as usize) & self.mask;
            let seq = unsafe { self.slot_header_mut(idx) }.sequence.load(Acquire);
            let dif = seq as i64 - tail as i64;
            if dif == 0 {
                if self.tail_cursor.compare_exchange_weak(tail, tail + 1, AcqRel, Relaxed).is_ok() {
                    // We own this slot now
                    let slot = unsafe { self.slot_header_mut(idx) };
                    slot.length.store(length, Relaxed);
                    // publish
                    slot.sequence.store(tail + 1, Release);
                    return Some(idx);
                }
                continue;
            } else if dif < 0 {
                // full
                return None;
            } else {
                // someone else is producing; backoff and retry
                std::hint::spin_loop();
                continue;
            }
        }
    }

    /// Dequeue acquires a ready slot and returns its index and recorded length.
    /// Returns None if the ring appears empty at the moment.
    pub fn dequeue(&self) -> Option<(usize, u64)> {
        loop {
            let head = self.head_cursor.load(Relaxed);
            let idx = (head as usize) & self.mask;
            let seq = unsafe { self.slot_header_mut(idx) }.sequence.load(Acquire);
            let dif = seq as i64 - (head as i64 + 1);
            if dif == 0 {
                if self.head_cursor.compare_exchange_weak(head, head + 1, AcqRel, Relaxed).is_ok() {
                    let slot = unsafe { self.slot_header_mut(idx) };
                    let len = slot.length.load(Relaxed);
                    // free slot for future producers
                    slot.sequence.store(head + self.capacity as u64, Release);
                    return Some((idx, len));
                }
                continue;
            } else if dif < 0 {
                // empty
                return None;
            } else {
                // producer not finished; retry
                std::hint::spin_loop();
                continue;
            }
        }
    }
}

unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}