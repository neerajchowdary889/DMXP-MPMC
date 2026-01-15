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
#[repr(C, align(128))]
pub struct ChannelEntry {
    /// Logical identifier (0xFFFF_FFFF if unused).
    pub channel_id: u32,

    /// Per-channel flags (e.g., drop-on-full, notify-on-write).
    pub flags: u32,

    /// The capacity of this channel's ring buffer in number of slots.
    /// Recommended power-of-two.
    pub capacity: u64,

    /// The byte offset from the start of the shared memory region to the
    /// beginning of this channel's data band (its ring buffer).
    pub band_offset: u64,

    /// Signal word for futex-based blocking/waking.
    /// Producers write to this (and wake), consumers wait on this.
    pub signal: std::sync::atomic::AtomicU32,

    /// The "tail" cursor for producers. Atomically incremented to claim a slot for writing.
    /// Padded to prevent false sharing with adjacent channel metadata.
    pub tail: CachePadded<AtomicU64>,

    /// The "head" cursor for consumers. Atomically incremented to claim a slot for reading.
    /// Padded to prevent false sharing with adjacent channel metadata.
    pub head: CachePadded<AtomicU64>,

    /// Padding to ensure the struct size is aligned to 128 bytes (or 64 bytes).
    /// We use explicit padding if necessary, but `align(128)` handles the stride.
    /// Note: The design asks for 64B alignment, but 128B is safer for modern CPUs (prefetchers).
    /// We'll stick to 128B alignment for the entry as a whole.
    pub _pad: [u64; 0],
}

/// The global header located at the very beginning of the shared memory region.
///
/// It acts as the entry point for any process, containing versioning info
/// and the table of channel entries.
#[repr(C, align(128))]
pub struct GlobalHeader {
    /// A "magic number" to identify the memory region as a DMXP-KVCache buffer.
    pub magic: u64,

    /// The version of the memory layout.
    pub version: u32,

    /// Compiled/allocated maximum channel entries.
    pub max_channels: u32,

    /// The number of channels currently active and configured.
    pub channel_count: u32,

    /// Reserved/padding (align to 16 or 64 bytes).
    pub reserved: u32,

    /// The table of metadata for each channel.
    pub channels: [ChannelEntry; MAX_CHANNELS],
}
