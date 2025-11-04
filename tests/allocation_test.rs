// Example allocation tracking tests
// 
// Run dhat test:
//   cargo test --test allocation_test track_allocations_with_dhat -- --nocapture
//
// Run memory-stats test:
//   cargo test --test allocation_test track_allocations_with_memory_stats -- --nocapture

use dmxp_kvcache::MPMC::Buffer::RingBuffer;

#[test]
fn track_allocations_with_dhat() {
    let _dhat = dhat::Profiler::new_heap();
    
    // Allocate after profiler starts - this should be tracked
    let capacity = 1024;
    let backing = vec![0u8; capacity * RingBuffer::slot_stride()];
    println!("Allocated backing buffer: {} bytes", backing.len());
    
    // Keep it alive by using it
    let ptr = backing.as_ptr() as *mut u8;
    let rb = RingBuffer::new(ptr, capacity);
    unsafe { rb.init_slots(); }
    
    // Do some operations - these should NOT allocate (zero-allocation hot path)
    for i in 0..100 {
        rb.enqueue(i);
    }
    
    // Allocate something that stays alive until profiler drops
    let _test_vec = vec![0u8; 2048];
    let _another_vec: Vec<u32> = (0..1000).collect();
    
    // Keep references alive - dhat tracks until profiler is dropped
    std::mem::drop(backing); // Explicitly drop to see if it shows up
    // _test_vec and _another_vec will be dropped when profiler drops
    
    // dhat will print allocation stats when dropped
    // Note: If still showing 0, it might be because Vec uses jemalloc/other allocator
    // that dhat doesn't intercept, or allocations are optimized away
}

#[test]
fn track_allocations_with_memory_stats() {
    use memory_stats::memory_stats;
    
    let before = memory_stats();
    println!("Memory before: {:?}", before);
    
    let capacity = 1024;
    let backing = vec![0u8; capacity * RingBuffer::slot_stride()];
    let ptr = backing.as_ptr() as *mut u8;
    let rb = RingBuffer::new(ptr, capacity);
    unsafe { rb.init_slots(); }
    
    for i in 0..100 {
        rb.enqueue(i);
    }
    
    let after = memory_stats();
    println!("Memory after: {:?}", after);
    
    if let (Some(b), Some(a)) = (before, after) {
        println!("Memory delta: {} bytes", a.physical_mem - b.physical_mem);
    }
}

#[test]
fn verify_zero_allocation_enqueue_dequeue() {
    let _dhat = dhat::Profiler::new_heap();
    
    // Pre-allocate buffer outside of measurement
    let capacity = 1024;
    let backing = vec![0u8; capacity * RingBuffer::slot_stride()];
    let ptr = backing.as_ptr() as *mut u8;
    let rb = RingBuffer::new(ptr, capacity);
    unsafe { rb.init_slots(); }
    
    // Now measure allocations during enqueue/dequeue operations
    // These should be zero (or minimal) since RingBuffer uses pre-allocated memory
    for i in 0..1000 {
        rb.enqueue(i);
        if let Some((_idx, _len)) = rb.dequeue() {
            // Successful dequeue
        }
    }
    
    // dhat output should show minimal allocations (ideally 0 for the hot path)
    println!("After 1000 enqueue/dequeue pairs, check dhat output for allocations");
}