use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::thread;

fn make_backing(capacity: usize) -> Vec<u8> {
    vec![0u8; capacity * dmxp_kvcache::MPMC::Buffer::RingBuffer::slot_stride()]
}

#[test]
fn single_thread_basic_enqueue_dequeue() {
    let capacity = 8;
    let mut backing = make_backing(capacity);
    let ptr = backing.as_mut_ptr();
    let rb = RingBuffer::new(ptr, capacity);
    unsafe { rb.init_slots(); }

    // Initially empty
    assert!(rb.dequeue().is_none());

    // Enqueue up to capacity
    for i in 0..capacity {
        assert!(rb.enqueue(i as u64).is_some());
    }
    // Now should be full
    assert!(rb.enqueue(123).is_none());

    // Dequeue all and verify order and lengths
    for i in 0..capacity {
        let (idx, len) = rb.dequeue().expect("must dequeue");
        // idx order may wrap, but len must match insertion value
        assert_eq!(len, i as u64, "unexpected length at idx {idx}");
    }
    // Empty again
    assert!(rb.dequeue().is_none());
}

#[test]
fn ring_full_then_frees_slots() {
    let capacity = 4;
    let mut backing = make_backing(capacity);
    let ptr = backing.as_mut_ptr();
    let rb = RingBuffer::new(ptr, capacity);
    unsafe { rb.init_slots(); }

    for _ in 0..capacity { assert!(rb.enqueue(1).is_some()); }
    assert!(rb.enqueue(2).is_none(), "should report full");

    // Free one slot
    assert!(rb.dequeue().is_some());
    // Now there should be space for one more
    assert!(rb.enqueue(3).is_some());
}

#[test]
fn small_mpmc_correctness() {
    let capacity = 64;
    let mut backing = make_backing(capacity);
    let ptr = backing.as_mut_ptr();
    let rb = Arc::new(RingBuffer::new(ptr, capacity));
    unsafe { rb.init_slots(); }

    let producers = 2usize;
    let consumers = 2usize;
    let per_producer = 10_000u64;
    let total = per_producer * producers as u64;

    let consumed = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..producers {
        let rb = rb.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_producer {
                while rb.enqueue(i).is_none() { std::hint::spin_loop(); }
            }
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
    assert_eq!(consumed.load(Relaxed), total);
}


