use crossbeam_utils::CachePadded;
use dmxp_kvcache::MPMC::Buffer::layout::ChannelEntry;
use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use dmxp_kvcache::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::alloc::{alloc, Layout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn create_dummy_channel_entry(capacity: u64) -> ChannelEntry {
    ChannelEntry {
        channel_id: 0,
        flags: 0,
        capacity,
        band_offset: 0,
        signal: std::sync::atomic::AtomicU32::new(0),
        tail: CachePadded::new(AtomicU64::new(0)),
        head: CachePadded::new(AtomicU64::new(0)),
        _pad: [],
    }
}

fn make_aligned_backing(capacity: usize) -> (*mut u8, Layout) {
    let size = capacity * RingBuffer::slot_stride();
    let layout = Layout::from_size_align(size, 128).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        panic!("Failed to allocate aligned memory");
    }
    (ptr, layout)
}

#[test]
fn mpmc_correctness_many_threads() {
    let capacity = 1024;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let entry = Box::new(entry);
    let entry_ptr: *const ChannelEntry = &*entry;

    struct SendRingBuffer(RingBuffer);
    unsafe impl Send for SendRingBuffer {}
    unsafe impl Sync for SendRingBuffer {}

    let buffer = Arc::new(SendRingBuffer(unsafe { RingBuffer::new(entry_ptr, ptr) }));
    unsafe {
        buffer.0.init_slots();
    }

    let producers = 4;
    let consumers = 4;
    let msgs_per_producer = 1000;
    let total_msgs = producers * msgs_per_producer;

    let mut handles = vec![];

    // Spawn producers
    for p_id in 0..producers {
        let buffer = buffer.clone();
        handles.push(thread::spawn(move || {
            let mut meta = MessageMeta::default();
            meta.message_id = p_id as u64; // Use message_id to track producer

            for i in 0..msgs_per_producer {
                let payload = vec![i as u8]; // Simple payload
                while buffer.0.enqueue(meta, &payload).is_none() {
                    thread::yield_now();
                }
            }
        }));
    }

    // Spawn consumers
    let received_count = Arc::new(AtomicU64::new(0));
    for _ in 0..consumers {
        let buffer = buffer.clone();
        let received_count = received_count.clone();
        handles.push(thread::spawn(move || loop {
            if let Some((_meta, _data)) = buffer.0.dequeue() {
                received_count.fetch_add(1, Ordering::Relaxed);
            } else {
                if received_count.load(Ordering::Relaxed) >= total_msgs as u64 {
                    break;
                }
                thread::yield_now();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(received_count.load(Ordering::SeqCst), total_msgs as u64);

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[test]
fn mpmc_throughput_print() {
    let capacity = 4096;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let entry = Box::new(entry);
    let entry_ptr: *const ChannelEntry = &*entry;

    struct SendRingBuffer(RingBuffer);
    unsafe impl Send for SendRingBuffer {}
    unsafe impl Sync for SendRingBuffer {}

    let buffer = Arc::new(SendRingBuffer(unsafe { RingBuffer::new(entry_ptr, ptr) }));
    unsafe {
        buffer.0.init_slots();
    }

    let start = std::time::Instant::now();
    let count = 100_000;

    let b_prod = buffer.clone();
    let p = thread::spawn(move || {
        let meta = MessageMeta::default();
        let payload = vec![0u8; 8];
        for _ in 0..count {
            while b_prod.0.enqueue(meta, &payload).is_none() {
                std::hint::spin_loop();
            }
        }
    });

    let b_cons = buffer.clone();
    let c = thread::spawn(move || {
        let mut rx = 0;
        while rx < count {
            if b_cons.0.dequeue().is_some() {
                rx += 1;
            } else {
                std::hint::spin_loop();
            }
        }
    });

    p.join().unwrap();
    c.join().unwrap();

    let elapsed = start.elapsed();
    println!(
        "Throughput: {:.2} million ops/sec",
        (count as f64 / elapsed.as_secs_f64()) / 1_000_000.0
    );

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}
