use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use crossbeam_utils::CachePadded;
use crate::MPMC::Buffer::layout::{GlobalHeader, ChannelEntry, MAX_CHANNELS};
use crate::MPMC::Buffer::RingBuffer;
use crate::Core::SharedMemory::{SharedMemoryBackend, RawHandle};

const MAGIC_NUMBER: u64 = 0x444D58505F4D454D; // "DMXP_MEM"

/// Represents a single channel's memory region
pub struct ChannelPartition {
    buffer: RingBuffer,
    channel_id: u32,
}

/// Global allocator for managing shared memory channels
pub struct SharedMemoryAllocator {
    shm: Box<dyn SharedMemoryBackend>,
    header: *mut GlobalHeader,
    next_channel_id: AtomicU64,
}

impl SharedMemoryAllocator {
    /// Create a new shared memory allocator with the specified total size
    pub fn new(size: usize) -> io::Result<Self> {
        // Calculate total size needed
        let control_size = std::mem::size_of::<GlobalHeader>();
        let data_size = size.saturating_sub(control_size);
        
        // Create shared memory
        let shm = crate::Core::SharedMemory::create_shared_memory(size, Some("dmxp_alloc"))?;
        
        // Initialize global header
        let header = shm.as_ptr() as *mut GlobalHeader;
        unsafe {
            (*header).magic = MAGIC_NUMBER;
            (*header).version = 1;
            (*header).channel_count = 0;
        }
        
        Ok(Self {
            shm,
            header,
            next_channel_id: AtomicU64::new(0),
        })
    }
    
    /// Attach to an existing shared memory allocator
    /// 
    /// # Arguments
    /// * `size` - The expected size of the shared memory region (must match the size used during creation)
    pub fn attach(size: usize) -> io::Result<Self> {
        let shm = crate::Core::SharedMemory::attach_shared_memory("dmxp_alloc", size)?;
        let header = shm.as_ptr() as *mut GlobalHeader;
        
        // Verify magic number
        unsafe {
            if (*header).magic != MAGIC_NUMBER {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid magic number",
                ));
            }
        }
        
        Ok(Self {
            shm,
            header,
            next_channel_id: AtomicU64::new(0), // Will be updated from header
        })
    }
    
    /// Create a new channel with the specified capacity
    pub fn create_channel(&self, capacity: usize) -> io::Result<ChannelPartition> {
        // Calculate required size
        let slot_size = RingBuffer::slot_stride();
        let channel_size = capacity * slot_size;
        
        // Find free channel slot
        let channel_id = self.next_channel_id.fetch_add(1, Ordering::SeqCst) as u32;
        if channel_id >= MAX_CHANNELS as u32 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "Maximum number of channels reached",
            ));
        }
        
        // Get channel entry
        let channel = unsafe { &mut (*self.header).channels[channel_id as usize] };
        
        // Initialize channel metadata
        channel.offset = 0; // Will be set by allocate_region
        channel.capacity = capacity as u64;
        channel.mask = (capacity - 1) as u64;
        channel.tail = CachePadded::new(AtomicU64::new(0));
        channel.head = CachePadded::new(AtomicU64::new(0));
        
        // Allocate memory for the channel
        let buffer_ptr = unsafe { self.shm.as_ptr().add(channel.offset as usize) };
        
        // Initialize ring buffer
        let ring_buffer = RingBuffer::new(buffer_ptr, capacity);
        unsafe { ring_buffer.init_slots(); }
        
        // Update channel count
        unsafe {
            (*self.header).channel_count += 1;
        }
        
        Ok(ChannelPartition {
            buffer: ring_buffer,
            channel_id,
        })
    }
    
    /// Get a channel by ID
    pub fn get_channel(&self, channel_id: u32) -> Option<ChannelPartition> {
        if channel_id >= MAX_CHANNELS as u32 {
            return None;
        }
        
        let channel = unsafe { &(*self.header).channels[channel_id as usize] };
        if channel.capacity == 0 {
            return None; // Channel not initialized
        }
        
        let buffer_ptr = unsafe { self.shm.as_ptr().add(channel.offset as usize) };
        let ring_buffer = RingBuffer::new(buffer_ptr, channel.capacity as usize);
        
        Some(ChannelPartition {
            buffer: ring_buffer,
            channel_id,
        })
    }
}

impl ChannelPartition {
    /// Get the channel ID
    pub fn id(&self) -> u32 {
        self.channel_id
    }
    
    /// Get a reference to the underlying ring buffer
    pub fn buffer(&self) -> &RingBuffer {
        &self.buffer
    }
}

// Implement Send + Sync since we manage synchronization internally
unsafe impl Send for SharedMemoryAllocator {}
unsafe impl Sync for SharedMemoryAllocator {}