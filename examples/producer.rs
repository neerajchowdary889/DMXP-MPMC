// In examples/producer.rs
use dmxp_kvcache::MPMC::ChannelBuilder;
use sha2::{Digest, Sha256};
use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <num_messages> [--auto-exit]", args[0]);
        std::process::exit(1);
    }

    let num_messages: usize = args[1].parse().expect("Invalid number of messages");
    let auto_exit = args.get(2).map(|s| s == "--auto-exit").unwrap_or(false);

    println!("Producer: Precomputing {} hashes...", num_messages);

    // Precompute hashes
    let start_precompute = std::time::Instant::now();
    let mut hashes = Vec::with_capacity(num_messages);

    for i in 0..num_messages {
        let mut hasher = Sha256::new();
        hasher.update(format!("message_{}", i).as_bytes());
        let result = hasher.finalize();
        let hash_hex = format!("{:x}", result);
        hashes.push(hash_hex);
    }

    let precompute_time = start_precompute.elapsed();
    println!(
        "Producer: Precomputed {} hashes in {:.2?}",
        num_messages, precompute_time
    );

    // Create producer
    let producer = ChannelBuilder::new().with_channel_id(0).build_producer()?;

    let keep_alive = Arc::new(producer.keep_alive().clone());
    let keep_alive_for_handler = Arc::clone(&keep_alive);

    // Handle Ctrl+C to clean up
    ctrlc::set_handler(move || {
        keep_alive_for_handler.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    println!("Producer: Created channel {}", producer.channel_id());
    println!(
        "Producer: Sending {} hashes to shared memory...",
        num_messages
    );

    // Send hashes to shared memory
    let start_send = std::time::Instant::now();
    let mut sent = 0;

    for (i, hash) in hashes.iter().enumerate() {
        // Format: "message_number:hash"
        let message = format!("{}:{}", i, hash);

        loop {
            match producer.send(&message) {
                Ok(()) => {
                    sent += 1;
                    if sent % 100 == 0 {
                        println!("Sent {} messages", sent);
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Buffer full, retry
                    std::thread::sleep(std::time::Duration::from_micros(10));
                }
                Err(e) => {
                    eprintln!("Failed to send message {}: {}", i, e);
                    break;
                }
            }
        }
    }

    let send_time = start_send.elapsed();
    println!("Producer: Sent {} messages in {:.2?}", sent, send_time);
    println!(
        "Producer: Throughput: {:.2} messages/sec",
        sent as f64 / send_time.as_secs_f64()
    );

    if auto_exit {
        // In auto-exit mode, just wait a bit for messages to be consumed
        println!("Producer: Auto-exit mode, waiting 2 seconds for consumer...");
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("Producer: Shutting down");
    } else {
        // Interactive mode: wait for Ctrl+C
        println!("Press Ctrl+C to exit...");
        println!("Waiting for consumer to finish processing (press Ctrl+C to exit)...");

        // Keep the producer alive until Ctrl+C
        while keep_alive.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        println!("Producer: Shutting down");
    }

    Ok(())
}
