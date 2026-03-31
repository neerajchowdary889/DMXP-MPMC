/// tui_demo — continuous MPMC traffic with LARGE overflow messages for live TUI observation.
///
/// Run this in one terminal, then `cargo run -p dmxp-tui` in another to see real-time SFU stats.
///
/// Usage: cargo run --example tui_demo
///
/// Four channels with increasing message sizes to stress-test SFU:
///   0 — small messages (100 bytes), high rate
///   1 — medium messages (5 MB), medium rate → triggers SFU overflow
///   2 — large messages (20 MB), slower rate → heavy SFU usage
///   3 — huge messages (50 MB), low rate → maximum SFU stress
use dmxp_mpmc::MPMC::ChannelBuilder;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const SHM_SIZE: usize = 256 * 1024 * 1024; // 256MB for large messages

// Message sizes
const SMALL_SIZE: usize = 100;
const MEDIUM_SIZE: usize = 5 * 1024 * 1024;   // 5 MB
const LARGE_SIZE: usize = 20 * 1024 * 1024;   // 20 MB
const HUGE_SIZE: usize = 50 * 1024 * 1024;    // 50 MB

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn main() -> std::io::Result<()> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║        TUI Demo with LARGE Overflow Messages                   ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");
    println!("This demo generates large messages to stress-test the SFU shared backend.");
    println!("Open another terminal and run: cargo run -p dmxp-tui\n");
    println!("Channel configuration:");
    println!("  Ch 0: {} messages (inline, no overflow)", format_bytes(SMALL_SIZE));
    println!("  Ch 1: {} messages (SFU overflow)", format_bytes(MEDIUM_SIZE));
    println!("  Ch 2: {} messages (heavy SFU)", format_bytes(LARGE_SIZE));
    println!("  Ch 3: {} messages (maximum SFU stress)\n", format_bytes(HUGE_SIZE));

    let stop = Arc::new(AtomicBool::new(false));
    let stop_handler = stop.clone();
    ctrlc::set_handler(move || {
        println!("\n\nShutting down...");
        stop_handler.store(true, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // Counters for terminal output
    let ch0_sent = Arc::new(AtomicU64::new(0));
    let ch1_sent = Arc::new(AtomicU64::new(0));
    let ch2_sent = Arc::new(AtomicU64::new(0));
    let ch3_sent = Arc::new(AtomicU64::new(0));

    // Build producers
    let p0 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(0)
        .with_capacity(1024)
        .build_producer()?;

    let p1 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(1)
        .with_capacity(128)
        .build_producer()?;

    let p2 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(2)
        .with_capacity(64)
        .build_producer()?;

    let p3 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(3)
        .with_capacity(32)
        .build_producer()?;

    // Build consumers
    let c0 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(0)
        .with_capacity(1024)
        .build_consumer()?;

    let c1 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(1)
        .with_capacity(128)
        .build_consumer()?;

    let c2 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(2)
        .with_capacity(64)
        .build_consumer()?;

    let c3 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(3)
        .with_capacity(32)
        .build_consumer()?;

    println!("✓ All channels ready. Starting traffic generation...\n");

    // Producer threads with terminal output
    let s0 = stop.clone();
    let counter0 = ch0_sent.clone();
    thread::spawn(move || {
        let msg = vec![0x01u8; SMALL_SIZE];
        while !s0.load(Ordering::Relaxed) {
            if p0.send(&msg).is_ok() {
                counter0.fetch_add(1, Ordering::Relaxed);
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    let s1 = stop.clone();
    let counter1 = ch1_sent.clone();
    thread::spawn(move || {
        let msg = vec![0x02u8; MEDIUM_SIZE];
        let start = Instant::now();
        while !s1.load(Ordering::Relaxed) {
            let send_start = Instant::now();
            if p1.send(&msg).is_ok() {
                let elapsed = send_start.elapsed();
                let count = counter1.fetch_add(1, Ordering::Relaxed) + 1;
                println!("[Ch1] Sent message #{} ({}) in {:?} | Runtime: {:?}",
                         count, format_bytes(MEDIUM_SIZE), elapsed, start.elapsed());
            }
            thread::sleep(Duration::from_millis(500));
        }
    });

    let s2 = stop.clone();
    let counter2 = ch2_sent.clone();
    thread::spawn(move || {
        let msg = vec![0x03u8; LARGE_SIZE];
        let start = Instant::now();
        while !s2.load(Ordering::Relaxed) {
            let send_start = Instant::now();
            if p2.send(&msg).is_ok() {
                let elapsed = send_start.elapsed();
                let count = counter2.fetch_add(1, Ordering::Relaxed) + 1;
                println!("[Ch2] Sent message #{} ({}) in {:?} | Runtime: {:?}",
                         count, format_bytes(LARGE_SIZE), elapsed, start.elapsed());
            }
            thread::sleep(Duration::from_secs(2));
        }
    });

    let s3 = stop.clone();
    let counter3 = ch3_sent.clone();
    thread::spawn(move || {
        let msg = vec![0x04u8; HUGE_SIZE];
        let start = Instant::now();
        while !s3.load(Ordering::Relaxed) {
            let send_start = Instant::now();
            if p3.send(&msg).is_ok() {
                let elapsed = send_start.elapsed();
                let count = counter3.fetch_add(1, Ordering::Relaxed) + 1;
                println!("[Ch3] Sent message #{} ({}) in {:?} | Runtime: {:?}",
                         count, format_bytes(HUGE_SIZE), elapsed, start.elapsed());
            }
            thread::sleep(Duration::from_secs(5));
        }
    });

    // Consumer threads (fast consumption to keep ring buffers flowing)
    let s0 = stop.clone();
    thread::spawn(move || {
        while !s0.load(Ordering::Relaxed) {
            let _ = c0.receive();
            thread::sleep(Duration::from_micros(100));
        }
    });

    let s1 = stop.clone();
    thread::spawn(move || {
        let mut count = 0u64;
        while !s1.load(Ordering::Relaxed) {
            if let Ok(Some(data)) = c1.receive() {
                count += 1;
                println!("[Ch1] Consumed message #{} ({} bytes)", count, data.len());
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    let s2 = stop.clone();
    thread::spawn(move || {
        let mut count = 0u64;
        while !s2.load(Ordering::Relaxed) {
            if let Ok(Some(data)) = c2.receive() {
                count += 1;
                println!("[Ch2] Consumed message #{} ({} bytes)", count, data.len());
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    let s3 = stop.clone();
    thread::spawn(move || {
        let mut count = 0u64;
        while !s3.load(Ordering::Relaxed) {
            if let Ok(Some(data)) = c3.receive() {
                count += 1;
                println!("[Ch3] Consumed message #{} ({} bytes)", count, data.len());
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // Status reporter
    let stop_reporter = stop.clone();
    thread::spawn(move || {
        let start = Instant::now();
        let mut last_report = Instant::now();

        while !stop_reporter.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(10));

            if last_report.elapsed() >= Duration::from_secs(10) {
                let ch0 = ch0_sent.load(Ordering::Relaxed);
                let ch1 = ch1_sent.load(Ordering::Relaxed);
                let ch2 = ch2_sent.load(Ordering::Relaxed);
                let ch3 = ch3_sent.load(Ordering::Relaxed);

                println!("\n━━━━━━━━━━━━━━━━ Status Report ━━━━━━━━━━━━━━━━");
                println!("Runtime: {:?}", start.elapsed());
                println!("Ch0 ({}): {} messages sent", format_bytes(SMALL_SIZE), ch0);
                println!("Ch1 ({}): {} messages sent", format_bytes(MEDIUM_SIZE), ch1);
                println!("Ch2 ({}): {} messages sent", format_bytes(LARGE_SIZE), ch2);
                println!("Ch3 ({}): {} messages sent", format_bytes(HUGE_SIZE), ch3);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

                last_report = Instant::now();
            }
        }
    });

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }

    println!("\n✓ Shutdown complete.");
    Ok(())
}
