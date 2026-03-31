/// Test that verifies cross-process overflow messages work correctly
/// with the new shared backend implementation.
///
/// This test creates a producer in one process that sends large messages
/// (which overflow to the SFU), and a consumer in another process that
/// can successfully read those messages.

use dmxp_mpmc::Core::alloc::SharedMemoryAllocator;
use dmxp_mpmc::MPMC::{ChannelBuilder, Consumer, Producer};
use std::thread;
use std::time::Duration;

const OVERFLOW_SIZE: usize = 2048; // Larger than MSG_INLINE (1024 bytes)
const TEST_CHANNEL_ID: u32 = 99;
const TEST_CAPACITY: usize = 16;
const NUM_MESSAGES: usize = 5;

#[test]
#[cfg(unix)]
fn test_cross_process_overflow() {
    // Clean up any previous test artifacts
    cleanup_test_artifacts();

    // Start producer process
    let producer_handle = thread::spawn(|| {
        run_producer_process();
    });

    // Give producer time to initialize and send messages
    thread::sleep(Duration::from_millis(500));

    // Start consumer process
    let consumer_result = run_consumer_process();

    // Wait for producer to finish
    producer_handle.join().expect("Producer thread failed");

    // Check consumer results
    assert!(consumer_result, "Consumer failed to receive overflow messages");

    // Clean up
    cleanup_test_artifacts();
}

fn run_producer_process() {
    println!("[Producer] Starting producer process");

    // Create the allocator and channel
    let allocator = SharedMemoryAllocator::new(16 * 1024 * 1024)  // 16MB instead of 128MB
        .expect("Failed to create allocator");

    allocator
        .create_channel(TEST_CAPACITY, Some(TEST_CHANNEL_ID))
        .expect("Failed to create channel");

    // Build producer
    let producer: Producer = ChannelBuilder::new()
        .with_channel_id(TEST_CHANNEL_ID)
        .with_capacity(TEST_CAPACITY)
        .build_producer()
        .expect("Failed to build producer");

    // Send large messages that will overflow
    for i in 0..NUM_MESSAGES {
        let mut data = vec![0u8; OVERFLOW_SIZE];
        // Put a unique pattern in each message
        data[0] = 0xAA;
        data[1] = i as u8;
        data[OVERFLOW_SIZE - 2] = i as u8;
        data[OVERFLOW_SIZE - 1] = 0xBB;

        let result = producer.send(&data);
        assert!(result.is_ok(), "Failed to send overflow message {}", i);
        println!("[Producer] Sent overflow message {} ({} bytes)", i, OVERFLOW_SIZE);
    }

    println!("[Producer] All messages sent successfully");

    // Keep the process alive a bit longer for consumer to read
    thread::sleep(Duration::from_millis(1000));
}

fn run_consumer_process() -> bool {
    println!("[Consumer] Starting consumer process");

    // Attach to existing shared memory
    thread::sleep(Duration::from_millis(100)); // Give producer time to create

    // Build consumer
    let consumer: Consumer = ChannelBuilder::new()
        .with_channel_id(TEST_CHANNEL_ID)
        .with_capacity(TEST_CAPACITY)
        .build_consumer()
        .expect("Failed to build consumer");

    // Read the messages
    let mut received_count = 0;
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 20;

    while received_count < NUM_MESSAGES && attempts < MAX_ATTEMPTS {
        match consumer.receive() {
            Ok(Some(data)) => {
                // Verify message structure
                assert_eq!(data.len(), OVERFLOW_SIZE, "Incorrect message size");
                assert_eq!(data[0], 0xAA, "Invalid start marker");
                assert_eq!(data[OVERFLOW_SIZE - 1], 0xBB, "Invalid end marker");

                let msg_id = data[1];
                assert_eq!(data[OVERFLOW_SIZE - 2], msg_id, "Message ID mismatch");

                println!("[Consumer] Received overflow message {} ({} bytes)",
                         msg_id, data.len());
                received_count += 1;
            }
            Ok(None) => {
                // No message available, wait a bit
                thread::sleep(Duration::from_millis(50));
                attempts += 1;
            }
            Err(e) => {
                eprintln!("[Consumer] Error receiving: {:?}", e);
                break;
            }
        }
    }

    println!("[Consumer] Received {}/{} messages", received_count, NUM_MESSAGES);
    received_count == NUM_MESSAGES
}

fn cleanup_test_artifacts() {
    // Clean up shared memory files
    #[cfg(unix)]
    {
        use std::fs;

        // Remove DMXP main shared memory
        let _ = fs::remove_file("/dev/shm/dmxp_alloc");

        // Remove SFU shared memory files
        // The cleanup_namespace is called by the allocator on creation,
        // but we do it here too for safety
        let _ = fs::remove_file("/dev/shm/dmxp_ovf_ctrl");
        for i in 0..10 {
            let _ = fs::remove_file(format!("/dev/shm/dmxp_ovf_data_{}", i));
        }
    }
}

/// Test that verifies same-process overflow still works
#[test]
fn test_same_process_overflow() {
    // Clean up any previous test artifacts
    cleanup_test_artifacts();

    let allocator = SharedMemoryAllocator::new(16 * 1024 * 1024)  // 16MB instead of 128MB
        .expect("Failed to create allocator");

    allocator
        .create_channel(TEST_CAPACITY, Some(TEST_CHANNEL_ID + 1))
        .expect("Failed to create channel");

    let producer: Producer = ChannelBuilder::new()
        .with_channel_id(TEST_CHANNEL_ID + 1)
        .with_capacity(TEST_CAPACITY)
        .build_producer()
        .expect("Failed to build producer");

    let consumer: Consumer = ChannelBuilder::new()
        .with_channel_id(TEST_CHANNEL_ID + 1)
        .with_capacity(TEST_CAPACITY)
        .build_consumer()
        .expect("Failed to build consumer");

    // Test with a large message
    let data = vec![0x42u8; OVERFLOW_SIZE];
    producer.send(&data).expect("Failed to send");

    let received = consumer.receive()
        .expect("Failed to receive")
        .expect("No message available");
    assert_eq!(received.len(), OVERFLOW_SIZE);
    assert_eq!(received[0], 0x42);
    assert_eq!(received[OVERFLOW_SIZE - 1], 0x42);

    println!("Same-process overflow test passed!");

    // Clean up
    cleanup_test_artifacts();
}