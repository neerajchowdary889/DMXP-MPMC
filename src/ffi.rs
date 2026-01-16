use crate::Core::alloc::SharedMemoryAllocator;
use crate::MPMC::ChannelBuilder;
use crate::MPMC::Consumer;
use crate::MPMC::Producer;
use std::ptr;

// Error codes
const DMXP_SUCCESS: i32 = 0;
const DMXP_ERROR_NULL_POINTER: i32 = -1;
const DMXP_ERROR_INVALID_ARG: i32 = -2;
const DMXP_ERROR_CHANNEL_FULL: i32 = -4;
const DMXP_ERROR_EMPTY: i32 = -5;
const DMXP_ERROR_INTERNAL: i32 = -6;
const DMXP_ERROR_TIMEOUT: i32 = -7;

/// Handle to a producer instance (opaque pointer)
pub struct ProducerHandle {
    inner: Producer,
}

/// Handle to a consumer instance (opaque pointer)
pub struct ConsumerHandle {
    inner: Consumer,
}

/// Structure to return metadata to FFI caller
/// Must match simple layout for C
#[repr(C)]
pub struct FFIMessageMeta {
    pub message_id: u64,
    pub timestamp_ns: u64,
    pub channel_id: u32,
    pub message_type: u32,
    pub sender_pid: u32,
    pub sender_runtime: u16,
    pub flags: u16,
    pub payload_len: u32,
}

// -----------------------------------------------------------------------------
// Allocator / Utils
// -----------------------------------------------------------------------------

/// Get the number of active channels.
#[no_mangle]
pub extern "C" fn dmxp_channel_count() -> i32 {
    const SHM_SIZE: usize = 128 * 1024 * 1024;
    match SharedMemoryAllocator::attach(SHM_SIZE) {
        Ok(allocator) => allocator.channel_count() as i32,
        Err(_) => -1, // Error attaching
    }
}

/// Get a list of active channel IDs.
///
/// # Arguments
/// * `out_buf` - Buffer to write u32 channel IDs into.
/// * `max_count` - Maximum number of IDs to write.
/// * `out_count` - Output: Actual number of IDs written.
///
/// # Returns
/// * 0 on success.
#[no_mangle]
pub extern "C" fn dmxp_list_channels(
    out_buf: *mut u32,
    max_count: usize,
    out_count: *mut usize,
) -> i32 {
    const SHM_SIZE: usize = 128 * 1024 * 1024;

    if out_count.is_null() {
        return DMXP_ERROR_NULL_POINTER;
    }

    match SharedMemoryAllocator::attach(SHM_SIZE) {
        Ok(allocator) => {
            let channels = allocator.get_channels();
            let count = std::cmp::min(channels.len(), max_count);

            unsafe { *out_count = count };

            if !out_buf.is_null() {
                for i in 0..count {
                    unsafe { *out_buf.add(i) = channels[i].id() };
                }
            }
            DMXP_SUCCESS
        }
        Err(_) => DMXP_ERROR_INTERNAL,
    }
}

// -----------------------------------------------------------------------------
// Producer API
// -----------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn dmxp_producer_new(channel_id: u32, capacity: u32) -> *mut ProducerHandle {
    const SHM_SIZE: usize = 128 * 1024 * 1024;

    // Ensure allocator exists (attach or create)
    let _ =
        SharedMemoryAllocator::attach(SHM_SIZE).or_else(|_| SharedMemoryAllocator::new(SHM_SIZE));

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

#[no_mangle]
pub extern "C" fn dmxp_producer_free(handle: *mut ProducerHandle) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle);
        }
    }
}

// -----------------------------------------------------------------------------
// Consumer API
// -----------------------------------------------------------------------------

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

/// Receive a message with full metadata.
///
/// # Arguments
/// * `timeout_ms` -
///     - -1: Blocking (wait forever)
///     -  0: Non-blocking (return immediately)
///     - >0: Wait for X milliseconds
/// * `out_meta` - Pointer to `FFIMessageMeta` struct to fill.
#[no_mangle]
pub extern "C" fn dmxp_consumer_receive_ext(
    handle: *mut ConsumerHandle,
    timeout_ms: i32,
    out_buf: *mut u8,
    out_len: *mut usize,
    out_meta: *mut FFIMessageMeta,
) -> i32 {
    if handle.is_null() || out_len.is_null() {
        return DMXP_ERROR_NULL_POINTER;
    }

    let consumer = unsafe { &(*handle).inner };
    let max_len = unsafe { *out_len };

    let result = if timeout_ms < 0 {
        // Blocking
        match consumer.receive_blocking_with_meta() {
            Ok(res) => Some(res),
            Err(_) => return DMXP_ERROR_INTERNAL,
        }
    } else if timeout_ms == 0 {
        // Non-blocking
        match consumer.receive_with_meta() {
            Ok(Some(res)) => Some(res),
            Ok(None) => return DMXP_ERROR_EMPTY,
            Err(_) => return DMXP_ERROR_INTERNAL,
        }
    } else {
        // Timeout
        match consumer
            .receive_timeout_with_meta(std::time::Duration::from_millis(timeout_ms as u64))
        {
            Ok(Some(res)) => Some(res),
            Ok(None) => return DMXP_ERROR_TIMEOUT,
            Err(_) => return DMXP_ERROR_INTERNAL,
        }
    };

    if let Some((meta, data)) = result {
        if data.len() > max_len {
            unsafe { *out_len = data.len() };
            return DMXP_ERROR_INVALID_ARG;
        }

        unsafe {
            if !out_buf.is_null() {
                ptr::copy_nonoverlapping(data.as_ptr(), out_buf, data.len());
            }
            *out_len = data.len();

            if !out_meta.is_null() {
                (*out_meta).message_id = meta.message_id;
                (*out_meta).timestamp_ns = meta.timestamp_ns;
                (*out_meta).channel_id = meta.channel_id;
                (*out_meta).message_type = meta.message_type;
                (*out_meta).sender_pid = meta.sender_pid;
                (*out_meta).sender_runtime = meta.sender_runtime;
                (*out_meta).flags = meta.flags;
                (*out_meta).payload_len = meta.payload_len;
            }
        }
        DMXP_SUCCESS
    } else {
        DMXP_ERROR_INTERNAL
    }
}

/// Same as above but strictly for backwards compatibility/simplicity
#[no_mangle]
pub extern "C" fn dmxp_consumer_receive(
    handle: *mut ConsumerHandle,
    blocking: bool,
    out_buf: *mut u8,
    out_len: *mut usize,
) -> i32 {
    let timeout = if blocking { -1 } else { 0 };
    dmxp_consumer_receive_ext(handle, timeout, out_buf, out_len, ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn dmxp_consumer_free(handle: *mut ConsumerHandle) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle);
        }
    }
}
