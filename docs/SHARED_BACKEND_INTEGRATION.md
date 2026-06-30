# Shared Backend Integration Summary

## Problem
The DMXP-MPMC overflow mechanism (SFU - Stable Fragmented Buffer) was using process-local heap memory, which prevented cross-process overflow messages from working. When a producer in one process sent a large message (>1024 bytes), it would store the overflow data in its own heap and write a handle to the ring buffer. A consumer in another process couldn't access this heap memory, resulting in failed overflow reads.

## Solution
Integrated the new `SharedBackend` from the `stable-fragmented-buffer` crate, which stores overflow data in POSIX shared memory (`/dev/shm` on Linux, `shm_open` on macOS) instead of process heap memory.

## Changes Made

### 1. Core Allocator ([src/Core/alloc/mod.rs](src/Core/alloc/mod.rs))

**Added cleanup functionality:**
- Added `CLEANUP_DONE` static flag to ensure cleanup happens only once per process
- Updated `process_sfu_create()` to call `SharedBackend::cleanup_namespace()` on startup
- This cleans up any orphaned SFU files from previous crashes

**Key changes:**
```rust
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

fn process_sfu_create() -> Arc<PinnedBlobStore> {
    PROCESS_SFU
        .get_or_init(|| {
            // Clean up any orphaned files from previous crashes
            if !CLEANUP_DONE.swap(true, Ordering::SeqCst) {
                #[cfg(unix)]
                if let Err(e) = sfb::backend::shared::SharedBackend::cleanup_namespace(SFU_NAMESPACE) {
                    eprintln!("Warning: Failed to cleanup orphaned SFU files: {}", e);
                }
            }

            BlobStoreBuilder::new()
                .with_shared_mode(SFU_NAMESPACE, SFU_CHUNK_SIZE)
                .build()
                .expect("Failed to create process-level shared SFU")
        })
        .clone()
}
```

### 2. Ring Buffer Implementation ([src/MPMC/Buffer/Buffer_impl.rs](src/MPMC/Buffer/Buffer_impl.rs))

**Added acknowledgment for proper cleanup:**
- Modified `dequeue()` method to call `acknowledge_shared()` after successfully resolving an overflow handle
- This allows the SFU to track when overflow data can be freed

**Key change:**
```rust
match self.sfu.resolve(&handle) {
    Some(data) => {
        // Acknowledge the handle for cleanup
        self.sfu.acknowledge_shared(&handle);
        let mut final_meta = meta;
        final_meta.payload_len = data.len() as u32;
        (final_meta, data.to_vec())
    }
    None => {
        eprintln!("Failed to get from SFU: handle expired or invalid");
        (meta, vec![])
    }
}
```

### 3. macOS Shared Memory Improvements ([src/Core/SharedMemory.rs](src/Core/SharedMemory.rs))

**Fixed stale shared memory handling:**
- Updated `MacOSSharedMemory::create()` to properly handle existing shared memory objects
- When `shm_open` with `O_EXCL` fails due to existing object, unlink it and retry
- Fixed the logic to use the correct file descriptor variable after retry

**Key improvements:**
- Changed `fd` to `mut fd` to allow reassignment after unlink
- Proper unlink and retry logic when shared memory already exists
- Removed debug print statements for cleaner production code

## Architecture

### Before (Heap-based)
```
Process A (producer)              Process B (consumer)
────────────────────              ────────────────────
Heap:                             Heap:
┌──────────────┐                  ┌──────────────┐
│ SFU          │                  │ SFU          │
│  [950 bytes] │                  │  (empty)     │
└──────────────┘                  └──────────────┘
      │                                  │
      ▼                                  ▼
Ring Buffer Slot: handle H        Ring Buffer Slot: handle H
                                  resolve(H) → None ❌
```

### After (Shared Memory-based)
```
Process A (producer)              /dev/shm/dmxp_ovf_*         Process B (consumer)
────────────────────              ───────────────────         ────────────────────
                                  ┌──────────────┐
append_shared() ──────────────►  │ Chunk 0      │ ◄────────── resolve(handle)
  returns handle H                │  [950 bytes] │               returns data ✓
                                  └──────────────┘
      │                                                             │
      ▼                                                             ▼
Ring Buffer Slot: handle H (24 bytes)                    acknowledge_shared(handle)
```

## How It Works

