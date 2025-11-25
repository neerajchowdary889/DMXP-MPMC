# Building Consumers and Producers

This guide shows how to build DMXP-MPMC consumers and producers in different programming languages.

## Table of Contents

- [Rust](#rust)
- [Python](#python)
- [C/C++](#cc)
- [Go](#go)
- [General Guidelines](#general-guidelines)

---

## Rust

### Using the Built-in API

The easiest way is to use the provided `ChannelBuilder`:

```rust
use dmxp_kvcache::MPMC::ChannelBuilder;

// Producer
fn main() -> std::io::Result<()> {
    let producer = ChannelBuilder::new()
        .with_channel_id(0)
        .with_capacity(1024)  // Optional, default is 1024
        .build_producer()?;

    // Send a message
    producer.send("Hello, World!")?;

    Ok(())
}

// Consumer
fn main() -> std::io::Result<()> {
    let consumer = ChannelBuilder::new()
        .with_channel_id(0)
        .build_consumer()?;

    // Receive a message
    if let Some(data) = consumer.receive()? {
        let message = String::from_utf8(data)?;
        println!("Received: {}", message);
    }

    Ok(())
}
```

### Custom Implementation

For more control, use the low-level API:

```rust
use dmxp_kvcache::Core::alloc::SharedMemoryAllocator;

// Create shared memory
let allocator = SharedMemoryAllocator::new(128 * 1024 * 1024)?;

// Create a channel
let channel = allocator.create_channel(1024)?;

// Get channel info
let channels = allocator.get_channels();
println!("Active channels: {}", channels.len());
```

---

## Python

### Complete Example

```python
import mmap
import os
import ctypes

# Constants
MAGIC_NUMBER = 0x444D58505F4D454D
SLOT_SIZE = 1088
MAX_CHANNELS = 256

class PythonConsumer:
    def __init__(self, shm_path="/dev/shm/dmxp_alloc"):
        self.shm_path = shm_path
        self.mm = None

    def attach(self):
        """Attach to existing shared memory"""
        fd = os.open(self.shm_path, os.O_RDWR)
        size = os.fstat(fd).st_size
        self.mm = mmap.mmap(fd, size, mmap.MAP_SHARED,
                           mmap.PROT_READ | mmap.PROT_WRITE)
        os.close(fd)

        # Validate magic number
        magic = int.from_bytes(self.mm[0:8], 'little')
        if magic != MAGIC_NUMBER:
            raise ValueError(f"Invalid magic: {magic:x}")

    def get_channel_info(self, channel_id):
        """Read channel metadata"""
        offset = 128 + (channel_id * 384)
        self.mm.seek(offset)
        data = self.mm.read(384)

        return {
            'channel_id': int.from_bytes(data[0:4], 'little'),
            'capacity': int.from_bytes(data[8:16], 'little'),
            'band_offset': int.from_bytes(data[16:24], 'little'),
            'tail': int.from_bytes(data[128:136], 'little'),
            'head': int.from_bytes(data[256:264], 'little'),
        }

    def receive(self, channel_id):
        """Receive one message"""
        info = self.get_channel_info(channel_id)

        head = info['head']
        tail = info['tail']

        # Check if empty
        if head == tail:
            return None

        # Calculate slot position
        pos = head % info['capacity']
        slot_offset = info['band_offset'] + (pos * SLOT_SIZE)

        # Read slot
        self.mm.seek(slot_offset)
        slot_data = self.mm.read(SLOT_SIZE)

        # Verify sequence
        sequence = int.from_bytes(slot_data[0:8], 'little')
        if sequence != head + 1:
            return None

        # Read payload
        payload_len = int.from_bytes(slot_data[40:44], 'little')
        payload = slot_data[64:64+payload_len]

        # Increment head
        new_head = head + 1
        head_offset = 128 + (channel_id * 384) + 256
        self.mm.seek(head_offset)
        self.mm.write(new_head.to_bytes(8, 'little'))

        return payload

    def close(self):
        if self.mm:
            self.mm.close()

# Usage
consumer = PythonConsumer()
consumer.attach()

message = consumer.receive(0)
if message:
    print(f"Received: {message.decode('utf-8')}")

consumer.close()
```

### Python Producer

```python
class PythonProducer:
    def __init__(self, shm_path="/dev/shm/dmxp_alloc"):
        self.shm_path = shm_path
        self.mm = None

    def attach(self):
        """Attach to existing shared memory"""
        fd = os.open(self.shm_path, os.O_RDWR)
        size = os.fstat(fd).st_size
        self.mm = mmap.mmap(fd, size, mmap.MAP_SHARED,
                           mmap.PROT_READ | mmap.PROT_WRITE)
        os.close(fd)

    def send(self, channel_id, payload):
        """Send a message"""
        info = self.get_channel_info(channel_id)

        tail = info['tail']
        head = info['head']
        capacity = info['capacity']

        # Check if full
        if tail - head >= capacity:
            raise IOError("Channel full")

        # Calculate slot position
        pos = tail % capacity
        slot_offset = info['band_offset'] + (pos * SLOT_SIZE)

        # Prepare slot data
        slot_data = bytearray(SLOT_SIZE)

        # Write sequence
        sequence = tail + 1
        slot_data[0:8] = sequence.to_bytes(8, 'little')

        # Write MessageMeta
        payload_bytes = payload.encode('utf-8') if isinstance(payload, str) else payload
        payload_len = len(payload_bytes)

        slot_data[40:44] = payload_len.to_bytes(4, 'little')

        # Write payload
        slot_data[64:64+payload_len] = payload_bytes

        # Write to shared memory
        self.mm.seek(slot_offset)
        self.mm.write(slot_data)

        # Increment tail
        new_tail = tail + 1
        tail_offset = 128 + (channel_id * 384) + 128
        self.mm.seek(tail_offset)
        self.mm.write(new_tail.to_bytes(8, 'little'))

    def get_channel_info(self, channel_id):
        """Same as consumer"""
        offset = 128 + (channel_id * 384)
        self.mm.seek(offset)
        data = self.mm.read(384)

        return {
            'channel_id': int.from_bytes(data[0:4], 'little'),
            'capacity': int.from_bytes(data[8:16], 'little'),
            'band_offset': int.from_bytes(data[16:24], 'little'),
            'tail': int.from_bytes(data[128:136], 'little'),
            'head': int.from_bytes(data[256:264], 'little'),
        }

# Usage
producer = PythonProducer()
producer.attach()
producer.send(0, "Hello from Python!")
```

---

## C/C++

### Structures

```c
#include <stdint.h>
#include <stdatomic.h>

#define MAX_CHANNELS 256
#define MSG_INLINE 960
#define SLOT_SIZE 1088

typedef struct {
    uint64_t value;
    uint8_t _pad[56];
} CachePadded;

typedef struct {
    uint64_t message_id;
    uint64_t timestamp_ns;
    uint32_t channel_id;
    uint32_t message_type;
    uint32_t sender_pid;
    uint16_t sender_runtime;
    uint16_t flags;
    uint32_t payload_len;
} __attribute__((packed)) MessageMeta;

typedef struct {
    uint32_t channel_id;
    uint32_t flags;
    uint64_t capacity;
    uint64_t band_offset;
    uint8_t _pad1[104];
    CachePadded tail;
    CachePadded head;
    uint8_t _pad2[64];
} __attribute__((aligned(128))) ChannelEntry;

typedef struct {
    uint64_t magic;
    uint32_t version;
    uint32_t max_channels;
    uint32_t channel_count;
    uint32_t reserved;
    uint8_t _pad[104];
    ChannelEntry channels[MAX_CHANNELS];
} __attribute__((aligned(128))) GlobalHeader;

typedef struct {
    atomic_uint64_t sequence;
    MessageMeta meta;
    uint8_t _pad1[16];
    uint8_t payload[MSG_INLINE];
} __attribute__((aligned(64))) Slot;
```

### Consumer Example

```c
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>

int consume_message(void* shm, uint32_t channel_id, char* buffer, size_t buf_size) {
    GlobalHeader* header = (GlobalHeader*)shm;
    ChannelEntry* channel = &header->channels[channel_id];

    uint64_t head = channel->head.value;
    uint64_t tail = channel->tail.value;

    // Check if empty
    if (head == tail) {
        return 0;  // No messages
    }

    // Calculate slot position
    uint64_t pos = head % channel->capacity;
    Slot* slot = (Slot*)((char*)shm + channel->band_offset + (pos * SLOT_SIZE));

    // Verify sequence
    uint64_t sequence = atomic_load(&slot->sequence);
    if (sequence != head + 1) {
        return -1;  // Slot not ready
    }

    // Copy payload
    size_t len = slot->meta.payload_len;
    if (len > buf_size) len = buf_size;
    memcpy(buffer, slot->payload, len);
    buffer[len] = '\0';

    // Increment head
    channel->head.value = head + 1;

    return len;
}

int main() {
    // Open shared memory
    int fd = shm_open("/dmxp_alloc", O_RDWR, 0666);
    if (fd < 0) {
        perror("shm_open");
        return 1;
    }

    struct stat sb;
    fstat(fd, &sb);

    void* shm = mmap(NULL, sb.st_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (shm == MAP_FAILED) {
        perror("mmap");
        return 1;
    }

    // Receive message
    char buffer[1024];
    int len = consume_message(shm, 0, buffer, sizeof(buffer));

    if (len > 0) {
        printf("Received: %s\n", buffer);
    }

    munmap(shm, sb.st_size);
    close(fd);

    return 0;
}
```

---

## Go

### Structures

```go
package main

import (
    "encoding/binary"
    "fmt"
    "os"
    "syscall"
    "unsafe"
)

const (
    MaxChannels = 256
    SlotSize    = 1088
    MagicNumber = 0x444D58505F4D454D
)

type ChannelInfo struct {
    ChannelID  uint32
    Capacity   uint64
    BandOffset uint64
    Tail       uint64
    Head       uint64
}

type Consumer struct {
    data []byte
}

func NewConsumer(shmPath string) (*Consumer, error) {
    file, err := os.OpenFile(shmPath, os.O_RDWR, 0666)
    if err != nil {
        return nil, err
    }
    defer file.Close()

    stat, err := file.Stat()
    if err != nil {
        return nil, err
    }

    data, err := syscall.Mmap(int(file.Fd()), 0, int(stat.Size()),
        syscall.PROT_READ|syscall.PROT_WRITE, syscall.MAP_SHARED)
    if err != nil {
        return nil, err
    }

    // Verify magic
    magic := binary.LittleEndian.Uint64(data[0:8])
    if magic != MagicNumber {
        return nil, fmt.Errorf("invalid magic: %x", magic)
    }

    return &Consumer{data: data}, nil
}

func (c *Consumer) GetChannelInfo(channelID uint32) *ChannelInfo {
    offset := 128 + (channelID * 384)

    return &ChannelInfo{
        ChannelID:  binary.LittleEndian.Uint32(c.data[offset:offset+4]),
        Capacity:   binary.LittleEndian.Uint64(c.data[offset+8:offset+16]),
        BandOffset: binary.LittleEndian.Uint64(c.data[offset+16:offset+24]),
        Tail:       binary.LittleEndian.Uint64(c.data[offset+128:offset+136]),
        Head:       binary.LittleEndian.Uint64(c.data[offset+256:offset+264]),
    }
}

func (c *Consumer) Receive(channelID uint32) ([]byte, error) {
    info := c.GetChannelInfo(channelID)

    // Check if empty
    if info.Head == info.Tail {
        return nil, nil
    }

    // Calculate slot position
    pos := info.Head % info.Capacity
    slotOffset := info.BandOffset + (pos * SlotSize)

    // Read sequence
    sequence := binary.LittleEndian.Uint64(c.data[slotOffset:slotOffset+8])

    // Verify sequence
    if sequence != info.Head+1 {
        return nil, fmt.Errorf("sequence mismatch")
    }

    // Read payload length
    payloadLen := binary.LittleEndian.Uint32(c.data[slotOffset+40:slotOffset+44])

    // Read payload
    payload := make([]byte, payloadLen)
    copy(payload, c.data[slotOffset+64:slotOffset+64+uint64(payloadLen)])

    // Increment head
    newHead := info.Head + 1
    headOffset := 128 + (channelID * 384) + 256
    binary.LittleEndian.PutUint64(c.data[headOffset:headOffset+8], newHead)

    return payload, nil
}

func (c *Consumer) Close() error {
    return syscall.Munmap(c.data)
}

func main() {
    consumer, err := NewConsumer("/dev/shm/dmxp_alloc")
    if err != nil {
        panic(err)
    }
    defer consumer.Close()

    payload, err := consumer.Receive(0)
    if err != nil {
        panic(err)
    }

    if payload != nil {
        fmt.Printf("Received: %s\n", string(payload))
    }
}
```

---

## General Guidelines

### 1. Shared Memory Access

**Linux:**

```bash
# Shared memory is at /dev/shm/dmxp_alloc
ls -lh /dev/shm/dmxp_alloc
```

**Opening:**

- Use `mmap()` with `MAP_SHARED`
- Open with `O_RDWR` for read/write access
- Size is typically 128MB (default)

### 2. Byte Order

- **Always use little-endian** byte order
- Most platforms (x86, ARM) are little-endian
- Use `to_bytes(..., 'little')` in Python
- Use `binary.LittleEndian` in Go
- Use `htole64()` in C

### 3. Alignment

- Respect structure alignment requirements
- Use `#[repr(C, align(N))]` in Rust
- Use `__attribute__((aligned(N)))` in C
- Python ctypes handles alignment automatically

### 4. Atomicity

- Head/tail cursors must be updated atomically
- In languages without atomics, use memory barriers
- Single 8-byte writes are atomic on x86-64

### 5. Error Handling

**Check for:**

- Magic number mismatch
- Channel doesn't exist (capacity == 0)
- Buffer full (tail - head >= capacity)
- Buffer empty (head == tail)
- Sequence mismatch (slot not ready)

### 6. Testing

```bash
# Terminal 1: Rust producer
cargo run --example producer 4 1000

# Terminal 2: Python consumer
python3 tests_py/consumer.py 4 1000

# Terminal 3: Your custom consumer
./my_consumer 4 1000
```

### 7. Debugging

```bash
# View shared memory
xxd /dev/shm/dmxp_alloc | head -50

# Check structure sizes
python3 tests_py/debug_shm.py

# Monitor activity
watch -n 0.5 'python3 tests_py/debug_shm.py'
```

## Performance Tips

1. **Batch operations** - Process multiple messages per iteration
2. **Avoid syscalls** - mmap once, reuse the mapping
3. **Cache channel info** - Don't re-read metadata every time
4. **Use SIMD** - For payload processing (if applicable)
5. **Pin threads** - Use CPU affinity for consistent performance

## Common Pitfalls

❌ **Wrong byte order** - Always use little-endian  
❌ **Wrong offsets** - Double-check all calculations  
❌ **Missing alignment** - Structures must be properly aligned  
❌ **Sequence off-by-one** - Remember sequences start at 1  
❌ **Not checking capacity** - Channel might not exist

## Next Steps

- See [MEMORY_LAYOUT.md](./MEMORY_LAYOUT.md) for exact byte offsets
- See [ARCHITECTURE.md](./ARCHITECTURE.md) for design overview
- Check `examples/` directory for reference implementations
