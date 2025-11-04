// This is the shared round buffer for MPMC - divided by the channels

use std::sync::atomic::AtomicU64;
use crossbeam_utils::CachePadded;

/// The header for each slot in the ring buffer, based on the Vyukov MPMC design.
///
/// It is marked `#[repr(C)]` to ensure a defined and stable memory layout,
/// which is critical for shared memory and inter-process communication.
#[repr(C)]
pub struct SlotHeader {
    /// The sequence number of the slot. This is the core of the synchronization.
    /// - A producer claims a `tail_cursor` sequence and waits for the `sequence` in
    ///   the target slot to equal `tail_cursor - capacity`.
    /// - After writing, it sets the `sequence` to `tail_cursor`, signaling completion.
    /// - A consumer waits for the `sequence` in its `head_cursor` slot to equal
    ///   `head_cursor + 1`.
    pub sequence: AtomicU64,

    /// The length of the message payload that follows this header.
    pub length: AtomicU64,
}

/// A high-performance, lock-free, multi-producer, multi-consumer (MPMC) ring buffer.
///
/// This is the core data structure for a single message channel. It is designed for
/// extreme speed using a memory-mapped file and a Vyukov-style MPMC algorithm.
///
/// ### Concurrency Design:
/// - **Producers (Enqueue)**: Producers claim a slot by atomically incrementing `tail_cursor`.
///   They use the `sequence` field in the `SlotHeader` to know when the slot is free to
///   be written.
/// - **Consumers (Dequeue)**: Consumers claim a message by atomically incrementing `head_cursor`.
///   They use the `sequence` field to know when a message has been fully written by a producer.
/// - **Cache-Line Padding**: The atomic cursors are wrapped in `CachePadded` to
///   prevent "false sharing," a major performance bottleneck where updates to one
///   atomic from one CPU core invalidate the cache line for another core accessing
///   a different atomic.
#[repr(C)]
pub struct RingBuffer {
    /// A pointer to the beginning of the memory-mapped region.
    /// This buffer stores the `SlotHeader`s and the raw message payloads.
    pub(crate) buffer: *mut u8,

    /// The total size of the `buffer` in bytes. Must be a power of two to allow
    /// for efficient bitwise-AND masking instead of expensive modulo operations.
    pub(crate) capacity: usize,

    /// A bitmask used to wrap sequence numbers around the buffer.
    /// Calculated as `capacity - 1`.
    pub(crate) mask: usize,

    // --- Producer-related fields ---

    /// The sequence number of the next slot to be claimed for writing (enqueue).
    /// This is the primary point of contention for producers.
    pub(crate) tail_cursor: CachePadded<AtomicU64>,

    // --- Consumer-related fields ---

    /// The sequence number of the next slot to be read (dequeue).
    /// This is the primary point of contention for consumers.
    pub(crate) head_cursor: CachePadded<AtomicU64>,
}

