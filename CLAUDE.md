# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

DMXP-MPMC is an ultra-low latency, lock-free, multi-producer multi-consumer shared memory message queue. It uses `/dev/shm/dmxp_alloc` for zero-copy IPC and exposes a C FFI for cross-language bindings (Python, C, Go, Node.js).

## Build & Test Commands

```bash
# Build
cargo build --release

# Run all tests (many tests use serial_test to avoid SHM conflicts)
cargo test

# Run a specific test by name
cargo test mpmc_correctness_many_threads

# Run a specific test file
cargo test --test mpmc

# Run tests with output
cargo test --test mpmc -- --nocapture

# Run examples (producer/consumer in separate terminals)
cargo run --example producer -- 4 1000       # 4 channels, 1000 msgs each
cargo run --example consumer -- 4 1000

# Build the TUI monitoring tool
cd tui && cargo run --bin tui
```

The crate builds as both `cdylib` (for FFI consumers) and `rlib` (for Rust dependents).

## Architecture

### Core Data Flow

```
ChannelBuilder → Producer/Consumer → SharedMemoryAllocator → ChannelPartition → RingBuffer → Slot[]
```

**`src/MPMC/`** — High-level API:
- `builder.rs`: `ChannelBuilder` fluent API for creating producers/consumers
- `producer.rs`: `Producer::send()` and `Producer::send_batch()` — atomic CAS on tail cursor → write slot → set sequence
- `consumer.rs`: `Consumer::receive()` — check head < tail → verify sequence → read → advance head
- `Buffer/`: `RingBuffer` and `Slot` (1088 bytes each, 64-byte aligned). `Slot` holds `MessageMeta` (40 bytes) + inline payload (1024 bytes)

**`src/Core/`** — Low-level shared memory management:
- `alloc/mod.rs`: `SharedMemoryAllocator` manages the 128MB SHM region, channel creation/lookup, and validates magic number `0x444D58505F4D454D`
- `alloc/getters.rs`: Channel info/enumeration methods
- `SharedMemory.rs`: OS-level SHM backend (`/dev/shm`)
- `futex.rs`: Futex-based blocking for consumers
- `sfu/`: `BlobStoreBuilder` for SFB (Stable Fragmented Buffer) overflow when messages exceed `MSG_INLINE` (1024 bytes)

**`src/ffi.rs`** — C FFI: `dmxp_producer_*` / `dmxp_consumer_*` functions with `#[repr(C)]` types. Error codes: 0 = success, negative = error.

### Shared Memory Layout

`GlobalHeader` (98,432 bytes at offset 0) contains up to 256 `ChannelEntry` structs (384 bytes each, 128-byte aligned). Each `ChannelEntry` has `CachePadded<AtomicU64>` head/tail cursors to prevent false sharing. Ring buffer bands follow the header, with slots at `band_offset + (cursor % capacity) * 1088`.

### Synchronization

Lock-free ring buffer using atomic CAS. Producers claim slots by incrementing `tail`, write data, then set slot `sequence = tail + 1`. Consumers verify `sequence == head + 1` before reading. Sequence numbers start at 1.

### Key Constants

- `MSG_INLINE`: 1024 bytes (max inline payload per slot)
- `Slot` size: 1088 bytes
- `ChannelEntry` size: 384 bytes
- Max channels: 256
- Default SHM size: 128MB

## Test Infrastructure

- Tests use `serial_test::serial` to prevent parallel SHM conflicts
- `dhat` is available for heap profiling in allocation tests
- Integration tests in `tests/mpmc_integration_test.rs` test cross-process scenarios
- Python tests in `tests_py/`

## Workspace Layout

The workspace has two members: the root crate (`DMXP-MPMC`) and `tui/` (a ratatui-based monitoring dashboard). Detailed documentation lives in `docs/` (ARCHITECTURE.md, MEMORY_LAYOUT.md, BUILDING_CONSUMERS.md).
