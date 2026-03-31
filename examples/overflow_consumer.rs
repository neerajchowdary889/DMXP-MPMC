/// Example consumer that receives large overflow messages
/// Run overflow_producer first, then run this in another terminal

use dmxp_mpmc::MPMC::ChannelBuilder;
use std::thread;
use std::time::Duration;

fn main() {
    println!("=== Overflow Consumer Example ===");
    println!("This consumer receives large messages (2048 bytes) from SFU overflow");
    println!();

    // Give producer time to start
    println!("Waiting 1 second for producer to initialize...");
    thread::sleep(Duration::from_secs(1));

    println!("Creating consumer...");
    let consumer = ChannelBuilder::new()
        .with_channel_id(99)
        .with_capacity(16)
        .with_buffer_size(2 * 1024 * 1024) // 2MB
        .build_consumer()
        .expect("Failed to build consumer");

    println!("Consumer ready. Waiting for messages...");
    println!();

    const NUM_MESSAGES: usize = 10;
    const MESSAGE_SIZE: usize = 2048;
    let mut received_count = 0;

    for i in 0..NUM_MESSAGES {
        println!("Waiting for message {}...", i);

        match consumer.receive_blocking() {
            Ok(data) => {
                // Verify the message
                if data.len() != MESSAGE_SIZE {
                    eprintln!("✗ Message {} has wrong size: expected {}, got {}",
                             i, MESSAGE_SIZE, data.len());
                    continue;
                }

                if data[0] != i as u8 || data[MESSAGE_SIZE - 1] != i as u8 {
                    eprintln!("✗ Message {} has wrong content: markers don't match", i);
                    continue;
                }

                println!("✓ Received message {} ({} bytes) - retrieved from SFU overflow",
                         i, data.len());
                received_count += 1;
            }
            Err(e) => {
                eprintln!("✗ Failed to receive message {}: {:?}", i, e);
                break;
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("Received: {}/{} messages", received_count, NUM_MESSAGES);

    if received_count == NUM_MESSAGES {
        println!("✓ SUCCESS: All overflow messages received correctly!");
        println!("✓ Cross-process SFU overflow is working!");
    } else {
        println!("✗ FAILED: Some messages were not received");
    }
}
