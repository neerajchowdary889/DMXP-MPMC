use crate::mpmc::buffer::SlotHeader;
use crossbeam_utils::CachePadded;
use std::sync::atomic::AtomicU64;

/// The maximum number of channels that can be configured in the shared memory region.
/// This must be a constant to allow for a fixed-size array in the GlobalHeader.
pub const MAX_CHANNELS: usize = 256;

/// Defines the metadata for a single MPMC channel within the global header.
///
/// This struct contains the atomic cursors and layout information necessary
/// to manage one channel's ring buffer. By centralizing these here, we keep
/// the control plane separate from the data plane and optimize memory layout.
#[repr(C)]
pub struct ChannelEntry {
    /// The byte offset from the start of the shared memory region to the
    /// beginning of this channel's data band (its ring buffer).
    pub offset: u64,

    /// The capacity of this channel's ring buffer in number of slots.
    pub capacity: u64,

    /// The bitmask for this channel, calculated as `capacity - 1`.
    pub mask: u64,

    /// The "tail" cursor for producers. Atomically incremented to claim a slot for writing.
    /// Padded to prevent false sharing with adjacent channel metadata.
    pub tail: CachePadded<AtomicU64>,

    /// The "head" cursor for consumers. Atomically incremented to claim a slot for reading.
    /// Padded to prevent false sharing with adjacent channel metadata.
    pub head: CachePadded<AtomicU64>,
}

/// The global header located at the very beginning of the shared memory region.
///
/// It acts as the entry point for any process, containing versioning info
/// and the table of channel entries.
#[repr(C)]
pub struct GlobalHeader {
    /// A "magic number" to identify the memory region as a DMXP-KVCache buffer.
    pub magic: u64,

    /// The version of the memory layout.
    pub version: u32,

    /// The number of channels currently active and configured.
    pub channel_count: u32,

    /// The table of metadata for each channel.
    pub channels: [ChannelEntry; MAX_CHANNELS],
}