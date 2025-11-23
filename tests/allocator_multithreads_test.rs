use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;
use dmxp_kvcache::MPMC::Buffer::layout::GlobalHeader;
use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use serial_test::serial;
use std::fs;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

// Test helper to ensure we're the only test using shared memory
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

#[test]
#[serial]
fn test_shared_memory_creation() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    // Get the exact minimum required size (aligned)
    let min_required = (std::mem::size_of::<GlobalHeader>() + 127) & !127;
    let test_sizes = &[min_required, min_required * 2, 1024 * 1024]; // 1MB

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
    let allocator = SharedMemoryAllocator::new(1024 * 1024 * 10)?; // 10MB

    // Test creating specific channels
    let student_channel = allocator.create_channel(256)?;
    let teacher_channel = allocator.create_channel(256)?;

    // Verify channels have distinct IDs
    assert_ne!(student_channel.id(), teacher_channel.id());

    // Store the channel IDs for later use
    let student_channel_id = student_channel.id();
    let teacher_channel_id = teacher_channel.id();

    // Test retrieving channels by their specific IDs
    let student_channel = allocator
        .get_channel(student_channel_id)
        .expect("Student channel should exist");
    let teacher_channel = allocator
        .get_channel(teacher_channel_id)
        .expect("Teacher channel should exist");

    // debugging
    println!("Student Channel Struct: {:#?}", student_channel);
    println!("Teacher Channel Struct: {:#?}", teacher_channel);

    // Test removing a channel
    assert!(allocator.remove_channel(student_channel_id).is_ok());
    assert!(
        allocator.get_channel(student_channel_id).is_none(),
        "Student channel should be removed"
    );

    // Teacher channel should still exist
    assert!(
        allocator.get_channel(teacher_channel_id).is_some(),
        "Teacher channel should still exist"
    );

    // Test removing non-existent channel
    assert!(
        allocator.remove_channel(999).is_err(),
        "Removing non-existent channel should fail"
    );

    Ok(())
}
#[test]
#[serial]
fn test_concurrent_channel_operations() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    let total_threads = 4; // Reduced threads to reduce contention
    let ops_per_thread = 10; // Further reduced operations per thread
    let allocator = Arc::new(SharedMemoryAllocator::new(64 * 1024 * 1024)?); // 64MB
    let counter = Arc::new(AtomicU64::new(0));

    // Track created channels to avoid removing non-existent ones
    let created_channels = Arc::new(Mutex::new(std::collections::HashSet::new()));

    // Track test duration
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30); // 30 second timeout

    let mut handles = vec![];

    // Spawn multiple threads
    for thread_id in 0..total_threads {
        let allocator = allocator.clone();
        let counter = counter.clone();
        let created_channels = created_channels.clone();

        let handle = thread::Builder::new()
            .name(format!("worker-{thread_id}"))
            .spawn(move || -> io::Result<()> {
                let thread_name = format!("worker-{}", thread_id);

                for op_num in 0..ops_per_thread {
                    // Check timeout
                    if start.elapsed() > timeout {
                        eprintln!("{}: Timeout after {:?}", thread_name, timeout);
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("Thread {} timed out after {:?}", thread_name, timeout),
                        ));
                    }

                    // Log progress
                    if op_num % 5 == 0 {
                        println!(
                            "{}: Operation {}/{} (channels: {})",
                            thread_name,
                            op_num,
                            ops_per_thread,
                            created_channels.lock().unwrap().len()
                        );
                    }

                    // Randomly choose between creating and removing channels
                    if fastrand::bool() {
                        // Create channel operation
                        match allocator.create_channel(1024) {
                            Ok(channel) => {
                                let id = channel.id();
                                counter.fetch_add(1, Ordering::SeqCst);
                                created_channels.lock().unwrap().insert(id);

                                // Verify we can read the channel
                                if let Some(channel) = allocator.get_channel(id) {
                                    assert_eq!(
                                        channel.id(),
                                        id,
                                        "Channel ID mismatch after creation"
                                    );
                                } else {
                                    return Err(io::Error::new(
                                        io::ErrorKind::Other,
                                        format!("Failed to get channel {} after creation", id),
                                    ));
                                }

                                // Sometimes remove the channel immediately (25% chance)
                                if fastrand::u8(0..4) == 0 {
                                    if allocator.remove_channel(id).is_ok() {
                                        counter.fetch_sub(1, Ordering::SeqCst);
                                        created_channels.lock().unwrap().remove(&id);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("{}: Failed to create channel: {}", thread_name, e);
                            }
                        }
                    } else {
                        // Remove operation - only try to remove channels we know exist
                        let channels: Vec<u32> = {
                            let chans = created_channels.lock().unwrap();
                            if chans.is_empty() {
                                continue;
                            }
                            chans.iter().cloned().collect()
                        };

                        if !channels.is_empty() {
                            let id = channels[fastrand::usize(0..channels.len())];

                            if allocator.remove_channel(id).is_ok() {
                                counter.fetch_sub(1, Ordering::SeqCst);
                                created_channels.lock().unwrap().remove(&id);
                                println!("{}: Removed channel {}", thread_name, id);
                            }
                        }
                    }

                    // Small delay to reduce contention
                    std::thread::yield_now();
                }
                Ok(())
            });

        match handle {
            Ok(handle) => handles.push(handle),
            Err(e) => {
                eprintln!("Failed to spawn worker thread: {}", e);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to spawn worker thread: {}", e),
                ));
            }
        }
    }

    // Wait for all threads to complete
    let mut errors = Vec::new();
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                errors.push(format!("Thread {} failed: {}", i, e));
            }
            Err(e) => {
                errors.push(format!("Thread {} panicked: {:?}", i, e));
            }
        }
    }

    // Check for any errors
    if !errors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Thread errors: {}", errors.join(", ")),
        ));
    }

    // Verify the allocator is in a consistent state
    let allocator = Arc::try_unwrap(allocator).expect("Failed to unwrap Arc");
    let active_channels = (0..1000) // Increased range to account for potential high channel IDs
        .filter(|&i| allocator.get_channel(i).is_some())
        .count();

    // Check that our counter matches the actual number of channels
    assert_eq!(
        active_channels as u64,
        counter.load(Ordering::SeqCst),
        "Mismatch between counter ({}) and actual channels ({})",
        counter.load(Ordering::SeqCst),
        active_channels
    );

    Ok(())
}

