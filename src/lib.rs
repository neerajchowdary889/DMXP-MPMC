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


