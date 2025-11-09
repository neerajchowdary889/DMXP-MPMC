// tests/allocator_test.rs

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::path::Path;
use std::fs;
use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;
use dmxp_kvcache::MPMC::Buffer::layout::GlobalHeader;

// Test helper to ensure we're the only test using shared memory
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

#[test]
fn test_shared_memory_allocator() -> io::Result<()> {
    // Use a mutex to ensure tests don't run in parallel
    let _guard = TEST_LOCK.lock();
    
    // Clean up any existing shared memory
    cleanup_shared_memory();
    
    // Test 1: Create a new allocator
    let allocator = SharedMemoryAllocator::new(1024 * 1024)?; // 1MB shared memory
    assert!(allocator.get_channel(0).is_none());
    
    // Test 2: Create a channel
    let channel = allocator.create_channel(1024)?; // 1K slots
    assert_eq!(channel.id(), 0);
    
    // Test 3: Get the channel back
    let channel_again = allocator.get_channel(0).expect("Channel should exist");
    assert_eq!(channel_again.id(), 0);
    
    // Test 4: Create multiple channels
    for i in 1..5 {
        allocator.create_channel(512)?;
        let channel = allocator.get_channel(i).expect("Channel should exist");
        assert_eq!(channel.id(), i);
    }
    
    // Test 5: Try to get non-existent channel
    assert!(allocator.get_channel(999).is_none());
    
    Ok(())
}

#[test]
fn test_concurrent_access() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    // Create shared memory in parent process with enough space for all channels
    let total_channels = 4;
    let channel_size = 256 * 1024; // 256KB per channel
    let header_size = std::mem::size_of::<GlobalHeader>();
    let total_size = (header_size + (total_channels * channel_size) + 127) & !127; // Align to 128 bytes
    
    let _allocator = SharedMemoryAllocator::new(total_size)?;
    
    let barrier = Arc::new(std::sync::Barrier::new(total_channels + 1)); // +1 for main thread
    let success = Arc::new(AtomicBool::new(true));
    let mut handles = vec![];
    
    // Start all worker threads
    for thread_id in 0..total_channels {
        let barrier = barrier.clone();
        let success = success.clone();
        
        handles.push(thread::spawn(move || -> io::Result<()> {
            // Wait for all threads to be ready
            barrier.wait();
            
            // All threads attach to the same shared memory
            let allocator = match SharedMemoryAllocator::attach(total_size) {
                Ok(alloc) => alloc,
                Err(e) => {
                    eprintln!("Thread {} failed to attach: {}", thread_id, e);
                    success.store(false, Ordering::SeqCst);
                    return Ok(());
                }
            };
            
            // Each thread creates its own channel
            let channel_id = thread_id as u32;
            match allocator.create_channel(256) {
                Ok(channel) => {
                    if channel.id() != channel_id {
                        success.store(false, Ordering::SeqCst);
                    }
                }
                Err(e) => {
                    eprintln!("Thread {} failed to create channel: {}", thread_id, e);
                    success.store(false, Ordering::SeqCst);
                }
            }
            
            Ok(())
        }));
    }
    
    // Release all threads at once
    barrier.wait();
    
    // Wait for all threads to complete
    for handle in handles {
        if let Err(e) = handle.join().expect("Thread panicked") {
            eprintln!("Thread error: {}", e);
            success.store(false, Ordering::SeqCst);
        }
    }
    
    assert!(success.load(Ordering::SeqCst), "One or more threads failed");
    Ok(())
}


fn cleanup_shared_memory() {
    // Clean up any existing shared memory files
    #[cfg(target_os = "linux")]
    {
        let paths = ["/dev/shm/dmxp_alloc", "/tmp/dmxp_alloc"];
        for path in &paths {
            if Path::new(path).exists() {
                let _ = fs::remove_file(path);
            }
        }
    }
}