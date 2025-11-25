# DMXP-MPMC Documentation

Complete documentation for the DMXP-MPMC (Distributed Multi-Producer Multi-Consumer) shared memory messaging system.

## Quick Start

### Running the Examples

```bash
# Terminal 1: Start Rust producer
cargo run --example producer 4 1000

# Terminal 2: Start Python consumer
python3 tests_py/consumer.py 4 1000
```

### Creating Your First Consumer (Python)

```python
from tests_py.consumer import PythonConsumer

consumer = PythonConsumer()
consumer.attach()

# Receive a message from channel 0
message = consumer.receive(0)
if message:
    print(f"Received: {message.payload.decode('utf-8')}")

consumer.close()
```

### Creating Your First Producer (Rust)

```rust
use dmxp_kvcache::MPMC::ChannelBuilder;

fn main() -> std::io::Result<()> {
    let producer = ChannelBuilder::new()
        .with_channel_id(0)
        .build_producer()?;

    producer.send("Hello, World!")?;
    Ok(())
}
```

## Documentation Index

### Core Documentation

1. **[ARCHITECTURE.md](./ARCHITECTURE.md)** - System architecture, design decisions, and performance characteristics

   - Overview and key features
   - Architecture diagrams
   - Data flow and synchronization
   - Performance metrics

2. **[MEMORY_LAYOUT.md](./MEMORY_LAYOUT.md)** - Exact byte-level memory layout specifications

   - Structure definitions
   - Field offsets and sizes
   - Alignment requirements
   - Validation checklist

3. **[BUILDING_CONSUMERS.md](./BUILDING_CONSUMERS.md)** - Multi-language implementation guide
   - Rust examples
   - Python examples
   - C/C++ examples
   - Go examples
   - General guidelines and best practices

## Key Concepts

### Shared Memory

- **Location**: `/dev/shm/dmxp_alloc`
- **Size**: 128MB (default, configurable)
- **Persistence**: Survives process restarts
- **Access**: Multiple processes can read/write simultaneously

### Channels

- **Count**: Up to 256 independent channels
- **Capacity**: 1024 slots per channel (default)
- **Isolation**: Each channel has its own ring buffer
- **Reuse**: Channels persist and can be reused

### Messages

- **Size**: Up to 960 bytes inline
- **Format**: Binary data (application-defined)
- **Metadata**: Timestamp, sender info, message ID
- **Ordering**: FIFO within each channel

## Memory Structure

```
/dev/shm/dmxp_alloc
├── GlobalHeader (98,432 bytes)
│   ├── Magic: 0x444D58505F4D454D
│   ├── Version: 1
│   ├── Channel Count: 4
│   └── ChannelEntry[256]
│       ├── [0] Channel 0 metadata
│       ├── [1] Channel 1 metadata
│       ├── [2] Channel 2 metadata
│       └── [3] Channel 3 metadata
│
├── RingBuffer 0 (1024 slots × 1088 bytes)
│   ├── Slot[0]: sequence, metadata, payload
│   ├── Slot[1]: sequence, metadata, payload
│   └── ...
│
├── RingBuffer 1 (1024 slots × 1088 bytes)
├── RingBuffer 2 (1024 slots × 1088 bytes)
└── RingBuffer 3 (1024 slots × 1088 bytes)
```

## Performance

### Throughput

| Language        | Producer → Consumer | Messages/Second |
| --------------- | ------------------- | --------------- |
| Rust → Rust     | Same process        | 2-3 million     |
| Rust → Rust     | Different processes | 2-3 million     |
| Rust → Python   | Different processes | 300-400K        |
| Python → Python | Different processes | 100-200K        |

### Latency

- **Write**: 100-500 nanoseconds
- **Read**: 200-800 nanoseconds
- **Round-trip**: 300-1300 nanoseconds

## API Reference

### Rust

```rust
// Create producer
let producer = ChannelBuilder::new()
    .with_channel_id(0)
    .with_capacity(1024)
    .build_producer()?;

// Send message
producer.send("data")?;

// Create consumer
let consumer = ChannelBuilder::new()
    .with_channel_id(0)
    .build_consumer()?;

// Receive message
if let Some(data) = consumer.receive()? {
    // Process data
}
```

### Python

```python
# Create consumer
consumer = PythonConsumer()
consumer.attach()

# Get channel info
info = consumer.get_channel_info(0)
print(f"Capacity: {info['capacity']}")

# Receive message
message = consumer.receive(0)
if message:
    print(message.payload.decode('utf-8'))

consumer.close()
```

