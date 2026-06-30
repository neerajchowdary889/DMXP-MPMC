# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

DMXP-MPMC is an ultra-low latency, lock-free, multi-producer multi-consumer shared memory message queue. It uses `/dev/shm/dmxp_alloc` for zero-copy IPC and exposes a C FFI for cross-language bindings (Python, C, Go, Node.js).

**Critical dependency:** `sfb` (stable-fragmented-buffer) is a local path dependency at `../stable-fragmented-buffer`. The workspace will not build without it present at that sibling path. The TUI crate references it via git instead.

## Build & Test Commands

```bash
# Build
cargo build --release

# Run all tests (serial_test prevents parallel SHM conflicts)
cargo test

# Run specific test by name
cargo test mpmc_correctness_many_threads

# Run specific test file
cargo test --test mpmc

# Run with stdout
cargo test --test mpmc -- --nocapture

# Run examples (separate terminals)
cargo run --example producer -- 4 1000   # 4 channels, 1000 msgs each
cargo run --example consumer -- 4 1000

# TUI monitoring dashboard
cd tui && cargo run --bin tui
```

The crate builds as both `cdylib` (FFI consumers) and `rlib` (Rust dependents).

## Architecture

### Core Data Flow

```
ChannelBuilder → Producer/Consumer → SharedMemoryAllocator → ChannelPartition → RingBuffer → Slot[]
```

**`src/MPMC/`** — High-level API:
- `builder.rs`: `ChannelBuilder` fluent API. `build_producer()` auto-attaches or creates SHM; `build_consumer()` requires SHM to already exist (returns `NotFound` otherwise).
- `producer.rs`: `Producer::send()` / `send_batch()` — CAS on tail cursor → write slot → set `sequence = tail + 1`. Producer holds a `sequence_counter` for message IDs.
- `consumer.rs`: `Consumer::receive()` — spins until `sequence == head + 1` → reads slot → increments head. Also has `receive_blocking()` with futex wait.
- `Buffer/layout.rs`: `GlobalHeader` + `ChannelEntry[]` (control plane), `RingBuffer` (data plane). `Slot` = `MessageMeta` (40 bytes, `#[repr(C)]`) + inline payload (1024 bytes) = 1088 bytes, 64-byte aligned.
- `Structs/Buffer_Structs.rs`: `MessageMeta` — ABI-stable across languages. `overflow: u8` flag: 0 = inline payload, 1 = payload holds an `OverflowHandle` pointing into the SFU blob store.

**`src/Core/`** — Low-level shared memory:
- `alloc/mod.rs`: `SharedMemoryAllocator` manages the 128 MB SHM region. Validates magic `0x444D58505F4D454D`. Holds a `PROCESS_SFU: OnceLock<Arc<PinnedBlobStore>>` singleton — first call wins (creator vs attacher). The overflow namespace is `"dmxp_ovf"` → `/dev/shm/dmxp_ovf_ctrl` + `/dev/shm/dmxp_ovf_data_*`.
- `alloc/getters.rs`: Channel enumeration/info.
- `SharedMemory.rs`: OS-level `mmap`/`shm_open` backend (`/dev/shm/dmxp_alloc`).
- `futex.rs`: Futex-based blocking for `Consumer::receive_blocking()`.
- `sfu/mod.rs`: `BlobStoreBuilder` wrapping the `sfb` crate. Supports `.with_shared_mode()` (creator) and `.with_shared_attach()` (attacher) for cross-process overflow.

**`src/ffi.rs`** — C FFI: `dmxp_producer_*` / `dmxp_consumer_*` functions. Error codes: `0` = success, negative = error (see constants at top of file). `FFIMessageMeta` is `#[repr(C)]` and mirrors `MessageMeta`.

### Shared Memory Layout

```
[GlobalHeader: 98,432 bytes]
  magic: u64
  256x ChannelEntry (384 bytes each, 128-byte aligned)
    head: CachePadded<AtomicU64>
    tail: CachePadded<AtomicU64>
    signal: AtomicU32  (futex word)
[Channel band 0: capacity × 1088 bytes]
[Channel band 1: ...]
...
```

Slot address: `band_offset + (cursor % capacity) * 1088`.

### Overflow Path (SFU)

When payload > 1024 bytes, the producer stores the payload in the process-level `PinnedBlobStore` (SFU) and writes an `OverflowHandle` (blob key) into the slot's inline payload area, setting `meta.overflow = 1`. The consumer detects this flag and resolves the handle through the same shared SFU namespace. Both processes must attach to the same `"dmxp_ovf"` namespace — creator initializes, attacher connects.

### Synchronization

Lock-free ring buffer via atomic CAS. Sequence numbers start at 1; a slot is readable when `slot.sequence == head + 1`. `CachePadded` on `head`/`tail` prevents false sharing between producer and consumer cache lines.

### Key Constants

| Constant | Value |
|---|---|
| `MSG_INLINE` | 1024 bytes |
| `Slot` size | 1088 bytes |
| `ChannelEntry` size | 384 bytes |
| Max channels | 256 |
| Default SHM size | 128 MB |
| SFU chunk size | 32 MB |

## Test Infrastructure

- All tests use `#[serial]` from `serial_test` — never run tests in parallel (SHM conflicts).
- `tests/mpmc.rs` — core ring buffer correctness.
- `tests/mpmc_integration_test.rs` — cross-process producer/consumer.
- `tests/cross_process_overflow_test.rs` / `simple_overflow_test.rs` — SFU overflow paths.
- `tests/allocation_test.rs` / `allocator_test.rs` — allocator correctness; `dhat` available for heap profiling.
- `tests_py/` — Python FFI tests.
- After tests, stale `/dev/shm/dmxp_*` and `/dev/shm/dmxp_ovf_*` files may linger if a test crashes; clean manually with `rm /dev/shm/dmxp_*`.

## Workspace Layout

Two members: root crate (`DMXP-MPMC`, the library) and `tui/` (ratatui dashboard reading live channel stats via `dmxp_mpmc`). The `tui/` crate uses the git version of `sfb` rather than the local path. Detailed docs: `docs/ARCHITECTURE.md`, `docs/MEMORY_LAYOUT.md`, `docs/BUILDING_CONSUMERS.md`.
