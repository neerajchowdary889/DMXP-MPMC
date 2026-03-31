/// Example producer that sends large overflow messages
/// Run this in one terminal, then run overflow_consumer in another

use dmxp_mpmc::MPMC::ChannelBuilder;
use std::thread;
use std::time::Duration;

fn main() {
    println!("=== Overflow Producer Example ===");
    println!("This producer sends large messages (2048 bytes) that trigger SFU overflow");
    println!();

    // Note: SFU cleanup happens automatically on first access
    // via the cleanup in process_sfu_create()

    println!("Creating producer...");
    let producer = ChannelBuilder::new()
        .with_channel_id(99)
        .with_capacity(16)
        .with_buffer_size(2 * 1024 * 1024) // 2MB
        .build_producer()
        .expect("Failed to build producer");

    println!("Producer ready. Starting to send messages...");
    println!();

    const NUM_MESSAGES: usize = 10;
    const MESSAGE_SIZE: usize = 2048; // Larger than MSG_INLINE (1024)

    for i in 0..NUM_MESSAGES {
        let mut data = vec![0xAAu8; MESSAGE_SIZE];
        // Put unique markers
        data[0] = i as u8;
        data[MESSAGE_SIZE - 1] = i as u8;

        match producer.send(&data) {
            Ok(_) => {
                println!("✓ Sent message {} ({} bytes) - triggers SFU overflow", i, MESSAGE_SIZE);
            }
            Err(e) => {
                eprintln!("✗ Failed to send message {}: {:?}", i, e);
            }
        }

        // Small delay to avoid overwhelming the consumer
        thread::sleep(Duration::from_millis(100));
    }

    println!();
    println!("All {} messages sent!", NUM_MESSAGES);
    println!("Keeping producer alive for 10 seconds to allow consumer to read...");

    // Keep the process alive so consumer can read
    thread::sleep(Duration::from_secs(10));

    println!("Producer exiting.");
}
