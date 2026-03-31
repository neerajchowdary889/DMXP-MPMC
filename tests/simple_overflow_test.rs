/// Simple test to verify overflow functionality works with shared backend
use dmxp_mpmc::MPMC::{ChannelBuilder, Consumer, Producer};
use std::thread;
use std::time::Duration;

const OVERFLOW_SIZE: usize = 2048; // Larger than MSG_INLINE (1024 bytes)

#[test]
fn test_overflow_with_shared_backend() {
    println!("Starting overflow test with shared backend");

    // Create producer first - this will create the shared memory
    let producer: Producer = ChannelBuilder::new()
        .with_channel_id(55)
        .with_capacity(16)
        .with_buffer_size(1024 * 1024)  // 1MB - much smaller
        .build_producer()
        .expect("Failed to build producer");

    // Create consumer - will attach to the same channel
    let consumer: Consumer = ChannelBuilder::new()
        .with_channel_id(55)
        .with_capacity(16)
        .with_buffer_size(1024 * 1024)  // 1MB
        .build_consumer()
        .expect("Failed to build consumer");

    // Test 1: Send small message (no overflow)
    let small_data = vec![0x11u8; 100];
    producer.send(&small_data).expect("Failed to send small message");

    let received = consumer.receive()
        .expect("Failed to receive")
        .expect("No message available");
    assert_eq!(received, small_data);
    println!("✓ Small message (100 bytes) passed");

    // Test 2: Send large message (triggers overflow)
    let large_data = vec![0x22u8; OVERFLOW_SIZE];
    producer.send(&large_data).expect("Failed to send large message");

    let received = consumer.receive()
        .expect("Failed to receive")
        .expect("No message available");
    assert_eq!(received.len(), OVERFLOW_SIZE);
    assert_eq!(received[0], 0x22);
    assert_eq!(received[OVERFLOW_SIZE - 1], 0x22);
    println!("✓ Large message ({} bytes) passed via SFU overflow", OVERFLOW_SIZE);

    // Test 3: Send multiple overflow messages
    for i in 0..3 {
        let mut data = vec![0xAAu8; OVERFLOW_SIZE];
        data[0] = i;
        producer.send(&data).expect("Failed to send");
    }

    for i in 0..3 {
        let received = consumer.receive()
            .expect("Failed to receive")
            .expect("No message available");
        assert_eq!(received.len(), OVERFLOW_SIZE);
        assert_eq!(received[0], i);
        println!("✓ Overflow message {} received correctly", i);
    }

    println!("All overflow tests passed!");
}

#[test]
fn test_overflow_in_separate_threads() {
    println!("Starting multi-threaded overflow test");

    let producer_thread = thread::spawn(|| {
        let producer: Producer = ChannelBuilder::new()
            .with_channel_id(56)
            .with_capacity(8)
            .with_buffer_size(8 * 1024 * 1024)  // 8MB
            .build_producer()
            .expect("Failed to build producer");

        for i in 0..5 {
            let mut data = vec![0xFFu8; OVERFLOW_SIZE];
            data[0] = i;
            data[OVERFLOW_SIZE - 1] = i;
            producer.send(&data).expect("Failed to send");
            println!("Producer: sent message {}", i);
            thread::sleep(Duration::from_millis(10));
        }
    });

    let consumer_thread = thread::spawn(|| {
        // Give producer time to create the channel
        thread::sleep(Duration::from_millis(100));

        let consumer: Consumer = ChannelBuilder::new()
            .with_channel_id(56)
            .with_capacity(8)
            .with_buffer_size(8 * 1024 * 1024)  // 8MB
            .build_consumer()
            .expect("Failed to build consumer");

        for i in 0..5 {
            let mut attempts = 0;
            loop {
                match consumer.receive() {
                    Ok(Some(data)) => {
                        assert_eq!(data.len(), OVERFLOW_SIZE);
                        assert_eq!(data[0], i);
                        assert_eq!(data[OVERFLOW_SIZE - 1], i);
                        println!("Consumer: received message {}", i);
                        break;
                    }
                    Ok(None) => {
                        thread::sleep(Duration::from_millis(10));
                        attempts += 1;
                        if attempts > 100 {
                            panic!("Timeout waiting for message {}", i);
                        }
                    }
                    Err(e) => panic!("Error receiving: {:?}", e),
                }
            }
        }
    });

    producer_thread.join().expect("Producer thread failed");
    consumer_thread.join().expect("Consumer thread failed");

    println!("Multi-threaded overflow test passed!");
}