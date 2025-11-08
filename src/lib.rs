// Module naming follows project convention (MPMC = Multi-Producer Multi-Consumer)
#[allow(non_snake_case)]
pub mod MPMC {
    pub mod Buffer {
        pub mod Buffer;
        pub mod Buffer_impl;
        pub mod layout;
        pub use Buffer::{SlotHeader, RingBuffer}; // re-export for stable path
    }
    pub mod Structs {
        pub mod Buffer_Structs;
        pub use Buffer_Structs::MessageMeta; // re-export for stable path
    }
}
#[allow(non_snake_case)]
pub mod Core {
    pub mod SharedMemory;
    pub use SharedMemory::{SharedMemoryBackend, RawHandle, create_shared_memory, attach_shared_memory};
    pub mod alloc;
}


