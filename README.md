# DMXP-MPMC (Dual-Mode Exchange Protocol - Multi-Producer Multi-Consumer)

**Ultra-low latency, cross-language, shared-memory message queue.**

DMXP-MPMC provides a unified ordered keyspace across all channels for fast appends and sequential scans. It combines **Rust's safety and performance** with a robust **FFI layer**, enabling seamless, zero-copy communication between **Rust, Python, Go, C, C++, and Node.js**.

---

## 🚀 Maximum Use Cases

This project shines where **latency** and **throughput** are critical, and **cross-language interoperability** is required.

### 1. High-Frequency Trading (HFT) & FinTech

- **Scenario**: A C++ Market Data Feed Handler ingests millions of ticks per second. A Python Strategy Engine needs to react in microseconds.
- **Why DMXP**: Standard TCP/IPC (sockets) adds interaction/serialization overhead. DMXP uses shared memory (SHM) and lock-free ring buffers (atomic CAS) to pass messages in **nanoseconds**.
- **Benefit**: Fastest possible data hand-off between components.

### 2. AI & ML Pipelines (Vision/LLM)

- **Scenario**: A Rust-based video decoder/pre-processor feeds frames to a Python/PyTorch inference engine.
- **Why DMXP**: Passing 4K video frames over localhost sockets burns CPU. DMXP allows passing pointers/data via shared memory.
- **Benefit**: Zero-copy data transfer. Python can read data written by Rust without copying.

### 3. Robotics & Autonomous Systems (ROS Alternative)

- **Scenario**: LIDAR and Camera sensors (C++) need to be fused and processed by a Planning module (Python/C++).
- **Why DMXP**: Unlike ROS1 (TCP/XML-RPC) or ROS2 (DDS), DMXP offers a lightweight, bare-metal shared memory transport with no serialization bloat.
- **Benefit**: Real-time deterministic performance.

### 4. Modular Microservices on Localhost

- **Scenario**: Breaking a monolith into a Go backend, a Rust calculation engine, and a Python data analyzer running on the same server.
- **Why DMXP**: Avoids the "Microservice Tax" of HTTP/gRPC loopback calls.
- **Benefit**: Monolith-like performance with Microservice architecture.

---

## ⚡ Key Features

- **Zero-Copy (ish)**: Uses Ring Buffers in `/dev/shm`.
- **Lock-Free**: Uses Atomic CAS (Compare-And-Swap) for extremely high throughput (~2.5M+ msgs/sec).
- **Cross-Language FFI**: First-class support for Python, C, Go, etc.
- **Persistence**: Channels survive process restarts (until SHM is cleared).
- **Batching**: Supports Atomic Batch Commits for bulk operations.

## 🛠️ Quick Start

### Python Example

```python
import dmxp
# Producer
prod = dmxp.create_producer(channel_id=100)
prod.send(b"Hello from Python")

# Consumer
cons = dmxp.create_consumer(channel_id=100)
msg = cons.receive()
```

### Rust Example

```rust
let recv = Consumer::new(100)?;
let msg = recv.receive()?;
```
