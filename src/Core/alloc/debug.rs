use super::*;
use std::fmt;

// Debug proxy implementations that call the standalone debug functions
impl fmt::Debug for SharedMemoryAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::Debug::StructDebug::debug_shared_memory_allocator(self, f)
    }
}

impl fmt::Debug for ChannelPartition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::Debug::StructDebug::debug_channel_partition(self, f)
    }
}

impl fmt::Debug for RingBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::Debug::StructDebug::debug_ring_buffer(self, f)
    }
}