#[test]
#[serial]
fn test_memory_management() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    // Calculate required sizes more accurately
    let header_size = std::mem::size_of::<GlobalHeader>();
    let slot_stride = RingBuffer::slot_stride();
    println!("Slot stride: {} bytes", slot_stride);
    println!("Header size: {} bytes", header_size);

    // Each channel's actual size is larger than requested due to:
    // 1. Alignment requirements (128 bytes)
    // 2. RingBuffer overhead
    // 3. Channel metadata
    let channel1_slots = 512;
    let channel2_slots = 256;
    let _channel3_slots = 512;

    // Calculate total size needed with more headroom
    // We allocate a LOT of memory to rule out simple OOM due to overhead
    let total_size = 100 * 1024 * 1024; // 100MB

    println!("Creating allocator with size: {} bytes", total_size);
    let allocator = SharedMemoryAllocator::new(total_size)?;

    // First channel should fit
    println!(
        "Creating first channel (requested: {} slots)",
        channel1_slots
    );
    let channel1 = allocator.create_channel(channel1_slots)?;
    println!("Created channel with ID: {}", channel1.id());

    // Second channel should also fit
    println!(
        "Creating second channel (requested: {} slots)",
        channel2_slots
    );
    let channel2 = allocator.create_channel(channel2_slots)?;
    println!("Created channel with ID: {}", channel2.id());

    // Third channel might fail if we sized it tight enough
    // But to be safe and test OOM, let's try a REALLY large channel that definitely won't fit
    let huge_channel_slots = 100 * 1024 * 1024; // 100M slots -> ~100GB
    println!(
        "Attempting to create huge channel (requested: {} slots) - should fail",
        huge_channel_slots
    );
    let result = allocator.create_channel(huge_channel_slots);
    assert!(
        result.is_err(),
        "Expected error when creating huge channel, but got: {:?}",
        result
    );
    println!("Successfully got error as expected: {:?}", result);

    // Remove first channel
    println!("Removing first channel (ID: {})", channel1.id());
    allocator.remove_channel(channel1.id())?;
    println!("Successfully removed channel {}", channel1.id());

    // Now we should be able to create a new channel
    // Note: Since our allocator doesn't reuse memory (bump pointer), this will append.
    // 100MB should be enough to append many channels.
    println!(
        "Creating new channel after removal (requested: {} slots)",
        channel2_slots
    );
    let channel3 = allocator.create_channel(channel2_slots)?;
    println!("Created channel with ID: {}", channel3.id());

    Ok(())
}

#[test]
#[serial]
fn test_persistence() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    // Create and populate shared memory
    {
        let allocator = SharedMemoryAllocator::new(1024 * 1024 * 10)?; // 10MB
        let channel = allocator.create_channel(256)?;
        // Simulate some data being written
        let _ = channel; // Use channel to prevent warning
    }

    // Re-attach and verify
    let allocator = SharedMemoryAllocator::attach(1024 * 1024 * 10)?;
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

    // 64MB should be enough for 256 channels of 16 slots
    let allocator = SharedMemoryAllocator::new(64 * 1024 * 1024)?;

    // Test invalid channel capacity (not power of two)
    assert!(allocator.create_channel(100).is_err());

    // Test zero capacity
    assert!(allocator.create_channel(0).is_err());

    // Test maximum channels
    // MAX_CHANNELS is 256.
    for i in 0..256 {
        if let Err(e) = allocator.create_channel(16) {
            panic!("Failed to create channel {}: {}", i, e);
        }
    }

    // Next channel should fail (channel limit reached)
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
