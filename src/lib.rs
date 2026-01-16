// Module naming follows project convention (MPMC = Multi-Producer Multi-Consumer)
#[allow(non_snake_case)]
pub mod MPMC;

// Debug implementations for various types
#[allow(non_snake_case)]
pub mod Debug {
    #[allow(non_snake_case)]
    pub mod StructDebug;
}

#[allow(non_snake_case)]
pub mod Core;

pub mod ffi;
