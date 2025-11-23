// Module naming follows project convention (MPMC = Multi-Producer Multi-Consumer)
#[allow(non_snake_case)]
pub mod MPMC;

// Debug implementations for various types
pub mod Debug {
    pub mod StructDebug;
}


#[allow(non_snake_case)]
pub mod Core {
    pub mod SharedMemory;
    pub use SharedMemory::{SharedMemoryBackend, RawHandle, create_shared_memory, attach_shared_memory};
    pub mod alloc;
}
