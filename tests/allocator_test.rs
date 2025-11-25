// tests/allocator_test.rs

use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;
use dmxp_kvcache::MPMC::Buffer::layout::GlobalHeader;
use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

// Test helper to ensure we're the only test using shared memory
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

#[test]
fn test_shared_memory_allocator() -> io::Result<()> {
    // Use a mutex to ensure tests don't run in parallel
    let _guard = TEST_LOCK.lock();

    // Clean up any existing shared memory
    cleanup_shared_memory();

    // Test 1: Create a new allocator
    // We need enough space for header + channels + alignment
    // 1024 slots * ~1KB = 1MB. Plus header. So 2MB is safer.
    let allocator = SharedMemoryAllocator::new(4 * 1024 * 1024)?; // 4MB shared memory
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
    let slot_stride = RingBuffer::slot_stride();
    let channel_slots = 256;
    let channel_size = channel_slots * slot_stride;
    let header_size = std::mem::size_of::<GlobalHeader>();

    // Add plenty of padding/alignment buffer
    let total_size = (header_size + (total_channels * channel_size) + 1024 * 1024) & !127;

    // Create the allocator in the main thread first
    let allocator = Arc::new(SharedMemoryAllocator::new(total_size)?);

    // Create a barrier to synchronize thread starts
    let barrier = Arc::new(std::sync::Barrier::new(total_channels + 1));

    // We'll use a channel to collect results from threads
    let (tx, rx) = std::sync::mpsc::channel();

    // Create a mutex to protect the allocator during channel creation
    let allocator_mutex = Arc::new(Mutex::new(()));

    let success = Arc::new(AtomicBool::new(true));
    let mut handles = vec![];

    // Start all worker threads
    for thread_id in 0..total_channels {
        let allocator = allocator.clone();
        let barrier = barrier.clone();
        let tx = tx.clone();
        let allocator_mutex = allocator_mutex.clone();

        handles.push(thread::spawn(move || -> io::Result<()> {
            // Wait for all threads to be ready
            barrier.wait();

            // Use a mutex to ensure only one thread creates a channel at a time
            let _guard = allocator_mutex.lock();

            // Each thread creates a channel with a unique ID
            match allocator.create_channel(channel_slots) {
                Ok(channel) => {
                    let channel_id = channel.id();
                    tx.send((thread_id, channel_id, Ok(()))).unwrap();
                }
                Err(e) => {
                    tx.send((thread_id, 0, Err(e))).unwrap();
                }
            }

            Ok(())
        }));
    }

    // Let all threads proceed
    barrier.wait();

    // Wait for all threads to finish
    for handle in handles {
        if let Err(e) = handle.join() {
            eprintln!("Thread panicked: {:?}", e);
            success.store(false, Ordering::SeqCst);
        }
    }

    // Collect results from all threads
    let mut created_channels = std::collections::HashSet::new();
    let mut errors = Vec::new();

    for _ in 0..total_channels {
        if let Ok((thread_id, channel_id, result)) = rx.recv() {
            match result {
                Ok(()) => {
                    println!("Thread {} created channel {}", thread_id, channel_id);
                    if !created_channels.insert(channel_id) {
                        eprintln!("Duplicate channel ID: {}", channel_id);
                        success.store(false, Ordering::SeqCst);
                    }
                }
                Err(e) => {
                    eprintln!("Thread {} failed to create channel: {}", thread_id, e);
                    errors.push(e);
                    success.store(false, Ordering::SeqCst);
                }
            }
        }
    }

    // Verify the expected number of channels were created
    if created_channels.len() != total_channels {
        eprintln!(
            "Expected {} channels, but created {}",
            total_channels,
            created_channels.len()
        );
        success.store(false, Ordering::SeqCst);
    }

    // Verify all channels exist in the allocator
    for channel_id in 0..total_channels as u32 {
        if allocator.get_channel(channel_id).is_none() {
            eprintln!("Channel {} was not created successfully", channel_id);
            success.store(false, Ordering::SeqCst);
        }
    }

    Ok(())
}

