# TUI Demo Guide — Real-Time SFU Shared Memory Monitoring

## Overview

The `tui_demo` example demonstrates DMXP-MPMC's overflow handling with **large messages** (5 MB, 20 MB, 50 MB) that stress-test the **SFU (Stable Fragmented Buffer)** shared memory backend. It runs continuous traffic generation in one terminal while a separate TUI (Terminal User Interface) monitors real-time SFU statistics.

## What You'll See

### Terminal Output (tui_demo)
The traffic generator prints detailed metrics for each large message:

```
[Ch1] Sent message #1 (5.0 MB) in 2.3ms | Runtime: 500ms
[Ch1] Consumed message #1 (5242880 bytes)

[Ch2] Sent message #1 (20.0 MB) in 8.1ms | Runtime: 2s
[Ch2] Consumed message #1 (20971520 bytes)

[Ch3] Sent message #1 (50.0 MB) in 19.5ms | Runtime: 5s
[Ch3] Consumed message #1 (52428800 bytes)

━━━━━━━━━━━━━━━━ Status Report ━━━━━━━━━━━━━━━━
Runtime: 10.2s
Ch0 (100 B): 1000 messages sent
Ch1 (5.0 MB): 20 messages sent
Ch2 (20.0 MB): 5 messages sent
Ch3 (50.0 MB): 2 messages sent
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### TUI Display (dmxp-tui)
The TUI shows **real-time SFU shared memory statistics** in a dedicated **SFB (Stable Fragmented Buffer)** panel:

```
┌─ SFB (Shared Fragmented Buffer) ─────────────────────┐
│ Status:        Active ✓                              │
│ Active Pages:  12                                    │
│ Capacity:      384.0 MB (32.0 MB × 12 chunks)        │
│ Used:          287.3 MB (74.9%)                      │
│ Free:          96.7 MB                               │
│ Fragmentation: 12.3%                                 │
│ Appends/sec:   145                                   │
│                                                      │
│ Capacity ████████████████░░░░░░ 74.9%               │
└──────────────────────────────────────────────────────┘
```

**What Each Metric Shows:**

| Metric | Description |
|--------|-------------|
| **Active Pages** | Number of 32 MB chunks currently allocated in `/dev/shm` |
| **Capacity** | Total shared memory allocated (pages × 32 MB) |
| **Used** | Bytes actively storing overflow data |
| **Free** | Available space in allocated chunks |
| **Fragmentation** | Percentage of space lost to internal fragmentation |
| **Appends/sec** | Rate of new overflow messages being written |

**Color Coding:**
- 🟢 **Green** (< 60%): Healthy headroom
- 🟡 **Yellow** (60-80%): Moderate usage
- 🔴 **Red** (> 80%): High usage, new chunks being allocated

## Running the Demo

### Step 1: Start the Traffic Generator

In **Terminal 1**, run:

```bash
cd /Users/neeraj/Dev/DMXP/DMXP-MPMC
cargo run --example tui_demo
```

Expected output:
```
╔════════════════════════════════════════════════════════════════╗
║        TUI Demo with LARGE Overflow Messages                   ║
╚════════════════════════════════════════════════════════════════╝

This demo generates large messages to stress-test the SFU shared backend.
Open another terminal and run: cargo run -p dmxp-tui

Channel configuration:
  Ch 0: 100 B messages (inline, no overflow)
  Ch 1: 5.0 MB messages (SFU overflow)
  Ch 2: 20.0 MB messages (heavy SFU)
  Ch 3: 50.0 MB messages (maximum SFU stress)

