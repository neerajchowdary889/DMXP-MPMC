use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use crossbeam_utils::CachePadded;
use crate::MPMC::Buffer::layout::{GlobalHeader, MAX_CHANNELS};
use crate::MPMC::Buffer::RingBuffer;
use crate::Core::SharedMemory::{SharedMemoryBackend};
mod debug;
mod getters;

// Use parking_lot's Mutex for better performance
use parking_lot::Mutex;

const MAGIC_NUMBER: u64 = 0x444D58505F4D454D; // "DMXP_MEM"

/// Represents a single channel's memory region
pub struct ChannelPartition {
    /// The underlying ring buffer for this channel
    pub buffer: RingBuffer,
    /// The unique identifier for this channel
    pub channel_id: u32,
}

/// Global allocator for managing shared memory channels
pub struct SharedMemoryAllocator {
    shm: Box<dyn SharedMemoryBackend>,
    header: *mut GlobalHeader,
    next_channel_id: AtomicU64,
    allocation_mutex: Mutex<()>, // For thread-safe channel creation
}

impl SharedMemoryAllocator {

    // Create a new shared memory allocator with the specified total size
    pub fn new(size: usize) -> io::Result<Self> {
        // Ensure the shared memory size is a multiple of the cache line size
        let aligned_size = (size + 127) & !127;  // Align to 128 bytes
        let control_size = std::mem::size_of::<GlobalHeader>();
        
        // Ensure there's enough space after the header
        // --- Validation: Check if size can fit the header ---
        if aligned_size < control_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "SharedMemoryAllocator::new(): size too small to fit header.\n\
                    ├─ Requested size: {size} bytes\n\
                    ├─ Aligned size:   {aligned_size} bytes (128-byte alignment)\n\
                    ├─ Header size:    {control_size} bytes\n\
                    ╰─ Expected: size >= {control_size} bytes (after alignment)"
                ),
            ));
        }
        let data_size = aligned_size - control_size;
        

        // --- Create shared memory ---
        let shm = crate::Core::SharedMemory::create_shared_memory(aligned_size, Some("dmxp_alloc"))
            .map_err(|e| io::Error::new(
                e.kind(),
                format!(
                    "Failed to create shared memory:\n\
                    ├─ Aligned size: {aligned_size}\n\
                    ├─ Header size:  {control_size}\n\
                    ╰─ Error: {e}"
                ),
            ))?;

        // Get a properly aligned pointer to the header
        let header_ptr = shm.as_ptr() as *mut GlobalHeader;
        if (header_ptr as usize) % 128 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Shared memory not properly aligned",
            ));
        }

        // Initialize global header
        unsafe {
            std::ptr::write(header_ptr, GlobalHeader {
                magic: MAGIC_NUMBER,
                version: 1,
                channel_count: 0,
                channels: std::mem::zeroed(),
            });
        }
        
        Ok(Self {
            shm,
            header: header_ptr,
            next_channel_id: AtomicU64::new(0),
            allocation_mutex: Mutex::new(()),
        })
    }
    
    /// Attach to an existing shared memory allocator
    /// 
    /// # Arguments
    /// * [size](cci:1://file:///home/neeraj/Codes/DMXP-MPMC/src/Core/SharedMemory.rs:298:4-300:5) - The expected size of the shared memory region (must match the size used during creation)
    pub fn attach(size: usize) -> io::Result<Self> {
        // Align the size
        let aligned_size = (size + 127) & !127;
        
        // Calculate minimum required size for the header
        let min_required_size = std::mem::size_of::<GlobalHeader>();
        if aligned_size < min_required_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Size too small to fit header: need at least {} bytes", min_required_size),
            ));
        }

        // Attach to shared memory
        let shm = match crate::Core::SharedMemory::attach_shared_memory("dmxp_alloc", aligned_size) {
            Ok(shm) => shm,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Failed to attach to shared memory: {}", e),
                ));
            }
        };

        // Get header pointer and verify alignment
        let header = shm.as_ptr() as *mut GlobalHeader;
        if (header as usize) % 128 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Shared memory not properly aligned",
            ));
        }

        // Verify magic number and size
        unsafe {
            // First verify we can safely read the magic number
            if (*header).magic != MAGIC_NUMBER {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid magic number - shared memory not properly initialized",
                ));
            }

            // Verify the shared memory is large enough for the header
            if shm.size() < min_required_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Shared memory too small for header: need at least {} bytes, got {}",
                        min_required_size,
                        shm.size()
                    ),
                ));
            }

            // Verify size based on channels
            let mut total_size = std::mem::size_of::<GlobalHeader>();
            let channel_count = (*header).channel_count as usize;
            
            // Safety: We've already verified the header is valid and memory is large enough
            for i in 0..channel_count {
                if i >= MAX_CHANNELS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Invalid channel count in header: {}", channel_count),
                    ));
                }
                
                let ch = &(*header).channels[i];
                total_size = (total_size + 127) & !127; // Align
                total_size = total_size.saturating_add((ch.capacity * RingBuffer::slot_stride() as u64) as usize);
                
                // Check for overflow
                if total_size > shm.size() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Shared memory size too small for channels: need at least {} bytes, got {}",
                            total_size,
                            shm.size()
                        ),
                    ));
                }
            }
        }
        
        // Find the next available channel ID
        let next_id = unsafe {
            let header = &*header;
            (0..MAX_CHANNELS)
                .filter(|&i| header.channels[i].capacity > 0)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0) as u64
        };

        Ok(Self {
            shm,
            header,
            next_channel_id: AtomicU64::new(next_id),
            allocation_mutex: Mutex::new(()),
        })
    }
    
    // Create a new channel with the specified capacity
    pub fn create_channel(&self, capacity: usize) -> io::Result<ChannelPartition> {
        // Validate capacity is a power of two and non-zero
        if capacity == 0 || (capacity & (capacity - 1)) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Capacity must be a power of two and greater than zero",
            ));
        }

        let slot_size = RingBuffer::slot_stride();
        let channel_size = (capacity * slot_size + 127) & !127; // Align to 128 bytes

        // Get next available channel ID
        let channel_id = loop {
            let current_id = self.next_channel_id.load(Ordering::Acquire);
            if current_id >= MAX_CHANNELS as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "Maximum number of channels reached",
                ));
            }
            
            // Try to claim this ID
            if self.next_channel_id.compare_exchange_weak(
                current_id,
                current_id + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                break current_id as u32;
            }
        };

        // Use a mutex to prevent multiple threads from allocating overlapping memory
        let _guard = self.allocation_mutex.lock();
        
        // Get channel entry
        let channel = unsafe { &mut (*self.header).channels[channel_id as usize] };
        
        // Check if channel is already in use
        if channel.capacity != 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Channel ID already in use",
            ));
        }

        // Calculate offset for this channel's data
        let control_size = std::mem::size_of::<GlobalHeader>();
        let mut offset = control_size;
        
        // Find the end of the last channel's data
        unsafe {
            for i in 0..(*self.header).channel_count as usize {
                let ch = &(*self.header).channels[i];
                let ch_end = ch.offset as usize + ch.capacity as usize * slot_size;
                offset = offset.max(ch_end);
            }
        }
        
        // Align the offset
        offset = (offset + 127) & !127;
        
        // Check if we have enough space
        if offset + channel_size > self.shm.size() {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "Not enough space in shared memory",
            ));
        }

        // Initialize channel metadata
        channel.offset = offset as u64;
        channel.capacity = capacity as u64;
        channel.mask = (capacity - 1) as u64;
        channel.tail = CachePadded::new(AtomicU64::new(0));
        channel.head = CachePadded::new(AtomicU64::new(0));

        // Initialize ring buffer
        let buffer_ptr = unsafe { self.shm.as_ptr().add(offset) };
        let mut ring_buffer = RingBuffer::new(buffer_ptr, capacity);
        
        // Initialize slots
        unsafe {
            ring_buffer.init_slots();
        }

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

    pub fn used_memory(&self) -> usize {
        let control_size = std::mem::size_of::<GlobalHeader>();
        let mut max_offset = control_size;
        
        unsafe {
            for i in 0..(*self.header).channel_count as usize {
                let ch = &(*self.header).channels[i];
                let ch_end = ch.offset as usize + ch.capacity as usize * RingBuffer::slot_stride();
                max_offset = max_offset.max(ch_end);
            }
        }
        
        max_offset
    }

    pub fn available_memory(&self) -> usize {
        self.shm.size().saturating_sub(self.used_memory())
    }

    // function to remove a channel
    pub fn remove_channel(&self, channel_id: u32) -> io::Result<()> {
        if channel_id >= MAX_CHANNELS as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Channel ID out of bounds",
            ));
        }
        
        let channel = unsafe { &mut (*self.header).channels[channel_id as usize] };
        if channel.capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Channel not initialized",
            ));
        }
        
        // Set capacity to 0 to mark the channel as free
        channel.capacity = 0;
        
        Ok(())
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

