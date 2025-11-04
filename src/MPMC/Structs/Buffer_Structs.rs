// This is the struct of the bounded lockfree MPMC shard buffer

// no atomics in MessageMeta; keep as plain integral types for ABI

/// Transport-only metadata that precedes each payload in a Slot.
/// ABI-stable across languages; all fields are little-endian.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct MessageMeta {
    pub message_id: u64,
    pub timestamp_ns: u64,
    pub channel_id: u32,
    pub message_type: u32,
    pub sender_pid: u32,
    pub sender_runtime: u16,
    pub flags: u16,
    pub payload_len: u32,
}