✓ All channels ready. Starting traffic generation...
```

The demo will continuously:
- Send **100-byte messages** on Ch0 every 10ms (no overflow, baseline traffic)
- Send **5 MB messages** on Ch1 every 500ms → triggers SFU allocation
- Send **20 MB messages** on Ch2 every 2 seconds → heavy SFU usage
- Send **50 MB messages** on Ch3 every 5 seconds → maximum stress test

### Step 2: Launch the TUI Monitor

In **Terminal 2**, run:

```bash
cd /Users/neeraj/Dev/DMXP/DMXP-MPMC
cargo run -p dmxp-tui
```

The TUI will connect to the shared memory region and display:
- Per-channel ring buffer statistics (head, tail, usage)
- **Real-time SFU metrics** (see panel above)
- Message throughput graphs
- Overall system health

**Navigation:**
- Press `q` to quit
- The display refreshes every 100ms

## What to Observe

### 1. Initial State (0-5 seconds)
- **Terminal 1**: Rapid small messages on Ch0, first large messages on Ch1-3
- **TUI SFB Panel**:
  - Active Pages: **1** (initial chunk)
  - Capacity: **32.0 MB**
  - Used: Rapidly increasing as 5 MB messages arrive
  - Appends/sec: **~2-3** (corresponding to large message rate)

### 2. Growth Phase (5-30 seconds)
- **Terminal 1**: Status reports show increasing message counts
- **TUI SFB Panel**:
  - Active Pages: **2-4** (new chunks allocated as data accumulates)
  - Capacity: **64-128 MB**
  - Used: **40-80%** (yellow/red gauge as buffers fill)
  - Fragmentation: **5-15%** (some space lost to chunk boundaries)

### 3. Steady State (30+ seconds)
- **Terminal 1**: Continuous send/receive loop with consistent timing
- **TUI SFB Panel**:
  - Active Pages: **Stable** (4-8 chunks depending on consumer rate)
  - Used: **Oscillating** (spikes on send, drops on acknowledge)
  - Appends/sec: **Steady** (~145 based on message schedule)

### 4. Cleanup Observation
The SFU background cleanup runs every 100ms. Watch for:
- **Used data dropping** after consumers acknowledge messages
- **Active Pages decreasing** as fully-acked chunks are freed
- **Free space reclaimed** back to `/dev/shm`

## Understanding the Overflow Flow

```
Producer (Ch1, 5 MB message)
  │
  ├─ Message size check: 5 MB > 921 bytes → OVERFLOW
  │
  ├─ SFU append_shared(&data[5MB])
  │    └─ Returns OverflowHandle (24 bytes):
  │        { page_id: 0, offset: 64, size: 5242880, ... }
  │
  ├─ Write OverflowHandle into ring buffer slot
  │    (only 24 bytes, not the 5 MB data)
  │
  └─ Terminal prints: "[Ch1] Sent message #1 (5.0 MB) in 2.3ms"

Consumer (Ch1)
  │
  ├─ Read OverflowHandle from ring buffer slot
  │
  ├─ SFU resolve(&handle)
  │    └─ Reads 5 MB from /dev/shm/dmxp_data_0
  │
  ├─ SFU acknowledge_shared(&handle)
  │    └─ Increments ack counter → enables cleanup
  │
  └─ Terminal prints: "[Ch1] Consumed message #1 (5242880 bytes)"

TUI (every 100ms)
  │
  └─ Queries SFU profiler stats
       └─ Displays: active_pages, capacity, used, appends/sec
