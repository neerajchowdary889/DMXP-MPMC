use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::path::Path;
use std::fs;
use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;
use dmxp_kvcache::MPMC::Buffer::layout::GlobalHeader;
use serial_test::serial;

// Test helper to ensure we're the only test using shared memory
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

#[test]
#[serial]
fn test_shared_memory_creation() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    // Get the exact minimum required size (aligned)
    let min_required = (std::mem::size_of::<GlobalHeader>() + 127) & !127;
    let test_sizes = &[min_required, min_required * 2, 1024 * 1024];  // 1MB

    for &size in test_sizes {
        let allocator = SharedMemoryAllocator::new(size)?;

        //debugging
        println!("Available memory: {} bytes", allocator.available_memory());
        println!("Allocator Struct: {:#?}", allocator);

        assert!(
            allocator.available_memory() <= size.saturating_sub(min_required),
            "Available memory should be at most size - header_size"
        );
    }
    
    // Test with size smaller than header (should fail)
    // We need to ensure the size is small enough that even after alignment it's still too small
    let too_small = std::mem::size_of::<GlobalHeader>() - 128; // Subtract a full alignment to be safe
    assert!(
        SharedMemoryAllocator::new(too_small).is_err(),
        "Should fail with size smaller than header"
    );
    
    Ok(())
}

#[test]
#[serial]
fn test_channel_management() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    // Create allocator with enough space
    let allocator = SharedMemoryAllocator::new(1024 * 1024)?;
    
    // Test creating specific channels
    let student_channel = allocator.create_channel(256)?;
    let teacher_channel = allocator.create_channel(256)?;
    
    // Verify channels have distinct IDs
    assert_ne!(student_channel.id(), teacher_channel.id());
    
    // Store the channel IDs for later use
    let student_channel_id = student_channel.id();
    let teacher_channel_id = teacher_channel.id();
    
    // Test retrieving channels by their specific IDs
    let student_channel = allocator.get_channel(student_channel_id).expect("Student channel should exist");
    let teacher_channel = allocator.get_channel(teacher_channel_id).expect("Teacher channel should exist");
    
    // debugging
    println!("Student Channel Struct: {:#?}", student_channel);
    println!("Teacher Channel Struct: {:#?}", teacher_channel);
    
    // Test removing a channel
    assert!(allocator.remove_channel(student_channel_id).is_ok());
    assert!(allocator.get_channel(student_channel_id).is_none(), "Student channel should be removed");
    
    // Teacher channel should still exist
    assert!(allocator.get_channel(teacher_channel_id).is_some(), "Teacher channel should still exist");
    
    // Test removing non-existent channel
    assert!(allocator.remove_channel(999).is_err(), "Removing non-existent channel should fail");
    
    Ok(())
}

#[test]
#[serial]
fn test_concurrent_channel_operations() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    let total_threads = 8;  // Using 8 threads for concurrent testing
    let ops_per_thread = 100;
    let allocator = Arc::new(SharedMemoryAllocator::new(16 * 1024 * 1024)?);
    let counter = Arc::new(AtomicU64::new(0));
    
    let mut handles = vec![];
    
    // Spawn multiple threads
    for _ in 0..total_threads {
        let allocator = allocator.clone();
        let counter = counter.clone();
        
        handles.push(thread::spawn(move || -> io::Result<()> {
            for _ in 0..ops_per_thread {
                // Randomly choose between creating and removing channels
                if fastrand::bool() {
                    if let Ok(channel) = allocator.create_channel(256) {
                        let id = channel.id();
                        counter.fetch_add(1, Ordering::SeqCst);
                        
                        // Verify we can read the channel
                        assert_eq!(allocator.get_channel(id).unwrap().id(), id);
                        
                        // Sometimes remove the channel immediately
                        if fastrand::bool() {
                            allocator.remove_channel(id)?;
                            counter.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                } else {
                    // Try to remove a random channel
                    let id = fastrand::u32(0..100);
                    if allocator.remove_channel(id).is_ok() {
                        counter.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
            Ok(())
        }));
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap()?;
    }
    
    // Verify the allocator is in a consistent state
    let allocator = Arc::try_unwrap(allocator).expect("Failed to unwrap Arc");
    let active_channels = (0..100)
        .filter(|&i| allocator.get_channel(i).is_some())
        .count();

    assert_eq!(active_channels as u64, counter.load(Ordering::SeqCst));
    
    Ok(())
}

#[test]
#[serial]
fn test_memory_management() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    let header_size = std::mem::size_of::<GlobalHeader>();
    let channel_size = 1024;
    let total_size = header_size + (channel_size * 2) + 128; // Extra space for alignment
    
    let allocator = SharedMemoryAllocator::new(total_size)?;
    
    // First channel should fit
    let _channel1 = allocator.create_channel(512)?;
    
    // Second channel should also fit
    let _channel2 = allocator.create_channel(256)?;
    
    // Third channel should fail (not enough space)
    assert!(allocator.create_channel(512).is_err());
    
    // Remove first channel
    allocator.remove_channel(0)?;
    
    // Now we should be able to create a new channel
    let _channel3 = allocator.create_channel(256)?;
    
    // But still not enough space for a large channel
    assert!(allocator.create_channel(1024).is_err());
    
    Ok(())
}

#[test]
#[serial]
fn test_persistence() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    // Create and populate shared memory
    {
        let allocator = SharedMemoryAllocator::new(1024 * 1024)?;
        let channel = allocator.create_channel(256)?;
        // Simulate some data being written
        let _ = channel; // Use channel to prevent warning
    }
    
    // Re-attach and verify
    let allocator = SharedMemoryAllocator::attach(1024 * 1024)?;
    assert!(allocator.get_channel(0).is_some());
    
    // Create another channel
    let channel = allocator.create_channel(256)?;
    assert_eq!(channel.id(), 1);
    
    Ok(())
}

#[test]
#[serial]
fn test_error_conditions() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();
    
    let allocator = SharedMemoryAllocator::new(1024 * 1024)?;
    
    // Test invalid channel capacity (not power of two)
    assert!(allocator.create_channel(100).is_err());
    
    // Test zero capacity
    assert!(allocator.create_channel(0).is_err());
    
    // Test maximum channels
    for _ in 0..256 {
        let _ = allocator.create_channel(16)?;
    }
    
    // Next channel should fail
    assert!(allocator.create_channel(16).is_err());
    
    // Test removing non-existent channel
    assert!(allocator.remove_channel(999).is_err());
    
    Ok(())
}

fn cleanup_shared_memory() {
    // Clean up any existing shared memory files
    if let Ok(entries) = fs::read_dir("/dev/shm") {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("dmxp_") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}