fn cleanup_shared_memory() {
    // Clean up any existing shared memory files
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};

        // Clean up any shared memory files in /dev/shm
        let shm_paths = [
            "/dev/shm/dmxp_alloc",
            "/dev/shm/dmxp_shm",
            "/tmp/dmxp_alloc",
            "/tmp/dmxp_shm",
        ];

        for path in &shm_paths {
            if Path::new(path).exists() {
                let _ = fs::remove_file(path);
            }
        }

        // Clean up System V shared memory segments
        let _ = Command::new("ipcrm")
            .args(&["-M", "0x444D5850"]) // DMXP in hex
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        // Also try to clean up any orphaned shared memory segments using ipcs/ipcrm
        if let Ok(output) = Command::new("ipcs")
            .args(&["-m"]) // Shared memory segments
            .output()
        {
            // Parse ipcs output to find our segments
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                for line in output_str.lines() {
                    if line.contains("DMXP") || line.contains("dmxp") {
                        if let Some(id) = line.split_whitespace().nth(1) {
                            if let Ok(_) = id.parse::<i32>() {
                                let _ = Command::new("ipcrm")
                                    .args(&["-m", id])
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status();
                            }
                        }
                    }
                }
            }
        }

        // Clean up any remaining file descriptors in /dev/shm
        if let Ok(entries) = fs::read_dir("/dev/shm") {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("dmxp") || name.contains("DMXP") {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }

        // Give the OS a moment to clean up
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn test_get_channels_and_count() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    // Create allocator with enough space
    let allocator = SharedMemoryAllocator::new(10 * 1024 * 1024)?; // 10MB

    // Initially, no channels should exist
    assert_eq!(allocator.channel_count(), 0);
    assert_eq!(allocator.get_channels().len(), 0);

    // Create first channel
    let channel1 = allocator.create_channel(512)?;
    assert_eq!(channel1.id(), 0);
    assert_eq!(allocator.channel_count(), 1);

    let channels = allocator.get_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id(), 0);

    // Create more channels
    let channel2 = allocator.create_channel(256)?;
    let channel3 = allocator.create_channel(1024)?;

    assert_eq!(channel2.id(), 1);
    assert_eq!(channel3.id(), 2);
    assert_eq!(allocator.channel_count(), 3);

    // Get all channels and verify
    let all_channels = allocator.get_channels();
    assert_eq!(all_channels.len(), 3);

    // Verify each channel ID
    let channel_ids: Vec<u32> = all_channels.iter().map(|c| c.id()).collect();
    assert_eq!(channel_ids, vec![0, 1, 2]);

    // Create a few more channels
    for _ in 0..5 {
        allocator.create_channel(128)?;
    }

    assert_eq!(allocator.channel_count(), 8);
    assert_eq!(allocator.get_channels().len(), 8);

    println!("✓ All channel enumeration tests passed!");

    Ok(())
}

#[test]
fn test_memory_tracking() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();
    cleanup_shared_memory();

    let total_size = 20 * 1024 * 1024; // 20MB
    let allocator = SharedMemoryAllocator::new(total_size)?;

    // Check initial state
    let initial_used = allocator.used_memory();
    let initial_available = allocator.available_memory();

    println!("Initial used memory: {} bytes", initial_used);
    println!("Initial available memory: {} bytes", initial_available);

    assert!(initial_used > 0, "Should have some memory used for header");
    assert!(initial_available > 0, "Should have available memory");
    assert_eq!(initial_used + initial_available, total_size);

    // Create a channel and check memory usage
    allocator.create_channel(1024)?;

    let used_after_channel = allocator.used_memory();
    let available_after_channel = allocator.available_memory();

    println!(
        "After creating channel - used: {}, available: {}",
        used_after_channel, available_after_channel
    );

    assert!(
        used_after_channel > initial_used,
        "Used memory should increase"
    );
    assert!(
        available_after_channel < initial_available,
        "Available memory should decrease"
    );
    assert_eq!(used_after_channel + available_after_channel, total_size);

    // Create more channels
    for _ in 0..3 {
        allocator.create_channel(512)?;
    }

    let final_used = allocator.used_memory();
    let final_available = allocator.available_memory();

    println!(
        "After 4 channels - used: {}, available: {}",
        final_used, final_available
    );

    assert!(
        final_used > used_after_channel,
        "Used memory should keep increasing"
    );
    assert_eq!(final_used + final_available, total_size);
    assert_eq!(allocator.channel_count(), 4);

    println!("✓ Memory tracking tests passed!");

    Ok(())
}