```

## SFU Shared Memory Files

While the demo runs, check `/dev/shm`:

```bash
ls -lh /dev/shm/dmxp_*
```

Expected output:
```
-rw------- 1 user user  128 Apr  1 12:34 /dev/shm/dmxp_ctrl       # Control file
-rw------- 1 user user  32M Apr  1 12:34 /dev/shm/dmxp_data_0     # Chunk 0
-rw------- 1 user user  32M Apr  1 12:34 /dev/shm/dmxp_data_1     # Chunk 1
-rw------- 1 user user  32M Apr  1 12:34 /dev/shm/dmxp_data_2     # Chunk 2
...
```

**Notes:**
- Each `dmxp_data_N` file is **32 MB** (default chunk size)
- Files are **automatically cleaned up** when the demo exits
- If the process crashes, orphaned files are removed on next startup

## Performance Expectations

On modern hardware (Apple M-series or recent x86_64):

| Message Size | Send Latency (p50) | Throughput |
|--------------|-------------------|------------|
| 100 B (inline) | < 0.5 µs | 2M msgs/sec |
| 5 MB (overflow) | 2-3 ms | ~400 msgs/sec |
| 20 MB (overflow) | 8-12 ms | ~100 msgs/sec |
| 50 MB (overflow) | 20-30 ms | ~40 msgs/sec |

**Latency breakdown for overflow messages:**
- Ring buffer write: **< 1 µs** (only writes 24-byte handle)
- SFU append (memcpy to /dev/shm): **~0.4 µs per KB** (3-20 ms for 5-50 MB)
- Consumer resolve (memcpy from /dev/shm): **~0.4 µs per KB**

## Troubleshooting

### "Failed to get from SFU: handle expired or invalid"
- **Cause**: Consumer is too slow, messages expired (30s TTL by default)
- **Fix**: Increase `default_ttl_ms` in `BlobStoreBuilder` or speed up consumers

### TUI shows "SFB: No data available"
- **Cause**: TUI started before traffic generator
- **Fix**: Ensure `tui_demo` runs first (it creates the shared memory region)

### Active Pages growing unbounded
- **Cause**: Consumers not calling `acknowledge_shared()`
- **Fix**: Check `Buffer_impl.rs:dequeue()` includes `self.sfu.acknowledge_shared(&handle)`

### Segmentation fault on macOS
- **Cause**: Stale `/dev/shm/dmxp_*` files from previous runs
- **Fix**: `rm /dev/shm/dmxp_*` before starting

## Stopping the Demo

1. In the TUI terminal (Terminal 2), press **`q`** to exit
2. In the traffic generator terminal (Terminal 1), press **`Ctrl+C`**
3. Shared memory files will be automatically cleaned up

## Next Steps

- **Scale up**: Modify `tui_demo.rs` to add more channels or increase message rates
- **Tune SFU**: Adjust `Config` parameters in `src/Core/alloc/mod.rs`:
  - `page_size`: Heap page size (not used in shared mode)
  - `decay_timeout_ms`: Grace period before freeing acked chunks (default: 5s)
  - `default_ttl_ms`: Auto-expire data after this time (default: 30s)
- **Profile**: Use `dhat` to analyze memory usage patterns
- **Cross-process**: Run producer and consumer in separate processes (see [examples/cross_process_test.rs](../examples/cross_process_test.rs))

## Testing

### Unit Test — Basic Overflow

```bash
cargo test mpmc_overflow_with_shared_backend -- --nocapture
```

Expected output:
```
✓ Successfully sent and received 10 overflow messages (2048 bytes each) via shared backend
test mpmc_overflow_with_shared_backend ... ok
```

Tests 2 KB messages (2× inline limit) in a single process. Verifies `append_shared` → `resolve` → `acknowledge_shared` lifecycle.

### Cross-Process Test

```bash
# Terminal 1
cargo run --example overflow_producer

# Terminal 2
cargo run --example overflow_consumer
```

Verifies that `/dev/shm/dmxp_ovf_*` files are truly accessible across separate OS processes.

## Limitations

| Constraint | Value | Notes |
|---|---|---|
| Max single message | 32 MB − 64 B | Overflow doesn't span chunks; limited by `SFU_CHUNK_SIZE` |
| Max total overflow | ~50% of RAM | `/dev/shm` is tmpfs on Linux |
| TTL | 30 seconds | Data auto-expires for crash recovery even if not acked |
| Max chunks | 2³² | Or `max_chunks` config limit |

To send messages **> 32 MB**, increase `SFU_CHUNK_SIZE` in `src/Core/alloc/mod.rs`:

```rust
const SFU_CHUNK_SIZE: usize = 64 * 1024 * 1024;  // 64 MB chunks
```

## Architecture Reference

For deeper understanding, see:
- [SFU_PROBLEM.md](SFU_PROBLEM.md) — Explains why shared backend is necessary
- [SHARED_BACKEND_INTEGRATION.md](SHARED_BACKEND_INTEGRATION.md) — Implementation details and deployment guide
- [MEMORY_LAYOUT.md](MEMORY_LAYOUT.md) — Shared memory structure
- [ARCHITECTURE.md](ARCHITECTURE.md) — Overall system design
