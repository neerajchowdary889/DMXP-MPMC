# SFU (Stable Fragmented Buffer) — Problem & Path Forward

## What is the SFU?

The SFU (`PinnedBlobStore` from the `sfb` crate) is an overflow store for messages larger than the inline payload threshold. Every ring buffer slot holds up to `MSG_INLINE = 1024` bytes inline. Messages that exceed **90% of that (921 bytes)** are too large to be stored directly in the slot — instead:

1. The actual bytes are stored in the SFU (a heap-allocated, page-based arena).
2. A small `BlobHandle` (24 bytes) is written into the slot's payload field as a pointer/ticket.
3. The consumer reads the `BlobHandle` and calls `sfu.get(&handle)` to retrieve the real data.

```
Producer                          Ring Buffer Slot               Consumer
────────                          ────────────────               ────────
950-byte message                  meta.overflow = 1              reads slot
      │                           payload = BlobHandle {         calls sfu.get(handle)
      ▼                             page: 0, offset: 0,              │
SFU.append(950 bytes)               len: 950 }                        ▼
  returns BlobHandle ──────────►                          ◄────── retrieves data
```

---

## Problem 1 — Multiple SFU instances per process (fixed)

### Root cause

`ChannelBuilder::build_producer()` and `build_consumer()` each called `SharedMemoryAllocator::new/attach()` independently. Each allocator constructed its own `BlobStoreBuilder::default().build()`, producing a **separate `Arc<PinnedBlobStore>`**.

```
ChannelBuilder::build_producer()
  └─ SharedMemoryAllocator::new()
       └─ sfu = BlobStoreBuilder::default().build()  ← SFU_A

ChannelBuilder::build_consumer()
  └─ SharedMemoryAllocator::attach()
       └─ sfu = BlobStoreBuilder::default().build()  ← SFU_B (different object!)
```

Even within the same process, the producer's ring buffer used `SFU_A` and the consumer's ring buffer used `SFU_B`. A `BlobHandle` written by the producer into `SFU_A` was invisible to the consumer looking in `SFU_B`.

### Symptom

```
Failed to get from SFU: handle expired or invalid
```

Printed repeatedly by the consumer for every overflow message, which was then returned as an empty payload.

### Fix applied

A process-level `OnceLock<Arc<PinnedBlobStore>>` in `src/Core/alloc/mod.rs`:

```rust
static PROCESS_SFU: OnceLock<Arc<PinnedBlobStore>> = OnceLock::new();

fn process_sfu() -> Arc<PinnedBlobStore> {
    PROCESS_SFU.get_or_init(|| {
        BlobStoreBuilder::default().build().expect("...")
    }).clone()
}
```

Both `SharedMemoryAllocator::new()` and `::attach()` now call `process_sfu()`. All allocators, all channels, and all ring buffers within a process share the same `Arc<PinnedBlobStore>`.

```
After fix — same process:

build_producer()  →  SharedMemoryAllocator  →  PROCESS_SFU ─┐
                                                              │ same Arc
build_consumer()  →  SharedMemoryAllocator  →  PROCESS_SFU ─┘
```

**Status: fixed for same-process producers and consumers.**

---

## Problem 2 — Cross-process SFU (not fixed, by design)

### Root cause

The `PinnedBlobStore` allocates its pages on the **process heap** (regular RAM). Each process has its own isolated address space. Even with the `OnceLock` fix, two separate OS processes each have their own `PROCESS_SFU` static — they cannot see each other's heap.

```
Process A (producer)              /dev/shm                  Process B (consumer)
────────────────────              ──────────                 ────────────────────
Heap:                             Ring buffer:               Heap:
┌──────────────┐                  ┌──────────┐              ┌──────────────┐
│ PROCESS_SFU  │──writes──►  Slot │ handle H │──reads──►   │ PROCESS_SFU  │
│  [950 bytes] │  handle H   only │          │             │  (empty)     │
└──────────────┘                  └──────────┘             └──────────────┘
      ▲                                                            │
  actual data                                               get(H) → None ❌
  lives here                                            data was never here
```

