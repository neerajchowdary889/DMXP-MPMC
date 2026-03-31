use std::mem::size_of;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::Arc;

use sfb::PinnedBlobStore;

use super::layout::ChannelEntry;
use super::Buffer::{RingBuffer, Slot, MSG_INLINE};
use crate::MPMC::Structs::Buffer_Structs::MessageMeta;

impl RingBuffer {
    /// Create a ring buffer view over an existing memory region.
    ///
    /// # Safety
    /// Caller must ensure `metadata` and `buffer_base` are valid pointers to shared memory.
    /// The `sfu` store must be the SAME `Arc` shared between producer and consumer
    /// RingBuffer instances for overflow to work correctly.
    pub unsafe fn new(metadata: *const ChannelEntry, buffer_base: *mut u8, sfu: Arc<PinnedBlobStore>) -> Self {
        let capacity = (*metadata).capacity as usize;
        Self {
            metadata,
            buffer_base,
            capacity,
            mask: capacity - 1,
            sfu,
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

    /// Enqueue a batch of messages.
    /// Returns the starting index on success, or None if the ring is genuinely full.
    ///
    /// This is "all or nothing" — either the entire batch is claimed, or nothing is.
    /// The implementation correctly distinguishes between:
    /// - **Full** (`dif < 0`): consumer hasn't freed slots → return `None`
    /// - **Stale tail** (`dif > 0`): another producer claimed slots → retry with fresh tail
    /// - **Contention** (CAS failure): another producer won the race → retry
    pub fn enqueue_batch(&self, messages: &[(&MessageMeta, &[u8])]) -> Option<usize> {
        let batch_size = messages.len();
        if batch_size == 0 {
            return Some(0);
        }
        if batch_size > self.capacity {
            return None;
        }

        let meta_ptr = self.metadata;
        let tail_atomic = unsafe { &(*meta_ptr).tail };

        loop {
            let tail = tail_atomic.load(Relaxed);

            // 1. Pre-check: verify all slots in the batch range are available
            let mut is_full = false;
            let mut all_available = true;
            for i in 0..batch_size {
                let target_seq = tail + i as u64;
                let idx = (target_seq as usize) & self.mask;
                let slot_ptr = unsafe { self.slot_mut(idx) };
                let seq = unsafe { &(*slot_ptr).sequence }.load(Acquire);

                let dif = seq as i64 - target_seq as i64;
                if dif < 0 {
                    // Slot genuinely occupied — consumer hasn't freed it yet.
                    is_full = true;
                    all_available = false;
                    break;
                } else if dif > 0 {
                    // Slot sequence is ahead of our expected position.
                    // This means our tail read is stale — another producer already
                    // claimed this slot. Retry from the outer loop with a fresh tail.
                    all_available = false;
                    break;
                }
                // dif == 0 → slot is free for this sequence, continue checking
            }

            if !all_available {
                if is_full {
                    return None; // genuinely full — caller should back off
                }
                // Stale tail read or contention — retry with fresh tail
                std::hint::spin_loop();
                continue;
            }

            // 2. Try to atomically claim the entire batch range
            if tail_atomic
                .compare_exchange_weak(tail, tail + batch_size as u64, AcqRel, Relaxed)
                .is_ok()
            {
                // We own [tail, tail + batch_size). Write each slot.
                for (i, (meta, payload)) in messages.iter().enumerate() {
                    let target_seq = tail + i as u64;
                    let idx = (target_seq as usize) & self.mask;
                    let slot_ptr = unsafe { self.slot_mut(idx) };

                    unsafe {
                        (*slot_ptr).meta = **meta;

                        if (*slot_ptr).meta.is_overflow() {
                            let handle = self.sfu.append_shared(payload).expect("Failed to append to SFU");
                            let handle_bytes = handle.as_bytes();
                            ptr::copy_nonoverlapping(
                                handle_bytes.as_ptr(),
                                (*slot_ptr).payload.as_mut_ptr(),
                                handle_bytes.len(),
                            );
                            (*slot_ptr).meta.payload_len = handle_bytes.len() as u32;
                        } else {
                            (*slot_ptr).meta.payload_len = payload.len() as u32;
                            let len = payload.len().min(MSG_INLINE);
                            ptr::copy_nonoverlapping(
                                payload.as_ptr(),
                                (*slot_ptr).payload.as_mut_ptr(),
                                len,
                            );
                        }

                        // Publish: signal consumers this slot is ready
                        (&(*slot_ptr).sequence).store(target_seq + 1, Release);
                    }
                }
                return Some((tail as usize) & self.mask);
            }
            // CAS failed — another producer won. Retry.
            std::hint::spin_loop();
        }
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

                        // Write payload
                        if (*slot_ptr).meta.is_overflow() {
                            let handle = self.sfu.append_shared(payload).expect("Failed to append to SFU");
                            let handle_bytes = handle.as_bytes();
                            ptr::copy_nonoverlapping(
                                handle_bytes.as_ptr(),
                                (*slot_ptr).payload.as_mut_ptr(),
                                handle_bytes.len(),
                            );
                            (*slot_ptr).meta.payload_len = handle_bytes.len() as u32;
                        } else {
                            (*slot_ptr).meta.payload_len = payload.len() as u32;
                            let len = payload.len().min(MSG_INLINE);
                            ptr::copy_nonoverlapping(
                                payload.as_ptr(),
                                (*slot_ptr).payload.as_mut_ptr(),
                                len,
                            );
                        }

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
                        let mut stored_payload = vec![0u8; len];
                        ptr::copy_nonoverlapping(
                            (*slot_ptr).payload.as_ptr(),
                            stored_payload.as_mut_ptr(),
                            len,
                        );

                        if meta.is_overflow() && len == std::mem::size_of::<sfb::OverflowHandle>() {
                            let handle = sfb::OverflowHandle::from_bytes(&stored_payload[..len])
                                .expect("Invalid OverflowHandle in slot");
                            match self.sfu.resolve(&handle) {
                                Some(data) => {
                                    // Acknowledge the handle for cleanup
                                    self.sfu.acknowledge_shared(&handle);
                                    let mut final_meta = meta;
                                    final_meta.payload_len = data.len() as u32;
                                    (final_meta, data.to_vec())
                                }
                                None => {
                                    eprintln!("Failed to get from SFU: handle expired or invalid");
                                    (meta, vec![])
                                }
                            }
                        } else {
                            (meta, stored_payload)
                        }
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
    // ── Monitoring accessors (read-only, used by TUI) ──────────────────────

    /// Current producer cursor (tail). Used by external monitors to compute fill level.
    pub fn tail(&self) -> u64 {
        unsafe { (*self.metadata).tail.load(Acquire) }
    }

    /// Current consumer cursor (head). Used by external monitors to compute fill level.
    pub fn head(&self) -> u64 {
        unsafe { (*self.metadata).head.load(Acquire) }
    }

    /// Slot capacity of this ring buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    // ─────────────────────────────────────────────────────────────────────────

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

    /// Peeks at the next available message without consuming it.
    /// Returns a copy of the message data, but leaves the message in the ring buffer.
    pub fn peek(&self) -> Option<(MessageMeta, Vec<u8>)> {
        let head_atomic = unsafe { &(*self.metadata).head };
        let tail_atomic = unsafe { &(*self.metadata).tail };

        loop {
            let head = head_atomic.load(Acquire);
            let tail = tail_atomic.load(Acquire);
            let dif = (tail as i64) - (head as i64);

            if dif < 0 {
                // empty
                return None;
            } else if dif >= 0 && dif < self.capacity as i64 {
                // data available
                let idx = (head as usize) & self.mask;
                let slot_ptr = unsafe { self.slot_mut(idx) };

                // Check if producer is finished
                let seq = unsafe { (&(*slot_ptr).sequence).load(Acquire) };
                if seq != head {
                    // producer not finished; retry
                    std::hint::spin_loop();
                    continue;
                }

                unsafe {
                    let meta = (*slot_ptr).meta;
                    let len = meta.payload_len as usize;
                    let mut stored_payload = vec![0u8; len];

                    // Copy payload data
                    ptr::copy_nonoverlapping(
                        (*slot_ptr).payload.as_ptr(),
                        stored_payload.as_mut_ptr(),
                        len,
                    );

                    // Handle SFU overflow case
                    if meta.is_overflow() && len == std::mem::size_of::<sfb::OverflowHandle>() {
                        let handle = sfb::OverflowHandle::from_bytes(&stored_payload[..len])
                            .expect("Invalid OverflowHandle in slot");
                        match self.sfu.resolve(&handle) {
                            Some(data) => {
                                let mut final_meta = meta;
                                final_meta.payload_len = data.len() as u32;
                                return Some((final_meta, data.to_vec()));
                            }
                            None => {
                                eprintln!("Failed to get from SFU during peek: handle expired or invalid");
                                return None;
                            }
                        }
                    } else {
                        return Some((meta, stored_payload));
                    }
                }
            } else {
                // full
                return None;
            }
        }
    }
}
