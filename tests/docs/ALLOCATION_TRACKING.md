# Memory Allocation Tracking Guide

This document describes tools and methods to track memory allocations in DMXP-KVCache.

## 1. dhat-rs (Recommended for Heap Profiling)

**Best for:** Detailed heap allocation analysis with call stacks

### Setup:
```bash
# Already added to Cargo.toml
```

### Usage:
```bash
# Run with dhat feature enabled
cargo test --test allocation_test --features dhat -- --nocapture

# Or run specific test
cargo test --test allocation_test track_allocations_with_dhat --features dhat
```

**Output:** Detailed heap profile showing:
- Peak heap size
- Allocation counts
- Call stacks for allocations
- Memory leaks

## 2. memory-stats Crate

**Best for:** Quick memory usage checks

### Usage:
```bash
cargo test --test allocation_test track_allocations_with_memory_stats -- --nocapture
```

**Output:** Physical memory usage before/after operations

## 3. Windows-Specific Tools

### A. Windows Performance Analyzer (WPA)
1. Download Windows Performance Toolkit (WPT) from Microsoft
2. Record with `wpr.exe` or use Windows Performance Recorder
3. Analyze heap allocations in WPA

### B. VMMap (Sysinternals)
- Download from Microsoft Sysinternals
- Attach to running process
- View memory breakdown by type (heap, stack, etc.)

### C. Application Verifier (AppVerif)
- Built into Windows SDK
- Enable heap tracing
- View allocations in debugger

## 4. Custom Allocation Tracking

For shared memory specifically (mmap), you can track:

```rust
// Track shared memory size
let shared_mem_size = capacity * RingBuffer::slot_stride();
println!("Shared memory allocated: {} bytes", shared_mem_size);
```

## 5. Valgrind (Linux/WSL only)

If running on WSL:
```bash
valgrind --leak-check=full --show-leak-kinds=all \
  cargo test --test mpmc
```

## Quick Reference

| Tool | Platform | Best For |
|------|----------|----------|
| dhat-rs | All | Heap profiling with call stacks |
| memory-stats | All | Simple memory usage |
| WPA | Windows | System-wide profiling |
| VMMap | Windows | Process memory breakdown |
| Valgrind | Linux/WSL | Leak detection |

## Example: Track RingBuffer Allocations

```bash
# With dhat (detailed)
cargo test --test allocation_test track_allocations_with_dhat --features dhat -- --nocapture

# With memory-stats (quick)
cargo test --test allocation_test track_allocations_with_memory_stats -- --nocapture

# Track MPMC test allocations
cargo test --test mpmc --features dhat -- --nocapture
```

