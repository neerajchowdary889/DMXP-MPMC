use crossbeam_utils::CachePadded;
use dmxp_mpmc::MPMC::Buffer::layout::ChannelEntry;
use dmxp_mpmc::MPMC::Buffer::RingBuffer;
use dmxp_mpmc::MPMC::Structs::Buffer_Structs::MessageMeta;
use std::alloc::{alloc, Layout};
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use dmxp_mpmc::Core::sfu::BlobStoreBuilder;
use sfb::PinnedBlobStore;
use std::thread;

/// Atomic counter to generate unique shm namespaces per test.
static TEST_NS_COUNTER: AtomicU32 = AtomicU32::new(0);

fn make_test_sfu() -> Arc<PinnedBlobStore> {
    let id = TEST_NS_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ns = format!("dmxp_test_{}", id);
    BlobStoreBuilder::new()
        .with_shared_mode(&ns, 4 * 1024 * 1024) // 4 MB chunks for tests
        .build()
        .expect("test SFU")
}

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
    let rb = unsafe { RingBuffer::new(&entry, ptr, make_test_sfu()) };
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
    let rb = unsafe { RingBuffer::new(&entry, ptr, make_test_sfu()) };
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
fn small_spsc_fifo_correctness() {
    let capacity = 8;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let entry = Box::new(entry);
    let entry_ptr: *const ChannelEntry = &*entry;

    struct SendRingBuffer(RingBuffer);
    unsafe impl Send for SendRingBuffer {}
    unsafe impl Sync for SendRingBuffer {}

    let rb = Arc::new(SendRingBuffer(unsafe { RingBuffer::new(entry_ptr, ptr, make_test_sfu()) }));
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

/// Verify that messages exceeding the 90% overflow threshold are correctly
/// stored in the SharedBackend and retrieved intact via OverflowHandle.
#[test]
fn overflow_roundtrip_shared() {
    let capacity = 16;
    let (ptr, layout) = make_aligned_backing(capacity);

    let entry = create_dummy_channel_entry(capacity as u64);
    let rb = unsafe { RingBuffer::new(&entry, ptr, make_test_sfu()) };
    unsafe {
        rb.init_slots();
    }

    // MSG_INLINE = 1024, overflow threshold = 90% = 921.
    // Payload of 950 bytes triggers overflow.
    let overflow_payload: Vec<u8> = (0..950).map(|i| (i % 256) as u8).collect();

    let mut meta = MessageMeta::default();
    meta.overflow = 1; // Mark as overflow

    // Enqueue
    let idx = rb.enqueue(meta, &overflow_payload);
    assert!(idx.is_some(), "enqueue of overflow message should succeed");

    // Dequeue — the ring buffer should resolve the OverflowHandle via SharedBackend
    let result = rb.dequeue();
    assert!(result.is_some(), "dequeue of overflow message should succeed");

    let (out_meta, data) = result.unwrap();
    assert_eq!(data.len(), overflow_payload.len(), "overflow data length mismatch");
    assert_eq!(data, overflow_payload, "overflow data content mismatch");
    assert_eq!(out_meta.payload_len, overflow_payload.len() as u32);

    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

