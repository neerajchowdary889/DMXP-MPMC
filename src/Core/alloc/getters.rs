use super::*;
use std::sync::atomic::Ordering;

/// Extension trait providing getter methods for SharedMemoryAllocator
/// 
/// These methods provide safe access to the private fields of SharedMemoryAllocator
/// for debugging and monitoring purposes.
impl SharedMemoryAllocator {
    /// Get a reference to the underlying shared memory backend
    /// 
    /// # Safety
    /// The returned reference must not outlive the SharedMemoryAllocator
    pub unsafe fn shm(&self) -> &dyn SharedMemoryBackend {
        &*self.shm
    }

    /// Get the raw pointer to the GlobalHeader
    /// 
    /// # Safety
    /// The caller must ensure the pointer is valid for the duration of its use
    pub fn header_ptr(&self) -> *const GlobalHeader {
        self.header
    }

    /// Get the next available channel ID
    /// 
    /// Returns the next channel ID that will be assigned to a new channel.
    /// The value is loaded with relaxed ordering since this is primarily used
    /// for debugging and monitoring.
    pub fn next_channel_id(&self) -> u64 {
        self.next_channel_id.load(Ordering::Relaxed)
    }

    /// Check if the allocator has been properly initialized
    /// 
    /// Returns true if the magic number in the header matches the expected value.
    /// This can be used to verify the allocator is attached to valid shared memory.
    pub fn is_initialized(&self) -> bool {
        // Safety: We assume the header pointer is valid if the allocator exists
        unsafe { !self.header.is_null() && (*self.header).magic == super::MAGIC_NUMBER }
    }
}
