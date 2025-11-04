use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn make_backing(capacity: usize) -> Vec<u8> { vec![0u8; capacity * dmxp_kvcache::MPMC::Buffer::RingBuffer::slot_stride()] }

#[test]
fn mpmc_correctness_many_threads() {
    let capacity = 1 << 12; // 4096 slots
    let mut backing = make_backing(capacity);
    let ptr = backing.as_mut_ptr();
    let rb = Arc::new(RingBuffer::new(ptr, capacity));
    unsafe { rb.init_slots(); }

    let producers = 4usize;
    let consumers = 4usize;
    let per_producer = 50_000u64;
    let total = per_producer * producers as u64;

    let produced = Arc::new(AtomicU64::new(0));
    let consumed = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..producers {
        let rb = rb.clone();
        let produced = produced.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..per_producer {
                loop { if rb.enqueue(1).is_some() { produced.fetch_add(1, Relaxed); break; } std::hint::spin_loop(); }
            }
        }));
    }
    for _ in 0..consumers {
        let rb = rb.clone();
        let consumed = consumed.clone();
        handles.push(thread::spawn(move || {
            while consumed.load(Relaxed) < total {
                if let Some((_idx, _len)) = rb.dequeue() { consumed.fetch_add(1, Relaxed); }
                else { std::hint::spin_loop(); }
            }
        }));
    }
    for h in handles { let _ = h.join(); }

    assert_eq!(produced.load(Relaxed), total);
    assert_eq!(consumed.load(Relaxed), total);
}

#[test]
fn mpmc_throughput_print() {
    let capacity = 1 << 12; // 4096
    let mut backing = make_backing(capacity);
    let ptr = backing.as_mut_ptr();
    let rb = Arc::new(RingBuffer::new(ptr, capacity));
    unsafe { rb.init_slots(); }

    let producers = 4usize;
    let consumers = 4usize;
    let per_producer = 250_000u64; // total 1_000_000
    let total = per_producer * producers as u64;

    let consumed = Arc::new(AtomicU64::new(0));

    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..producers {
        let rb = rb.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..per_producer { while rb.enqueue(1).is_none() { std::hint::spin_loop(); } }
        }));
    }
    for _ in 0..consumers {
        let rb = rb.clone();
        let consumed = consumed.clone();
        handles.push(thread::spawn(move || {
            while consumed.load(Relaxed) < total {
                if rb.dequeue().is_some() { consumed.fetch_add(1, Relaxed); } else { std::hint::spin_loop(); }
            }
        }));
    }
    for h in handles { let _ = h.join(); }
    let dur = start.elapsed();
    let msgs_per_sec = (total as f64) / dur.as_secs_f64();
    println!("Throughput: {:.1} msgs/sec (total={}, duration={:?})", msgs_per_sec, total, dur);
    // Not asserting a hard floor to avoid flakiness across CI environments.
}