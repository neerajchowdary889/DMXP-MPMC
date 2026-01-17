// src/MPMC/consumer.rs

use crate::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A consumer for receiving messages from a shared memory channel.
/// The consumer is responsible for reading messages from the ring buffer
/// and managing the lifecycle of the shared memory region.
pub struct Consumer {
    _allocator: crate::Core::alloc::SharedMemoryAllocator,
    channel: crate::Core::alloc::ChannelPartition,
    channel_id: u32,
    producer_alive: Arc<AtomicBool>,
    last_message_time: std::sync::atomic::AtomicI64,
}

impl Consumer {
    pub(crate) fn new(
        allocator: crate::Core::alloc::SharedMemoryAllocator,
        channel: crate::Core::alloc::ChannelPartition,
        channel_id: u32,
    ) -> Self {
        Self {
            _allocator: allocator,
            channel,
            channel_id,
            producer_alive: Arc::new(AtomicBool::new(true)),
            last_message_time: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// Updates the last message timestamp to now
    fn update_last_message_time(&self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.last_message_time.store(now, Ordering::Release);
    }

    /// Receives a message from the channel if one is available.
    ///
    /// # Returns
    /// * `Ok(Some(data))` if a message was received
    /// * `Ok(None)` if no message is available
    /// * `Err(io::Error)` if the producer has terminated or an error occurred
    pub fn receive(&self) -> std::io::Result<Option<Vec<u8>>> {
        self.receive_with_meta()
            .map(|opt| opt.map(|(_, payload)| payload))
    }

    /// Receives a message and metadata from the channel if one is available.
    pub fn receive_with_meta(&self) -> std::io::Result<Option<(MessageMeta, Vec<u8>)>> {
        let buffer = self.channel.buffer();

        match buffer.dequeue() {
            Some((meta, payload)) => {
                self.update_last_message_time();
                Ok(Some((meta, payload)))
            }
            None => {
                // Check if producer is still alive
                if !self.is_producer_alive() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Producer has terminated",
                    ));
                }
                Ok(None)
            }
        }
    }

    /// Receives a message, blocking until one is available or the producer terminates.
    pub fn receive_blocking(&self) -> std::io::Result<Vec<u8>> {
        self.receive_blocking_with_meta()
            .map(|(_, payload)| payload)
    }

    /// Receives a message and metadata, blocking until one is available.
    pub fn receive_blocking_with_meta(&self) -> std::io::Result<(MessageMeta, Vec<u8>)> {
        let buffer = self.channel.buffer();
        loop {
            match buffer.dequeue() {
                Some((meta, payload)) => {
                    self.update_last_message_time();
                    return Ok((meta, payload));
                }
                None => {
                    if !self.is_producer_alive() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "Producer has terminated",
                        ));
                    }
                    // Wait for signal
                    buffer.wait_for_data();
                }
            }
        }
    }

    /// Checks if the producer is still alive
    fn is_producer_alive(&self) -> bool {
        // If we've received a message recently, assume the producer is still alive
        let last_msg_time = self.last_message_time.load(Ordering::Acquire);
        if last_msg_time > 0 {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // If we've received a message in the last 5 seconds, assume producer is alive
            if now - last_msg_time < 5 {
                return true;
            }
        }

        // Fall back to the keep-alive flag
        self.producer_alive.load(Ordering::Acquire)
    }

    /// Receives a message from the channel, waiting up to the specified timeout.
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait for a message
    ///
    /// # Returns
    /// * `Ok(Some(data))` if a message was received
    /// * `Ok(None)` if the timeout was reached
    /// * `Err(io::Error)` if the producer has terminated or an error occurred
    pub fn receive_timeout(&self, timeout: Duration) -> std::io::Result<Option<Vec<u8>>> {
        self.receive_timeout_with_meta(timeout)
            .map(|opt| opt.map(|(_, payload)| payload))
    }

    /// Receives a message and metadata with timeout.
    pub fn receive_timeout_with_meta(
        &self,
        timeout: Duration,
    ) -> std::io::Result<Option<(MessageMeta, Vec<u8>)>> {
        let start = Instant::now();

        loop {
            match self.receive_with_meta() {
                Ok(Some(data)) => return Ok(Some(data)),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    // Use exponential backoff to reduce CPU usage
                    let elapsed = start.elapsed();
                    let remaining = timeout.saturating_sub(elapsed);
                    let sleep_time = std::cmp::min(remaining, Duration::from_millis(10));
                    std::thread::sleep(sleep_time);
                }
                Err(e) => return Err(e),
            }

            if start.elapsed() >= timeout {
                return Ok(None);
            }
        }
    }

    /// Returns the channel ID for this consumer
    pub fn channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns a reference to the producer alive flag
    ///
    /// This can be used to check if the producer is still alive.
    /// When the producer drops, this flag will be set to false.
    pub fn producer_alive(&self) -> &Arc<AtomicBool> {
        &self.producer_alive
    }
}
