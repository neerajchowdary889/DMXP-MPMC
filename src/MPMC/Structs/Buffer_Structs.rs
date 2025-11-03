// This is the struct of the bounded lockfree MPMC shard buffer

use std::sync::atomic::AtomicU64;
use crossbeam_utils::CachePadded;

#[repr(C)]
pub(crate) struct MessageHeader {
    pub(crate) length: AtomicU64,

}

#[repr(C)]
pub(crate) struct RoundBuffer {
    
}