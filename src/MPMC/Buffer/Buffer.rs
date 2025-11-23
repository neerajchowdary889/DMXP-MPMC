// This is the shared round buffer for MPMC - divided by the channels

use super::layout::ChannelEntry;
use crate::MPMC::Structs::Buffer_Structs::MessageMeta;

use std::sync::atomic::AtomicU64;

/// The size of the inline payload per slot.
/// This should be tuned per deployment.
pub const MSG_INLINE: usize = 1024;

/// A single slot in the ring buffer.
///
/// This struct represents the actual data layout in shared memory.
/// It is marked `#[repr(C)]` to ensure a defined and stable memory layout.
#[repr(C, align(64))]
pub struct Slot {
    /// The sequence number of the slot. This is the core of the synchronization.
    /// - A producer claims a `tail` sequence and waits for the `sequence` in
    ///   the target slot to equal `tail`.
    /// - After writing, it sets the `sequence` to `tail + 1`, signaling completion.
    /// - A consumer waits for the `sequence` in its `head` slot to equal
    ///   `head + 1`.
    pub sequence: AtomicU64,

    /// Transport-only metadata (message ID, timestamp, etc.).
    pub meta: MessageMeta,

    /// Opaque byte array payload.
    pub payload: [u8; MSG_INLINE],
}

/// A high-performance, lock-free, multi-producer, multi-consumer (MPMC) ring buffer view.
///
/// This struct is NOT stored in shared memory. It is a transient view that holds
/// pointers to the shared memory region.
///
/// ### Concurrency Design:
/// - **Producers (Enqueue)**: Producers claim a slot by atomically incrementing `tail`.
///   They use the `sequence` field in the `Slot` to know when the slot is free to
///   be written.
/// - **Consumers (Dequeue)**: Consumers claim a message by atomically incrementing `head`.
///   They use the `sequence` field to know when a message has been fully written by a producer.
pub struct RingBuffer {
    /// Pointer to the channel metadata in the control area.
    pub(crate) metadata: *const ChannelEntry,

    /// Pointer to the start of this channel's data band (array of Slots).
    pub(crate) buffer_base: *mut u8,

    /// The capacity of the buffer (number of slots).
    pub(crate) capacity: usize,

    /// A bitmask used to wrap sequence numbers around the buffer.
    /// Calculated as `capacity - 1`.
    pub(crate) mask: usize,
}

unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}
