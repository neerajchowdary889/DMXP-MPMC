mod builder;
mod consumer;
mod producer;

pub use builder::ChannelBuilder;
pub use consumer::Consumer;
pub use producer::Producer;

pub mod Buffer {
    pub mod Buffer;
    pub mod Buffer_impl;
    pub mod layout;
    pub use Buffer::{RingBuffer, Slot, MSG_INLINE}; // re-export for stable path
}

pub mod Structs {
    pub mod Buffer_Structs;
    pub use Buffer_Structs::MessageMeta; // re-export for stable path
}
