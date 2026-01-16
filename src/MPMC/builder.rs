use super::{Consumer, Producer};
use crate::Core::alloc::SharedMemoryAllocator;

pub struct ChannelBuilder {
    buffer_size: usize,
    channel_id: u32,
    capacity: usize,
}

impl Default for ChannelBuilder {
    fn default() -> Self {
        Self {
            buffer_size: 128 * 1024 * 1024, // 128MB default
            channel_id: 0,                  // Default channel ID
            capacity: 1024,                 // Default capacity
        }
    }
}

impl ChannelBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    pub fn with_channel_id(mut self, channel_id: u32) -> Self {
        self.channel_id = channel_id;
        self
    }

    pub fn build_producer(self) -> std::io::Result<Producer> {
        // Try to attach to existing shared memory first, create if it doesn't exist
        let allocator = match SharedMemoryAllocator::attach(self.buffer_size) {
            Ok(alloc) => {
                // Shared memory exists, attach to it
                alloc
            }
            Err(_) => {
                // Shared memory doesn't exist, create it
                SharedMemoryAllocator::new(self.buffer_size)?
            }
        };

        // Check if channel already exists, if not create it
        let channel = match allocator.get_channel(self.channel_id) {
            Some(existing_channel) => {
                // Channel exists, reuse it
                existing_channel
            }
            None => {
                // Channel doesn't exist, create a new one
                allocator.create_channel(self.capacity, Some(self.channel_id))?
            }
        };

        Ok(Producer::new(allocator, channel, self.channel_id))
    }

    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn build_consumer(self) -> std::io::Result<Consumer> {
        let allocator = SharedMemoryAllocator::attach(self.buffer_size)?;
        let channel = allocator.get_channel(self.channel_id).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Channel {} not found", self.channel_id),
            )
        })?;
        Ok(Consumer::new(allocator, channel, self.channel_id))
    }
}
