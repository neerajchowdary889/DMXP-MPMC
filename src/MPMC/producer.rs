// In src/MPMC/producer.rs
use crate::MPMC::Buffer::MSG_INLINE;
use crate::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// A producer for sending messages through a shared memory channel.
/// The producer is responsible for writing messages to the ring buffer
/// and managing the lifecycle of the shared memory region.
pub struct Producer {
    _allocator: crate::Core::alloc::SharedMemoryAllocator,
    channel: crate::Core::alloc::ChannelPartition,
    channel_id: u32,
    keep_alive: Arc<AtomicBool>,
    max_message_size: usize,
}

impl Producer {
    pub(crate) fn new(
        allocator: crate::Core::alloc::SharedMemoryAllocator,
        channel: crate::Core::alloc::ChannelPartition,
        channel_id: u32,
    ) -> Self {
        // Max message size is fixed by the inline payload size
        let max_message_size = MSG_INLINE;

        Self {
            _allocator: allocator,
            channel,
            channel_id,
            keep_alive: Arc::new(AtomicBool::new(true)),
            max_message_size,
        }
    }

    /// Sends a message through the channel.
    ///
    /// # Arguments
    /// * `message` - The message to send
    ///
    /// # Returns
    /// * `Ok(())` if the message was sent successfully
    /// * `Err(io::Error)` if the message is too large or the buffer is full
    pub fn send<T: AsRef<[u8]>>(&self, message: T) -> std::io::Result<()> {
        let message = message.as_ref();

        // Check message size before attempting to enqueue
        if message.len() > self.max_message_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Message too large ({} > {})",
                    message.len(),
                    self.max_message_size
                ),
            ));
        }

        let buffer = self.channel.buffer();

        // Create metadata
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let meta = MessageMeta {
            message_id: 0, // TODO: Add sequence counter if needed
            timestamp_ns: now,
            channel_id: self.channel_id,
            message_type: 1, // Default type
            sender_pid: std::process::id(),
            sender_runtime: 1, // Rust
            flags: 0,
            payload_len: message.len() as u32,
        };

        match buffer.enqueue(meta, message) {
            Some(_) => {
                buffer.signal_consumer();
                Ok(())
            }
            None => {
                if !self.keep_alive.load(Ordering::Acquire) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Consumer has terminated",
                    ));
                }

                Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "Failed to enqueue message - buffer full",
                ))
            }
        }
    }

    /// Returns the channel ID for this producer
    pub fn channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns a reference to the keep-alive flag
    ///
    /// This can be used to check if the consumer is still alive.
    /// When the consumer drops, this flag will be set to false.
    pub fn keep_alive(&self) -> &Arc<AtomicBool> {
        &self.keep_alive
    }

    /// Returns the maximum message size that can be sent
    pub fn max_message_size(&self) -> usize {
        self.max_message_size
    }
}
