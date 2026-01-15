use crate::Core::alloc::SharedMemoryAllocator;
use crate::MPMC::ChannelBuilder;
use crate::MPMC::Consumer;
use crate::MPMC::Producer;
use std::ptr;

// Error codes
const DMXP_SUCCESS: i32 = 0;
const DMXP_ERROR_NULL_POINTER: i32 = -1;
const DMXP_ERROR_INVALID_ARG: i32 = -2;
const DMXP_ERROR_ALLOCATION_FAILED: i32 = -3;
const DMXP_ERROR_CHANNEL_FULL: i32 = -4;
const DMXP_ERROR_EMPTY: i32 = -5;
const DMXP_ERROR_INTERNAL: i32 = -6;

/// Handle to a producer instance (opaque pointer)
pub struct ProducerHandle {
    inner: Producer,
}

/// Handle to a consumer instance (opaque pointer)
pub struct ConsumerHandle {
    inner: Consumer,
}

// -----------------------------------------------------------------------------
// Producer API
// -----------------------------------------------------------------------------

/// Create a new producer.
///
/// # Arguments
/// * `channel_id` - ID of the channel to produce to.
/// * `capacity` - Capacity of the channel (power of 2).
///
/// # Returns
/// * Pointer to `ProducerHandle`, or NULL on failure.
#[no_mangle]
pub extern "C" fn dmxp_producer_new(channel_id: u32, capacity: u32) -> *mut ProducerHandle {
    // Try to attach to existing allocator first (standard size)
    // If it fails, create new (but we usually expect pre-existing shm or create it here)
    // For simplicity, we'll try to attach, if fail, create.
    const SHM_SIZE: usize = 128 * 1024 * 1024; // 128MB default

    // Logic: In a real deploy, usually one process creates the SHM.
    // Here we'll try attach, if fail, create.
    let allocator_res =
        SharedMemoryAllocator::attach(SHM_SIZE).or_else(|_| SharedMemoryAllocator::new(SHM_SIZE));

    match allocator_res {
        Ok(_) => {
            // We don't propagate the allocator instance directly in this simple wrapper
            // because ChannelBuilder handles it internally essentially?
            // Wait, ChannelBuilder uses SharedMemoryAllocator internally?
            // Actually ChannelBuilder::new() creates a fresh allocator instance internally if needed
            // or attaches? Let's check ChannelBuilder.
            // Looking at previous files, ChannelBuilder usage is:
            // ChannelBuilder::new().with_channel_id(id).with_capacity(cap).build_producer()
            // This seems self-contained.

            match ChannelBuilder::new()
                .with_channel_id(channel_id)
                .with_capacity(capacity as usize)
                .build_producer()
            {
                Ok(producer) => {
                    let handle = Box::new(ProducerHandle { inner: producer });
                    Box::into_raw(handle)
                }
                Err(e) => {
                    eprintln!("FFI Error: Failed to build producer: {}", e);
                    ptr::null_mut()
                }
            }
        }
        Err(e) => {
            eprintln!("FFI Error: Failed to init allocator: {}", e);
            ptr::null_mut()
        }
    }
}

/// Send a message.
///
/// # Arguments
/// * `handle` - Pointer to `ProducerHandle`.
/// * `data` - Pointer to data buffer.
/// * `len` - Length of data.
///
/// # Returns
/// * 0 on success, negative error code otherwise.
#[no_mangle]
pub extern "C" fn dmxp_producer_send(
    handle: *mut ProducerHandle,
    data: *const u8,
    len: usize,
) -> i32 {
    if handle.is_null() || data.is_null() {
        return DMXP_ERROR_NULL_POINTER;
    }

    let producer = unsafe { &(*handle).inner };
    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    match producer.send(slice) {
        Ok(_) => DMXP_SUCCESS,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => DMXP_ERROR_CHANNEL_FULL,
        Err(_) => DMXP_ERROR_INTERNAL,
    }
}

/// Free a producer handle.
#[no_mangle]
pub extern "C" fn dmxp_producer_free(handle: *mut ProducerHandle) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle); // Dropped automatically
        }
    }
}

// -----------------------------------------------------------------------------
// Consumer API
// -----------------------------------------------------------------------------

/// Create a new consumer.
///
/// # Arguments
/// * `channel_id` - ID of the channel to consue from.
///
/// # Returns
/// * Pointer to `ConsumerHandle`, or NULL on failure.
#[no_mangle]
pub extern "C" fn dmxp_consumer_new(channel_id: u32) -> *mut ConsumerHandle {
    match ChannelBuilder::new()
        .with_channel_id(channel_id)
        .build_consumer()
    {
        Ok(consumer) => {
            let handle = Box::new(ConsumerHandle { inner: consumer });
            Box::into_raw(handle)
        }
        Err(e) => {
            eprintln!("FFI Error: Failed to build consumer: {}", e);
            ptr::null_mut()
        }
    }
}

/// Receive a message.
///
/// # Arguments
/// * `handle` - Pointer to `ConsumerHandle`.
/// * `blocking` - If true, wait for message. If false, return immediately if empty.
/// * `out_buf` - Buffer to write message into.
/// * `out_len` - Input: size of buf, Output: size of message received.
///
/// # Returns
/// * 0 on success.
/// * DMXP_ERROR_EMPTY if non-blocking and empty.
/// * DMXP_ERROR_INVALID_ARG if buffer too small.
#[no_mangle]
pub extern "C" fn dmxp_consumer_receive(
    handle: *mut ConsumerHandle,
    blocking: bool,
    out_buf: *mut u8,
    out_len: *mut usize,
) -> i32 {
    if handle.is_null() || out_len.is_null() {
        return DMXP_ERROR_NULL_POINTER;
    }

    let consumer = unsafe { &(*handle).inner };
    let max_len = unsafe { *out_len };

    let result = if blocking {
        match consumer.receive_blocking() {
            Ok(data) => Some(data),
            Err(_) => return DMXP_ERROR_INTERNAL,
        }
    } else {
        match consumer.receive() {
            Ok(Some(data)) => Some(data),
            Ok(None) => return DMXP_ERROR_EMPTY,
            Err(_) => return DMXP_ERROR_INTERNAL,
        }
    };

    if let Some(data) = result {
        if data.len() > max_len {
            unsafe { *out_len = data.len() };
            return DMXP_ERROR_INVALID_ARG; // Buffer too small
        }

        if !out_buf.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(data.as_ptr(), out_buf, data.len());
                *out_len = data.len();
            }
        }
        DMXP_SUCCESS
    } else {
        DMXP_ERROR_INTERNAL // Should not happen given logic above
    }
}

/// Free a consumer handle.
#[no_mangle]
pub extern "C" fn dmxp_consumer_free(handle: *mut ConsumerHandle) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle);
        }
    }
}