1. **Overflow Detection**: When a message exceeds 921 bytes (90% of 1024), it's marked for overflow
2. **Shared Storage**: Producer calls `sfu.append_shared(data)` which stores data in `/dev/shm/dmxp_ovf_data_*`
3. **Handle Transmission**: A 24-byte `OverflowHandle` is written to the ring buffer slot
4. **Cross-Process Access**: Consumer calls `sfu.resolve(&handle)` to read data from shared memory
5. **Cleanup**: Consumer calls `acknowledge_shared(&handle)` to mark data for cleanup
6. **Background Cleanup**: Background thread frees chunks when all handles are acknowledged or TTL expires

## Benefits

1. **Zero-Copy Cross-Process**: Overflow data is accessible from any process without IPC
2. **Automatic Cleanup**: Dual cleanup mechanism (ack-based + TTL-based) prevents memory leaks
3. **Crash Recovery**: Orphaned files are cleaned up on startup
4. **Backpressure**: Optional `max_chunks` limit prevents `/dev/shm` exhaustion
5. **Performance**: Lock-free appends via CAS, proactive prefetching

## Configuration

### SFU Settings (in [src/Core/alloc/mod.rs](src/Core/alloc/mod.rs))
- **Namespace**: `dmxp_ovf` → creates `/dev/shm/dmxp_ovf_ctrl` and `/dev/shm/dmxp_ovf_data_*`
- **Chunk Size**: 32 MB per chunk
- **TTL**: 30 seconds (default from `Config::default()`)
- **Cleanup Interval**: 100ms background sweep

## Testing

The existing MPMC tests pass successfully:
```bash
cargo test --test mpmc
```

Output:
```
test mpmc_correctness_many_threads ... ok
test mpmc_throughput_print ... ok
```

## Files Modified

1. [src/Core/alloc/mod.rs](src/Core/alloc/mod.rs) - Added cleanup and shared backend initialization
2. [src/MPMC/Buffer/Buffer_impl.rs](src/MPMC/Buffer/Buffer_impl.rs) - Added acknowledge_shared calls
3. [src/Core/SharedMemory.rs](src/Core/SharedMemory.rs) - Fixed macOS shared memory handling

## Files Already Supporting Shared Backend

1. [src/Core/sfu/mod.rs](src/Core/sfu/mod.rs) - BlobStoreBuilder with `with_shared_mode()` and `with_shared_attach()`
2. Ring buffer already using `append_shared()` and `resolve()` methods

## Notes

- The shared backend is Unix-only (`#[cfg(unix)]`)
- On macOS, POSIX shared memory is used via `shm_open`/`shm_unlink`
- On Linux, both `/dev/shm` files and `memfd_create` are supported
- The `OverflowHandle` is 24 bytes and ABI-stable (`#[repr(C)]`)

## Verification

To verify the implementation works:

1. **Same-process**: Messages >1024 bytes should work in existing tests ✓
2. **Cross-process**: Producer and consumer in separate processes can exchange large messages
3. **Cleanup**: Orphaned shared memory files are cleaned up on next startup
4. **No leaks**: Background cleanup removes old overflow data

## Deployment

### Starting a New System

1. First process creates shared memory: `SharedMemoryAllocator::new()`
2. Subsequent processes attach: `SharedMemoryAllocator::attach()`
3. SFU cleanup runs automatically on first access
4. Background cleanup runs every 100ms

### Monitoring SFU Files

```bash
# Linux
ls -lh /dev/shm/ | grep dmxp_ovf

# macOS (POSIX shm is under /private/tmp/com.apple.launchd.*)
ls -lh /tmp | grep dmxp_ovf
```

Expected files while overflow is active:
- `/dev/shm/dmxp_ovf_ctrl` — Control file (128 bytes)
- `/dev/shm/dmxp_ovf_data_N` — Data chunks (32 MB each)

### Manual Cleanup

```bash
# Use the provided utility (repo root)
./cleanup_shm

# Or manually (Linux)
rm -f /dev/shm/dmxp_ovf_*
```

## Future Improvements

1. Add metrics/telemetry for SFU usage (chunk count, bytes stored, etc.)
2. Make TTL and cleanup interval configurable
3. Add explicit cross-process overflow tests to `tests/mpmc.rs`
4. Add SFU statistics to TUI monitoring dashboard
