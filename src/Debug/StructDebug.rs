use std::fmt;
use std::sync::atomic::Ordering;
use crate::Core::alloc::ChannelPartition;
use crate::MPMC::Buffer::RingBuffer;
use crate::Core::alloc::SharedMemoryAllocator;

/// Debug function for SharedMemoryAllocator
///
/// Provides a safe debug representation that shows:
/// - Header pointer location
/// - Next channel ID
/// - Opaque reference to shared memory
/// - Initialization status
pub fn debug_shared_memory_allocator(allocator: &SharedMemoryAllocator, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("SharedMemoryAllocator")
        .field("shm", &"<opaque>")
        .field("header", &format_args!("{:p}", allocator.header_ptr()))
        .field("next_channel_id", &allocator.next_channel_id())
        .field("initialized", &allocator.is_initialized())
        .finish()
}

/// Debug function for ChannelPartition
///
/// Shows:
/// - Channel ID
/// - Underlying RingBuffer details
pub fn debug_channel_partition(partition: &ChannelPartition, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ChannelPartition")
        .field("channel_id", &partition.channel_id)
        .field("buffer", &format_args!("RingBuffer(0x{:x})", partition.buffer.buffer as usize))
        .finish()
}

/// Debug function for RingBuffer
///
/// Safely displays the buffer's memory location without dereferencing
pub fn debug_ring_buffer(buffer: &RingBuffer, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("RingBuffer")
        .field("buffer", &format_args!("0x{:x}", buffer.buffer as usize))
        .finish_non_exhaustive()
}

// Getter functions