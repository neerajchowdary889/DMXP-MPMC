// In src/MPMC/producer.rs
use crate::MPMC::Buffer::MSG_INLINE;
use crate::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// A producer for sending messages through a shared memory channel.
/// The producer is responsible for writing messages to the ring buffer
/// and managing the lifecycle of the shared memory region.
pub struct Producer {
    _allocator: Arc<crate::Core::alloc::SharedMemoryAllocator>,
    channel: crate::Core::alloc::ChannelPartition,
    channel_id: u32,
    keep_alive: Arc<AtomicBool>,
    sequence_counter: Arc<AtomicU64>,
}

impl Clone for Producer {
    fn clone(&self) -> Self {
        Self {
            _allocator: self._allocator.clone(),
            channel: self.channel.clone(),
            channel_id: self.channel_id,
            keep_alive: self.keep_alive.clone(),
            sequence_counter: self.sequence_counter.clone(),
        }
    }
}

impl Producer {
    pub(crate) fn new(
        allocator: Arc<crate::Core::alloc::SharedMemoryAllocator>,
        channel: crate::Core::alloc::ChannelPartition,
        channel_id: u32,
    ) -> Self {
        Self {
            _allocator: allocator,
            channel,
            channel_id,
            keep_alive: Arc::new(AtomicBool::new(true)),
            sequence_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a producer with a shared lifecycle flag.
    /// The `alive_flag` should be the same Arc shared with the Consumer
    /// so the consumer can detect when the producer terminates.
    pub(crate) fn new_with_lifecycle(
        allocator: Arc<crate::Core::alloc::SharedMemoryAllocator>,
        channel: crate::Core::alloc::ChannelPartition,
        channel_id: u32,
        alive_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            _allocator: allocator,
            channel,
            channel_id,
            keep_alive: alive_flag,
            sequence_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Send a batch of messages.
    /// Returns Ok(()) on success, or WouldBlock if the channel is full.
    pub fn send_batch(&self, messages: &[&[u8]]) -> std::io::Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let batch_size = messages.len();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Pre-allocate IDs (gaps on failure are acceptable for now)
        let base_msg_id = self
            .sequence_counter
            .fetch_add(batch_size as u64, Ordering::Relaxed);

        // Prepare metadata objects
        let mut meta_storage: Vec<MessageMeta> = Vec::with_capacity(batch_size);

        for (i, msg) in messages.iter().enumerate() {
            meta_storage.push(MessageMeta {
                message_id: base_msg_id + i as u64,
                timestamp_ns: now,
                channel_id: self.channel_id,
                message_type: 1,
                sender_pid: std::process::id(),
                sender_runtime: 1,
                overflow: self.overflow_flag(msg),
                flags: 0,
                payload_len: msg.len() as u32,
            });
        }

        // Create the slice of references required by enqueue_batch
        let batch_args: Vec<(&MessageMeta, &[u8])> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| (&meta_storage[i], *msg))
            .collect();

        // Attempt enqueue
        if self.channel.buffer().enqueue_batch(&batch_args).is_some() {
            self.channel.buffer().signal_consumer();
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "Channel full or contended",
            ))
        }
    }

    /// Sends a message through the channel.
    ///
    /// Messages smaller than 90% of MSG_INLINE are stored inline in the slot.
    /// Larger messages are automatically overflowed to SFB (Stable Fragmented Buffer).
    ///
    /// # Arguments
    /// * `message` - The message to send
    ///
    /// # Returns
    /// * `Ok(())` if the message was sent successfully
    /// * `Err(io::Error)` if the buffer is full
    pub fn send<T: AsRef<[u8]>>(&self, message: T) -> std::io::Result<()> {
        let message = message.as_ref();

        let buffer = self.channel.buffer();

        // Create metadata
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let overflow = self.overflow_flag(message);

        let meta = MessageMeta {
            message_id: self.sequence_counter.fetch_add(1, Ordering::Relaxed),
            timestamp_ns: now,
            channel_id: self.channel_id,
            message_type: 1,
            sender_pid: std::process::id(),
            sender_runtime: 1,
            overflow,
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

    /// Returns the overflow flag as u8: 1 if the message should be overflowed to SFB, 0 otherwise.
    /// Overflow threshold is 90% of MSG_INLINE.
    fn overflow_flag(&self, message: &[u8]) -> u8 {
        let overflow_threshold = (MSG_INLINE * 90) / 100;
        if message.len() > overflow_threshold {
            1
        } else {
            0
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
}

impl Drop for Producer {
    fn drop(&mut self) {
        // Signal that this producer is terminating
        self.keep_alive.store(false, Ordering::Release);
        // Wake any blocked consumers so they can detect producer death
        self.channel.buffer().signal_consumer();
    }
}
