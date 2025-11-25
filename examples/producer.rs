// In examples/producer.rs
use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;
use dmxp_kvcache::MPMC::ChannelBuilder;
use sha2::{Digest, Sha256};
use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <num_channels> <messages_per_channel> [capacity] [--auto-exit]",
            args[0]
        );
        eprintln!("  capacity: optional, number of slots per channel (default: 1024)");
        std::process::exit(1);
    }

    let num_channels: usize = args[1].parse().expect("Invalid number of channels");
    let messages_per_channel: usize = args[2].parse().expect("Invalid messages per channel");

    // Parse capacity (default 1024)
    let capacity: usize = args
        .get(3)
        .and_then(|s| {
            if s == "--auto-exit" {
                None
            } else {
                s.parse().ok()
            }
        })
        .unwrap_or(1024);

    let auto_exit = args.iter().any(|s| s == "--auto-exit");

    println!(
        "Producer: Connecting to or creating {} channels with {} messages each",
        num_channels, messages_per_channel
    );
    println!("Producer: Channel capacity: {} slots", capacity);

    // Precompute hashes
    let start_precompute = std::time::Instant::now();
    let mut hashes = Vec::with_capacity(messages_per_channel);

    for i in 0..messages_per_channel {
        let mut hasher = Sha256::new();
        hasher.update(format!("message_{}", i).as_bytes());
        let result = hasher.finalize();
        let hash_hex = format!("{:x}", result);
        hashes.push(hash_hex);
    }

    let precompute_time = start_precompute.elapsed();
    println!(
        "Producer: Precomputed {} hashes in {:.2?}",
        messages_per_channel, precompute_time
    );

    // Try to attach to existing shared memory first to see what channels exist
    let existing_channels = match SharedMemoryAllocator::attach(128 * 1024 * 1024) {
        Ok(allocator) => {
            let channels = allocator.get_channels();
            println!(
                "Producer: Found existing shared memory with {} channels",
                channels.len()
            );
            for ch in &channels {
                println!("  - Channel {}: {} slots", ch.id(), ch.capacity);
            }
            channels.into_iter().map(|ch| ch.id()).collect::<Vec<_>>()
        }
        Err(_) => {
            println!("Producer: No existing shared memory found, will create new channels");
            Vec::new()
        }
    };

    // Create or connect to producers for all channels
    let mut producers = Vec::new();
    for channel_id in 0..num_channels {
        let channel_id_u32 = channel_id as u32;

        if existing_channels.contains(&channel_id_u32) {
            // Channel exists, connect to it
            match ChannelBuilder::new()
                .with_channel_id(channel_id_u32)
                .with_capacity(capacity)
                .build_producer()
            {
                Ok(producer) => {
                    println!("Producer: Connected to existing channel {}", channel_id);
                    producers.push(producer);
                }
                Err(e) => {
                    eprintln!(
                        "Producer: Failed to connect to channel {}: {}",
                        channel_id, e
                    );
                    return Err(e);
                }
            }
        } else {
            // Channel doesn't exist, create it
            match ChannelBuilder::new()
                .with_channel_id(channel_id_u32)
                .with_capacity(capacity)
                .build_producer()
            {
                Ok(producer) => {
                    println!("Producer: Created new channel {}", channel_id);
                    producers.push(producer);
                }
                Err(e) => {
                    eprintln!("Producer: Failed to create channel {}: {}", channel_id, e);
                    return Err(e);
                }
            }
        }
    }

    println!("Producer: Ready with {} channels", producers.len());

    let keep_alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let keep_alive_for_handler = Arc::clone(&keep_alive);

    // Handle Ctrl+C to clean up
    ctrlc::set_handler(move || {
        keep_alive_for_handler.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    println!("Producer: Sending messages to all channels...");

    // Send messages to all channels in round-robin fashion
    let start_send = std::time::Instant::now();
    let mut total_sent = 0;
    let mut channel_sent = vec![0; num_channels];
    let mut channel_full = vec![false; num_channels];

    // Round-robin through channels
    for msg_idx in 0..messages_per_channel {
        for (channel_id, producer) in producers.iter().enumerate() {
            // Skip channels that are full
            if channel_full[channel_id] {
                continue;
            }

            let hash = &hashes[msg_idx];
            let message = format!("{}:{}:{}", channel_id, msg_idx, hash);

            // Try to send with limited retries
            let mut retries = 0;
            let max_retries = 10;

            loop {
                match producer.send(&message) {
                    Ok(()) => {
                        total_sent += 1;
                        channel_sent[channel_id] += 1;
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        retries += 1;
                        if retries >= max_retries {
                            println!("Channel {}: Buffer full, stopping sends to this channel (sent {} messages)", 
                                     channel_id, channel_sent[channel_id]);
                            channel_full[channel_id] = true;
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                    Err(e) => {
                        eprintln!(
                            "Channel {}: Failed to send message {}: {}",
                            channel_id, msg_idx, e
                        );
                        channel_full[channel_id] = true;
                        break;
                    }
                }
            }
        }

        // Check if all channels are full
        if channel_full.iter().all(|&full| full) {
            println!("All channels are full, stopping at message {}", msg_idx);
            break;
        }

        // Progress indicator
        if (msg_idx + 1) % 100 == 0 {
            let active_channels = channel_full.iter().filter(|&&full| !full).count();
            println!(
                "  Sent {} messages per active channel ({} total, {} channels active)",
                msg_idx + 1,
                total_sent,
                active_channels
            );
        }
    }

    let send_time = start_send.elapsed();

    // Print statistics
    println!("\n{}", "=".repeat(80));
    println!("PRODUCER STATISTICS");
    println!("{}", "=".repeat(80));
    println!("Channels:              {}", num_channels);
    println!("Channel capacity:      {} slots", capacity);
    println!("Messages per channel:  {}", messages_per_channel);
    println!("Total messages sent:   {}", total_sent);
    println!("Time taken:            {:.3?}", send_time);
    println!(
        "Throughput (TPS):      {:.2} messages/sec",
        total_sent as f64 / send_time.as_secs_f64()
    );
    println!(
        "Per-channel TPS:       {:.2} messages/sec",
        (total_sent as f64 / num_channels as f64) / send_time.as_secs_f64()
    );
    println!();
    println!("Per-Channel Breakdown:");
    println!("{:-<80}", "");

    for (channel_id, sent) in channel_sent.iter().enumerate() {
        println!("  Channel {:2}: {:6} messages", channel_id, sent);
    }

    println!("{}", "=".repeat(80));

    if auto_exit {
        println!("Producer: Auto-exit mode, waiting 2 seconds for consumer...");
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("Producer: Shutting down");
    } else {
        println!("Press Ctrl+C to exit...");
        while keep_alive.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        println!("Producer: Shutting down");
    }

    Ok(())
}