The shared memory only carries the **ticket** (`BlobHandle`). The actual bytes stay in Process A's private heap, which Process B has no access to.

### Symptom

Same error as Problem 1, but now in a cross-process scenario (separate producer and consumer binaries). The consumer silently returns an empty payload for every overflow message.

### Why a background daemon does not help

An intuitive idea: spin up a background DMXP daemon that owns the SFU heap. Producers and consumers IPC into it to store/retrieve overflow data.

```
Process A ──IPC──► Daemon (owns SFU heap) ◄──IPC── Process B
```

This works logically but creates a new problem: **every overflow message now requires two IPC round-trips** (one to store, one to retrieve). On a Unix socket or pipe, each round-trip costs 10–100 µs. For a system targeting sub-microsecond message latency, this is a 10–100× regression for overflow messages. The daemon also becomes a serialization bottleneck for all overflow traffic across all channels.

The daemon's heap is still private memory. Other processes cannot touch it directly — they must go through IPC, which is the same isolation problem with extra overhead.

### Correct fix — overflow band in `/dev/shm`

Store overflow data directly in the shared memory region instead of the heap. All processes already map `/dev/shm/dmxp_alloc`; data written there is immediately visible to every attached process with zero IPC.

**Proposed layout:**

```
/dev/shm/dmxp_alloc (current):
┌─────────────────┬──────────────┬──────────────┬──────────────┐
│  GlobalHeader   │  Channel 0   │  Channel 1   │  Channel N   │
│  (98 KB)        │  ring slots  │  ring slots  │  ring slots  │
└─────────────────┴──────────────┴──────────────┴──────────────┘

/dev/shm/dmxp_alloc (proposed):
┌─────────────────┬──────────────┬──────────────┬──────────────┬───────────────────┐
│  GlobalHeader   │  Channel 0   │  Channel 1   │  Channel N   │  Overflow Band    │
│  (98 KB)        │  ring slots  │  ring slots  │  ring slots  │  (lock-free arena)│
└─────────────────┴──────────────┴──────────────┴──────────────┴───────────────────┘
                                                                         ▲
                                                           All processes read/write here
                                                           directly via mmap pointer.
                                                           No IPC. No daemon. ~0 ns overhead.
```

The `BlobHandle` becomes a simple `{ shm_offset: u64, len: u32 }` — an offset into the shared region. Any process can dereference it directly after mapping `/dev/shm/dmxp_alloc`.

---

## Summary

| Scenario | Status | Notes |
|---|---|---|
| Same process, multiple channels | ✅ Fixed | `OnceLock` ensures one SFU per process |
| Same process, multiple `ChannelBuilder` calls | ✅ Fixed | All share `PROCESS_SFU` |
| Cross-process (separate binaries) | ✅ Fixed | Shared SFU via `/dev/shm/dmxp_ovf_*` chunked arena |
| Cross-process via daemon + IPC | ❌ Wrong direction | Adds 10–100 µs latency per overflow message |
| Cross-process via `/dev/shm` overflow band | ✅ Fixed | Zero-IPC; `SharedBackend` in `sfb` crate |

## Relevant files

| File | Role |
|---|---|
| `src/Core/alloc/mod.rs` | `PROCESS_SFU` static; `process_sfu_create()` / `process_sfu_attach()` build shared-mode store |
| `src/Core/sfu/mod.rs` | `BlobStoreBuilder` wrapping the `sfb` crate; `with_shared_mode()` / `with_shared_attach()` |
| `src/MPMC/Buffer/Buffer_impl.rs` | `enqueue` writes overflow to SFU via `append_shared()`; `dequeue` reads via `resolve()` using `OverflowHandle` |
| `src/MPMC/producer.rs` | `overflow_flag()` — 90% of `MSG_INLINE` threshold |
| `sfb::backend::shared` | `SharedBackend` — lock-free `/dev/shm` chunked arena (control file + data chunks) |
| `sfb::types::overflow_handle` | `OverflowHandle` — 24-byte `#[repr(C)]` cross-process handle with `as_bytes()` / `from_bytes()` |

