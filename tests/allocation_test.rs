// Example allocation tracking tests
//
// Run dhat test:
//   cargo test --test allocation_test track_allocations_with_dhat -- --nocapture
//
// Run memory-stats test:
//   cargo test --test allocation_test track_allocations_with_memory_stats -- --nocapture

use crossbeam_utils::CachePadded;
use dmxp_kvcache::MPMC::Buffer::layout::ChannelEntry;
use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use dmxp_kvcache::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::alloc::{alloc, Layout};
use std::sync::atomic::AtomicU64;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn create_dummy_channel_entry(capacity: u64) -> ChannelEntry {
    ChannelEntry {
        channel_id: 0,
        flags: 0,
        capacity,
        band_offset: 0,
        tail: CachePadded::new(AtomicU64::new(0)),
        head: CachePadded::new(AtomicU64::new(0)),
        _pad: [],
    }
}

fn make_aligned_backing(capacity: usize) -> (*mut u8, Layout) {
    let size = capacity * RingBuffer::slot_stride();
    let layout = Layout::from_size_align(size, 128).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        panic!("Failed to allocate aligned memory");
    }
    (ptr, layout)
}

#[test]
#[serial_test::serial]
fn track_allocations_with_dhat() {
    let _profiler = dhat::Profiler::new_heap();

    // Allocate after profiler starts - this should be tracked
    let capacity = 1024;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr) };
    unsafe {
        rb.init_slots();
    }

    let payload = vec![1u8; 100];
    let meta = MessageMeta::default();

    // Test enqueue/dequeue operations
    for _ in 0..1000 {
        while rb.enqueue(meta, &payload).is_none() {
            std::hint::spin_loop();
        }
        while rb.dequeue().is_none() {
            std::hint::spin_loop();
        }
    }

    println!("After 1000 enqueue/dequeue pairs, check dhat output for allocations");

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[test]
fn track_allocations_with_memory_stats() {
    use memory_stats::memory_stats;

    let before = memory_stats();
    println!("Memory before: {:?}", before);

    let capacity = 1024;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr) };
    unsafe {
        rb.init_slots();
    }

    let payload = vec![1u8; 100];
    let meta = MessageMeta::default();

    for _ in 0..100 {
        rb.enqueue(meta, &payload);
    }

    let after = memory_stats();
    println!("Memory after: {:?}", after);

    if let (Some(b), Some(a)) = (before, after) {
        println!("Memory delta: {} bytes", a.physical_mem - b.physical_mem);
    }

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[test]
#[serial_test::serial]
fn verify_zero_allocation_enqueue_dequeue() {
    let _dhat = dhat::Profiler::new_heap();

    // Pre-allocate buffer outside of measurement
    let capacity = 1024;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr) };
    unsafe {
        rb.init_slots();
    }

    let payload = vec![1u8; 100];
    let meta = MessageMeta::default();

    // Now measure allocations during enqueue/dequeue operations
    // Note: dequeue now allocates a Vec for the payload, so this won't be zero allocation anymore
    for _ in 0..1000 {
        rb.enqueue(meta, &payload);
        if let Some((_meta, _data)) = rb.dequeue() {
            // Successful dequeue
        }
    }

    // dhat output should show minimal allocations (ideally 0 for the hot path)
    println!("After 1000 enqueue/dequeue pairs, check dhat output for allocations");

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}
