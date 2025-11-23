// In examples/consumer.rs
use dmxp_kvcache::MPMC::ChannelBuilder;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <expected_messages>", args[0]);
        std::process::exit(1);
    }

    let expected_messages: usize = args[1].parse().expect("Invalid number of messages");
    let mut received = 0;

    println!("Consumer: Waiting for channel 0...");

    // Create consumer - must match producer's 10MB shared memory size
    let consumer = match ChannelBuilder::new().with_channel_id(0).build_consumer() {
        Ok(consumer) => {
            println!("Consumer: Found channel 0");
            consumer
        }
        Err(e) => {
            eprintln!("Failed to create consumer: {}", e);
            return Ok(());
        }
    };

    // Receive messages
    let start = std::time::Instant::now();
    println!("\n{:<10} {}", "Msg #", "Hash");
    println!("{}", "=".repeat(80));

    while received < expected_messages {
        match consumer.receive() {
            Ok(Some(data)) => {
                if let Ok(message) = String::from_utf8(data) {
                    // Parse "message_number:hash" format
                    if let Some((num_str, hash)) = message.split_once(':') {
                        println!("{:<10} {}", num_str, hash);
                    } else {
                        println!("Invalid format: {}", message);
                    }
                }
                received += 1;

                // Show progress every 100 messages
                if received % 100 == 0 {
                    println!("--- Received {} messages ---", received);
                }
            }
            Ok(None) => {
                if start.elapsed() > std::time::Duration::from_secs(5) {
                    eprintln!("Timeout waiting for messages");
                    break;
                }
                std::thread::yield_now();
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                break;
            }
        }
    }

    let elapsed = start.elapsed();
    println!("\n{}", "=".repeat(80));
    println!(
        "Consumer: Received {} messages in {:.2?}",
        received, elapsed
    );
    println!(
        "Average: {:.2} messages/second",
        received as f64 / elapsed.as_secs_f64()
    );

    if received == expected_messages {
        println!("All messages received successfully");
    }

    // Keep the consumer alive for a short time to ensure all messages are processed
    std::thread::sleep(std::time::Duration::from_secs(1));

    Ok(())
}
