use std::mem::size_of;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};

use super::layout::ChannelEntry;
use super::Buffer::{RingBuffer, Slot, MSG_INLINE};
use crate::MPMC::Structs::Buffer_Structs::MessageMeta;

impl RingBuffer {
    /// Create a ring buffer view over an existing memory region.
    ///
    /// # Safety
    /// Caller must ensure `metadata` and `buffer_base` are valid pointers to shared memory.
    pub unsafe fn new(metadata: *const ChannelEntry, buffer_base: *mut u8) -> Self {
        let capacity = (*metadata).capacity as usize;
        Self {
            metadata,
            buffer_base,
            capacity,
            mask: capacity - 1,
        }
    }

    /// Size in bytes of one slot stride in memory.
    #[inline]
    pub fn slot_stride() -> usize {
        size_of::<Slot>()
    }

    /// Initialize per-slot sequence numbers to k for k in 0..capacity.
    /// This should ONLY be called by the creator process.
    ///
    /// # Safety
    /// Caller guarantees the underlying memory is allocated and writable.
    pub unsafe fn init_slots(&self) {
        for k in 0..self.capacity {
            let slot = self.slot_mut(k);
            (*slot).sequence.store(k as u64, Relaxed);
        }
    }

    #[inline]
    unsafe fn slot_mut(&self, index: usize) -> *mut Slot {
        let base = self.buffer_base.add(index * Self::slot_stride());
        base as *mut Slot
    }

    /// Enqueue reserves a slot and publishes the message.
    /// Returns the index on success, or None if the ring appears full.
    pub fn enqueue(&self, meta: MessageMeta, payload: &[u8]) -> Option<usize> {
        let meta_ptr = self.metadata;
        // Safety: We assume metadata pointer is valid for the lifetime of the RingBuffer view
        let tail_atomic = unsafe { &(*meta_ptr).tail };

        loop {
            let tail = tail_atomic.load(Relaxed);
            let idx = (tail as usize) & self.mask;
            let slot_ptr = unsafe { self.slot_mut(idx) };
            let seq = unsafe { &(*slot_ptr).sequence }.load(Acquire);
            let dif = seq as i64 - tail as i64;

            if dif == 0 {
                if tail_atomic
                    .compare_exchange_weak(tail, tail + 1, AcqRel, Relaxed)
                    .is_ok()
                {
                    // We own this slot now
                    unsafe {
                        // Write metadata
                        (*slot_ptr).meta = meta;
                        (*slot_ptr).meta.payload_len = payload.len() as u32;

                        // Write payload
                        let len = payload.len().min(MSG_INLINE);
                        ptr::copy_nonoverlapping(
                            payload.as_ptr(),
                            (*slot_ptr).payload.as_mut_ptr(),
                            len,
                        );

                        // Publish
                        (&(*slot_ptr).sequence).store(tail + 1, Release);
                    }
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

    /// Dequeue acquires a ready slot and returns its content.
    /// Returns None if the ring appears empty.
    pub fn dequeue(&self) -> Option<(MessageMeta, Vec<u8>)> {
        let meta_ptr = self.metadata;
        let head_atomic = unsafe { &(*meta_ptr).head };

        loop {
            let head = head_atomic.load(Relaxed);
            let idx = (head as usize) & self.mask;
            let slot_ptr = unsafe { self.slot_mut(idx) };
            let seq = unsafe { &(*slot_ptr).sequence }.load(Acquire);
            let dif = seq as i64 - (head as i64 + 1);

            if dif == 0 {
                if head_atomic
                    .compare_exchange_weak(head, head + 1, AcqRel, Relaxed)
                    .is_ok()
                {
                    let (meta, payload) = unsafe {
                        let meta = (*slot_ptr).meta;
                        let len = meta.payload_len as usize;
                        let mut payload = vec![0u8; len];
                        ptr::copy_nonoverlapping(
                            (*slot_ptr).payload.as_ptr(),
                            payload.as_mut_ptr(),
                            len,
                        );
                        (meta, payload)
                    };

                    // free slot for future producers
                    unsafe {
                        (&(*slot_ptr).sequence).store(head + self.capacity as u64, Release);
                    }
                    return Some((meta, payload));
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
    /// Signal consumers that new data is available
    pub fn signal_consumer(&self) {
        unsafe {
            let signal = &(*self.metadata).signal;
            signal.fetch_add(1, Release);
            crate::Core::futex::futex_wake(signal);
        }
    }

    /// Wait for new data to be available
    pub fn wait_for_data(&self) {
        unsafe {
            let signal = &(*self.metadata).signal;
            let val = signal.load(Acquire);
            crate::Core::futex::futex_wait(signal, val);
        }
    }
}
