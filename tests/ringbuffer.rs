use crossbeam_utils::CachePadded;
use dmxp_kvcache::MPMC::Buffer::layout::ChannelEntry;
use dmxp_kvcache::MPMC::Buffer::RingBuffer;
use dmxp_kvcache::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::alloc::{alloc, Layout};
use std::sync::atomic::AtomicU64;
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
fn simple_enqueue_dequeue() {
    let capacity = 16;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr) };
    unsafe {
        rb.init_slots();
    }

    let meta = MessageMeta::default();
    let payload = vec![1, 2, 3, 4];

    // Enqueue
    let idx = rb.enqueue(meta, &payload);
    assert!(idx.is_some());

    // Dequeue
    let result = rb.dequeue();
    assert!(result.is_some());
    let (_meta_out, data) = result.unwrap();
    assert_eq!(data, payload);

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[test]
fn full_buffer() {
    let capacity = 4;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr) };
    unsafe {
        rb.init_slots();
    }

    let meta = MessageMeta::default();
    let payload = vec![0u8; 8];

    // Fill buffer
    for _ in 0..4 {
        assert!(rb.enqueue(meta, &payload).is_some());
    }

    // Next enqueue should fail
    assert!(rb.enqueue(meta, &payload).is_none());

    // Dequeue one
    assert!(rb.dequeue().is_some());

    // Enqueue should succeed now
    assert!(rb.enqueue(meta, &payload).is_some());

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[test]
fn small_mpmc_correctness() {
    let capacity = 8;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let entry = Box::new(entry);
    let entry_ptr: *const ChannelEntry = &*entry;

    struct SendRingBuffer(RingBuffer);
    unsafe impl Send for SendRingBuffer {}
    unsafe impl Sync for SendRingBuffer {}

    let rb = Arc::new(SendRingBuffer(unsafe { RingBuffer::new(entry_ptr, ptr) }));
    unsafe {
        rb.0.init_slots();
    }

    let rb_prod = rb.clone();
    let p = thread::spawn(move || {
        let meta = MessageMeta::default();
        for i in 0..100 {
            let payload = vec![i as u8];
            while rb_prod.0.enqueue(meta, &payload).is_none() {
                std::hint::spin_loop();
            }
        }
    });

    let rb_cons = rb.clone();
    let c = thread::spawn(move || {
        let mut count = 0;
        while count < 100 {
            if let Some((_meta, data)) = rb_cons.0.dequeue() {
                assert_eq!(data[0], count as u8);
                count += 1;
            } else {
                std::hint::spin_loop();
            }
        }
    });

    p.join().unwrap();
    c.join().unwrap();

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}
