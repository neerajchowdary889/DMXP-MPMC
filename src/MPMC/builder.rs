use super::{Consumer, Producer};
use crate::Core::alloc::SharedMemoryAllocator;

pub struct ChannelBuilder {
    buffer_size: usize,
    channel_id: u32,
}

impl Default for ChannelBuilder {
    fn default() -> Self {
        Self {
            buffer_size: 128 * 1024 * 1024, // 128MB default
            channel_id: 0, // Default channel ID
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
        let allocator = SharedMemoryAllocator::new(self.buffer_size)?;
        let channel = allocator.create_channel(1024)?; // 1024 slots
        Ok(Producer::new(allocator, channel, self.channel_id))
    }

    pub fn build_consumer(self) -> std::io::Result<Consumer> {
        let allocator = SharedMemoryAllocator::attach(self.buffer_size)?;
        let channel = allocator.get_channel(self.channel_id)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Channel {} not found", self.channel_id)
            ))?;
        Ok(Consumer::new(allocator, channel, self.channel_id))
    }
}