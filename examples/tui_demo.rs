/// tui_demo — continuous MPMC traffic for live TUI observation.
///
/// Run this in one terminal, then `cargo run -p dmxp-tui` in another.
///
/// Usage: cargo run --example tui_demo
///
/// Three channels:
///   0 — small messages, high rate   (~200µs interval)
///   1 — medium messages, slower     (~800µs interval)
///   2 — large messages (>SFB threshold), low rate  (~5ms interval)
use dmxp_mpmc::MPMC::ChannelBuilder;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const SHM_SIZE: usize = 128 * 1024 * 1024;

fn main() -> std::io::Result<()> {
    println!("tui_demo: starting 3 channels. Open another terminal and run:");
    println!("  cargo run -p dmxp-tui\n");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_handler = stop.clone();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        stop_handler.store(true, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // Build producers
    let p0 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(0)
        .with_capacity(1024)
        .build_producer()?;

    let p1 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(1)
        .with_capacity(512)
        .build_producer()?;

    let p2 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(2)
        .with_capacity(256)
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
        .with_capacity(512)
        .build_consumer()?;

    let c2 = ChannelBuilder::new()
        .with_buffer_size(SHM_SIZE)
        .with_channel_id(2)
        .with_capacity(256)
        .build_consumer()?;

    println!("tui_demo: all channels ready. Producing traffic...");

    // Producer threads
    let s0 = stop.clone();
    thread::spawn(move || {
        let msg = b"ch0:small payload".to_vec();
        while !s0.load(Ordering::Relaxed) {
            let _ = p0.send(&msg);
            thread::sleep(Duration::from_micros(200));
        }
    });

    let s1 = stop.clone();
    thread::spawn(move || {
        let msg = vec![0xABu8; 512];
        while !s1.load(Ordering::Relaxed) {
            let _ = p1.send(&msg);
            thread::sleep(Duration::from_micros(800));
        }
    });

    let s2 = stop.clone();
    thread::spawn(move || {
        // 950 bytes > 921-byte overflow threshold (90% of MSG_INLINE=1024) → overflows to SFB
        let msg = vec![0xFFu8; 950];
        while !s2.load(Ordering::Relaxed) {
            let _ = p2.send(&msg);
            thread::sleep(Duration::from_millis(5));
        }
    });

    // Consumer threads
    let s0 = stop.clone();
    thread::spawn(move || {
        while !s0.load(Ordering::Relaxed) {
            let _ = c0.receive();
            thread::sleep(Duration::from_micros(500));
        }
    });

    let s1 = stop.clone();
    thread::spawn(move || {
        while !s1.load(Ordering::Relaxed) {
            let _ = c1.receive();
            thread::sleep(Duration::from_micros(1200));
        }
    });

    let s2 = stop.clone();
    thread::spawn(move || {
        while !s2.load(Ordering::Relaxed) {
            let _ = c2.receive();
            thread::sleep(Duration::from_millis(8));
        }
    });

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}
