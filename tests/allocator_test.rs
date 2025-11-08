// tests/allocator_test.rs

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;

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
    
    // Create shared memory in parent process
    let _allocator = SharedMemoryAllocator::new(1024 * 1024)?;
    
    // Simulate multiple processes by using threads
    let num_threads = 4;
    let barrier = Arc::new(std::sync::Barrier::new(num_threads));
    let success = Arc::new(AtomicBool::new(true));
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let barrier = barrier.clone();
        let success = success.clone();
        
        handles.push(thread::spawn(move || -> io::Result<()> {
            // All threads attach to the same shared memory (1MB, matching the size used in create)
            let allocator = SharedMemoryAllocator::attach(1024 * 1024)?;
            
            // Each thread creates its own channel
            let channel_id = thread_id as u32;
            let channel = allocator.create_channel(256)?;
            assert_eq!(channel.id(), channel_id);
            
            // Wait for all threads to create their channels
            barrier.wait();
            
            // Verify all channels exist
            for i in 0..num_threads {
                if allocator.get_channel(i as u32).is_none() {
                    success.store(false, Ordering::SeqCst);
                    return Ok(());
                }
            }
            
            Ok(())
        }));
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked")?;
    }
    
    assert!(success.load(Ordering::SeqCst), "Failed to verify all channels");
    Ok(())
}

fn cleanup_shared_memory() {
    // On Linux, we could remove the shared memory file
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let _ = fs::remove_file("/dev/shm/dmxp_alloc");
    }
    
    // On Windows, the OS will clean up when the last handle is closed
}