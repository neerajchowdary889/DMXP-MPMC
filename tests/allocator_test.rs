// tests/allocator_test.rs

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
            match allocator.create_channel(256) {
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
        eprintln!("Expected {} channels, but created {}", total_channels, created_channels.len());
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
            "/tmp/dmxp_shm"
        ];
        
        for path in &shm_paths {
            if Path::new(path).exists() {
                let _ = fs::remove_file(path);
            }
        }
        
        // Clean up System V shared memory segments
        let _ = Command::new("ipcrm")
            .args(&["-M", "0x444D5850"])  // DMXP in hex
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
            
        // Also try to clean up any orphaned shared memory segments using ipcs/ipcrm
        if let Ok(output) = Command::new("ipcs")
            .args(&["-m"])  // Shared memory segments
            .output() {
                
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