## Common Use Cases

### 1. High-Frequency Data Streaming

```rust
// Producer: Market data feed
loop {
    let market_data = fetch_market_data();
    producer.send(&market_data)?;
}

// Consumer: Trading algorithm
while let Some(data) = consumer.receive()? {
    process_market_data(data);
}
```

### 2. Multi-Language Pipeline

```
Rust Producer → Python ML Model → Rust Decision Engine
     ↓                ↓                    ↓
  Channel 0       Channel 1           Channel 2
```

### 3. Fan-Out Pattern

```
Single Producer → Multiple Consumers (same channel)
                  ├── Consumer 1 (logging)
                  ├── Consumer 2 (analytics)
                  └── Consumer 3 (alerting)
```

### 4. Fan-In Pattern

```
Multiple Producers → Single Consumer
├── Producer 1 (sensor A) → Channel 0 ┐
├── Producer 2 (sensor B) → Channel 1 ├→ Consumer (aggregator)
└── Producer 3 (sensor C) → Channel 2 ┘
```

## Troubleshooting

### Issue: "Shared memory not found"

```bash
# Check if shared memory exists
ls -lh /dev/shm/dmxp_alloc

# If not, run producer first to create it
cargo run --example producer 4 100 --auto-exit
```

### Issue: "Channel not found"

```bash
# Check active channels
python3 -c "
from tests_py.consumer import PythonConsumer
c = PythonConsumer()
c.attach()
print(c.list_channels())
"
```

### Issue: "No messages received"

- Ensure producer ran **before** consumer
- Check that both use the same channel ID
- Verify messages haven't been consumed already

### Issue: "Invalid magic number"

- Shared memory was created by incompatible version
- Delete and recreate: `rm -f /dev/shm/dmxp_alloc`

## Best Practices

### 1. Channel Management

✅ **Do**: Reuse channels across runs  
✅ **Do**: Use different channels for different data types  
❌ **Don't**: Create channels dynamically in hot path  
❌ **Don't**: Exceed 256 channels

### 2. Message Design

✅ **Do**: Keep messages under 960 bytes  
✅ **Do**: Use binary formats (protobuf, flatbuffers)  
❌ **Don't**: Send large payloads (use references instead)  
❌ **Don't**: Assume message order across channels

### 3. Error Handling

✅ **Do**: Check for buffer full/empty conditions  
✅ **Do**: Validate magic number on attach  
✅ **Do**: Handle sequence mismatches gracefully  
❌ **Don't**: Assume infinite capacity  
❌ **Don't**: Ignore error returns

### 4. Performance

✅ **Do**: Batch message processing  
✅ **Do**: Pin threads to CPUs  
✅ **Do**: Use CPU affinity for producers/consumers  
❌ **Don't**: Call `attach()` repeatedly  
❌ **Don't**: Re-read channel metadata every message

## Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_mpmc_integration
```

### Integration Tests

```bash
# Test Rust → Rust
cargo run --example producer 4 1000 --auto-exit && \
cargo run --example consumer 4 1000

# Test Rust → Python
cargo run --example producer 4 1000 --auto-exit && \
python3 tests_py/consumer.py 4 1000
```

### Benchmarks

```bash
# Measure throughput
cargo run --example producer 4 10000 --auto-exit
# Note the TPS in output

# Measure latency
cargo run --example producer 1 1000 --auto-exit
# Check "Time taken" in statistics
```

## Advanced Topics

### Custom Allocators

See `src/Core/alloc/mod.rs` for implementing custom memory allocators.

### Zero-Copy Deserialization

Use `flatbuffers` or `capnproto` for zero-copy message parsing.

### Multi-Region Setup

Create multiple shared memory regions for isolation:

```rust
let allocator1 = SharedMemoryAllocator::new_named("region1", size)?;
let allocator2 = SharedMemoryAllocator::new_named("region2", size)?;
```

## Contributing

When adding new features:

1. Update memory layout documentation
2. Add tests for new functionality
3. Update this README with examples
4. Ensure cross-language compatibility

## License

See LICENSE file in repository root.

## Support

- **Issues**: GitHub Issues
- **Documentation**: This `docs/` directory
- **Examples**: `examples/` and `tests_py/` directories

## Version History

- **v0.1.0** - Initial release
  - Basic MPMC functionality
  - Rust and Python support
  - Lock-free ring buffers
  - Cross-language compatibility
