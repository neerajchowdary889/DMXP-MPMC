// In examples/consumer.rs
use dmxp_kvcache::MPMC::ChannelBuilder;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <num_channels> <expected_messages_per_channel>",
            args[0]
        );
        std::process::exit(1);
    }

    let num_channels: usize = args[1].parse().expect("Invalid number of channels");
    let expected_per_channel: usize = args[2].parse().expect("Invalid messages per channel");
    let total_expected = num_channels * expected_per_channel;

    println!("Consumer: Waiting for {} channels...", num_channels);

    // First, try to attach to shared memory and see what channels exist
    match dmxp_kvcache::Core::alloc::SharedMemoryAllocator::attach(128 * 1024 * 1024) {
        Ok(allocator) => {
            println!("Consumer: Successfully attached to shared memory");
            println!(
                "Consumer: Found {} active channels",
                allocator.channel_count()
            );

            // List all available channels
            let all_channels = allocator.get_channels();
            println!(
                "Consumer: Available channel IDs: {:?}",
                all_channels.iter().map(|c| c.id()).collect::<Vec<_>>()
            );
        }
        Err(e) => {
            eprintln!("Consumer: Failed to attach to shared memory: {}", e);
            eprintln!("Make sure the producer is running first!");
            return Ok(());
        }
    }

    // Create consumers for all channels
    let mut consumers = Vec::new();
    for channel_id in 0..num_channels {
        match ChannelBuilder::new()
            .with_channel_id(channel_id as u32)
            .build_consumer()
        {
            Ok(consumer) => {
                consumers.push(consumer);
            }
            Err(e) => {
                eprintln!(
                    "Failed to create consumer for channel {}: {}",
                    channel_id, e
                );
                return Ok(());
            }
        }
    }

    println!("Consumer: Found {} channels", consumers.len());
    println!("Consumer: Receiving messages from all channels...");

    // Start receiving from all channels in round-robin
    let start = std::time::Instant::now();
    let mut total_received = 0;
    let mut channel_received = vec![0; num_channels];
    let mut consecutive_empty = 0;
    let max_consecutive_empty = 1000; // Stop if no messages for 1000 iterations

    while total_received < total_expected {
        let mut received_this_round = false;

        // Try to receive from each channel
        for (channel_id, consumer) in consumers.iter().enumerate() {
            match consumer.receive() {
                Ok(Some(_data)) => {
                    total_received += 1;
                    channel_received[channel_id] += 1;
                    received_this_round = true;
                    consecutive_empty = 0;
                }
                Ok(None) => {
                    // No message available
                }
                Err(e) => {
                    eprintln!("Channel {}: Error receiving message: {}", channel_id, e);
                }
            }
        }

        if !received_this_round {
            consecutive_empty += 1;
            if consecutive_empty >= max_consecutive_empty {
                eprintln!(
                    "No messages received for {} iterations, stopping...",
                    max_consecutive_empty
                );
                break;
            }
            std::thread::yield_now();
        }

        // Progress indicator
        if total_received % 1000 == 0 && total_received > 0 {
            println!("  Received {} messages", total_received);
        }
    }

    let elapsed = start.elapsed();

    // Print statistics
    println!("\n{}", "=".repeat(80));
    println!("CONSUMER STATISTICS");
    println!("{}", "=".repeat(80));
    println!("Channels:                 {}", num_channels);
    println!("Expected per channel:     {}", expected_per_channel);
    println!("Total expected:           {}", total_expected);
    println!("Total received:           {}", total_received);
    println!("Time taken:               {:.3?}", elapsed);
    println!(
        "Throughput (TPS):         {:.2} messages/sec",
        total_received as f64 / elapsed.as_secs_f64()
    );
    println!(
        "Per-channel TPS:          {:.2} messages/sec",
        (total_received as f64 / num_channels as f64) / elapsed.as_secs_f64()
    );
    println!();
    println!("Per-Channel Breakdown:");
    println!("{:-<80}", "");

    for (channel_id, received) in channel_received.iter().enumerate() {
        let percentage = (*received as f64 / expected_per_channel as f64) * 100.0;
        println!(
            "  Channel {:2}: {:6} messages ({:5.1}%)",
            channel_id, received, percentage
        );
    }

    println!("{}", "=".repeat(80));

    if total_received == total_expected {
        println!("✓ All messages received successfully!");
    } else {
        println!(
            "⚠ Warning: Expected {} but received {}",
            total_expected, total_received
        );
    }

    // Keep the consumer alive for a short time
    std::thread::sleep(std::time::Duration::from_secs(1));

    Ok(())
}
