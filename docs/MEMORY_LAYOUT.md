# Memory Layout Reference

This document provides exact byte-level specifications for all structures in DMXP-MPMC shared memory.

## Quick Reference

| Structure              | Size         | Alignment | Location                |
| ---------------------- | ------------ | --------- | ----------------------- |
| GlobalHeader           | 98,432 bytes | 128 bytes | Offset 0                |
| ChannelEntry           | 384 bytes    | 128 bytes | Offset 128+             |
| Slot                   | 1,088 bytes  | 64 bytes  | Variable (band_offset)  |
| MessageMeta            | 40 bytes     | 8 bytes   | Inside Slot at offset 8 |
| CachePadded<AtomicU64> | 64 bytes     | 8 bytes   | Inside ChannelEntry     |

## GlobalHeader

**Total Size**: 98,432 bytes  
**Alignment**: 128 bytes  
**Location**: Offset 0 in shared memory

### Field Layout

| Offset | Size   | Type              | Field         | Description                                     |
| ------ | ------ | ----------------- | ------------- | ----------------------------------------------- |
| 0      | 8      | u64               | magic         | Magic number: `0x444D58505F4D454D` ("DMXP_MEM") |
| 8      | 4      | u32               | version       | Layout version (currently 1)                    |
| 12     | 4      | u32               | max_channels  | Maximum channels (256)                          |
| 16     | 4      | u32               | channel_count | Active channel count                            |
| 20     | 4      | u32               | reserved      | Reserved for future use                         |
| 24     | 104    | -                 | \_pad         | Padding to offset 128                           |
| 128    | 98,304 | ChannelEntry[256] | channels      | Array of channel metadata                       |

### Rust Definition

```rust
#[repr(C, align(128))]
pub struct GlobalHeader {
    pub magic: u64,
    pub version: u32,
    pub max_channels: u32,
    pub channel_count: u32,
    pub reserved: u32,
    pub channels: [ChannelEntry; MAX_CHANNELS],
}
```

### Python Definition

```python
class GlobalHeader(ctypes.Structure):
    _fields_ = [
        ("magic", ctypes.c_uint64),
        ("version", ctypes.c_uint32),
        ("max_channels", ctypes.c_uint32),
        ("channel_count", ctypes.c_uint32),
        ("reserved", ctypes.c_uint32),
        ("_pad", ctypes.c_uint8 * 104),
        ("channels", ChannelEntry * 256),
    ]
```

## ChannelEntry

**Total Size**: 384 bytes  
**Alignment**: 128 bytes  
**Location**: Offset 128 + (channel_id × 384)

### Field Layout

| Offset | Size | Type                   | Field       | Description                      |
| ------ | ---- | ---------------------- | ----------- | -------------------------------- |
| 0      | 4    | u32                    | channel_id  | Logical channel identifier       |
| 4      | 4    | u32                    | flags       | Channel flags (reserved)         |
| 8      | 8    | u64                    | capacity    | Number of slots in ring buffer   |
| 16     | 8    | u64                    | band_offset | Byte offset to ring buffer start |
| 24     | 104  | -                      | \_pad1      | Padding to offset 128            |
| 128    | 64   | CachePadded<AtomicU64> | tail        | Producer cursor (write position) |
| 192    | 64   | -                      | \_pad2      | Padding between tail and head    |
| 256    | 64   | CachePadded<AtomicU64> | head        | Consumer cursor (read position)  |
| 320    | 64   | -                      | \_pad3      | Padding to 384 bytes             |

### Rust Definition

```rust
#[repr(C, align(128))]
pub struct ChannelEntry {
    pub channel_id: u32,
    pub flags: u32,
    pub capacity: u64,
    pub band_offset: u64,
    pub tail: CachePadded<AtomicU64>,
    pub head: CachePadded<AtomicU64>,
    pub _pad: [u64; 0],
}
```

### Python Definition

```python
class ChannelEntry(ctypes.Structure):
    _fields_ = [
        ("channel_id", ctypes.c_uint32),
        ("flags", ctypes.c_uint32),
        ("capacity", ctypes.c_uint64),
        ("band_offset", ctypes.c_uint64),
        ("_pad1", ctypes.c_uint8 * 104),
        ("tail", CachePadded),  # 64 bytes
        ("head", CachePadded),  # 64 bytes
        ("_pad2", ctypes.c_uint8 * 64),
    ]
```

## CachePadded<AtomicU64>

**Total Size**: 64 bytes  
**Alignment**: 8 bytes  
**Purpose**: Prevent false sharing between cache lines

### Field Layout

| Offset | Size | Type      | Field | Description             |
| ------ | ---- | --------- | ----- | ----------------------- |
| 0      | 8    | AtomicU64 | value | The actual atomic value |
| 8      | 56   | -         | \_pad | Padding to 64 bytes     |

### Rust Definition

```rust
#[repr(C, align(64))]
pub struct CachePadded<T> {
    value: T,
    _pad: [u8; 64 - std::mem::size_of::<T>()],
}
```

### Python Definition

```python
class CachePadded(ctypes.Structure):
    _fields_ = [
        ("value", AtomicU64),
        ("_pad", ctypes.c_uint8 * 56),
    ]
```

## Slot

**Total Size**: 1,088 bytes  
**Alignment**: 64 bytes  
**Location**: band_offset + (slot_index × 1088)

### Field Layout

| Offset | Size | Type        | Field    | Description                         |
| ------ | ---- | ----------- | -------- | ----------------------------------- |
| 0      | 8    | AtomicU64   | sequence | Sequence number for synchronization |
| 8      | 40   | MessageMeta | meta     | Message metadata                    |
| 48     | 16   | -           | \_pad1   | Padding to 64 bytes                 |
| 64     | 960  | u8[960]     | payload  | Message payload data                |
| 1024   | 64   | -           | \_pad2   | Padding to 1088 bytes               |

### Rust Definition

```rust
#[repr(C, align(64))]
pub struct Slot {
    pub sequence: AtomicU64,
    pub meta: MessageMeta,
    pub _pad1: [u8; 16],
    pub payload: [u8; MSG_INLINE],
}
```

### Python Reading (Raw Bytes)

```python
# Read slot at position
slot_offset = band_offset + (pos * 1088)
mm.seek(slot_offset)
slot_data = mm.read(1088)

# Parse fields
sequence = int.from_bytes(slot_data[0:8], 'little')
# MessageMeta starts at offset 8
# Payload starts at offset 64
payload = slot_data[64:64+payload_len]
```

## MessageMeta

**Total Size**: 40 bytes  
**Alignment**: 8 bytes  
**Location**: Inside Slot at offset 8

### Field Layout

| Offset | Size | Type | Field          | Description                        |
| ------ | ---- | ---- | -------------- | ---------------------------------- |
| 0      | 8    | u64  | message_id     | Unique message identifier          |
| 8      | 8    | u64  | timestamp_ns   | Timestamp in nanoseconds           |
| 16     | 4    | u32  | channel_id     | Channel this message belongs to    |
| 20     | 4    | u32  | message_type   | Message type (application-defined) |
| 24     | 4    | u32  | sender_pid     | Process ID of sender               |
| 28     | 2    | u16  | sender_runtime | Runtime identifier                 |
| 30     | 2    | u16  | flags          | Message flags                      |
| 32     | 4    | u32  | payload_len    | Actual payload length in bytes     |
| 36     | 4    | -    | \_pad          | Padding to 40 bytes                |

### Rust Definition

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessageMeta {
    pub message_id: u64,
    pub timestamp_ns: u64,
    pub channel_id: u32,
    pub message_type: u32,
    pub sender_pid: u32,
    pub sender_runtime: u16,
    pub flags: u16,
    pub payload_len: u32,
}
```

### Python Definition

```python
class MessageMeta(ctypes.Structure):
    _pack_ = 1
    _fields_ = [
        ("message_id", ctypes.c_uint64),
        ("timestamp_ns", ctypes.c_uint64),
        ("channel_id", ctypes.c_uint32),
        ("message_type", ctypes.c_uint32),
        ("sender_pid", ctypes.c_uint32),
        ("sender_runtime", ctypes.c_uint16),
        ("flags", ctypes.c_uint16),
        ("payload_len", ctypes.c_uint32),
    ]
```

## Calculating Offsets

### Channel Entry Offset

```
channel_entry_offset = 128 + (channel_id × 384)
```

### Ring Buffer Offset

```
ring_buffer_offset = ChannelEntry.band_offset
```

### Slot Offset

```
slot_index = cursor % capacity
slot_offset = band_offset + (slot_index × 1088)
```

### Field Offsets Within Slot

```
sequence_offset = slot_offset + 0
meta_offset = slot_offset + 8
payload_offset = slot_offset + 64
```

## Atomic Operations

### Reading Cursors

```rust
// Rust
let head = channel.head.load(Ordering::Acquire);
let tail = channel.tail.load(Ordering::Acquire);
```

```python
# Python (raw bytes)
head_offset = 128 + (channel_id * 384) + 256
mm.seek(head_offset)
head = int.from_bytes(mm.read(8), 'little')

tail_offset = 128 + (channel_id * 384) + 128
mm.seek(tail_offset)
tail = int.from_bytes(mm.read(8), 'little')
```

### Writing Cursors

```rust
// Rust
channel.tail.fetch_add(1, Ordering::AcqRel);
```

```python
# Python (raw bytes)
new_head = head + 1
head_offset = 128 + (channel_id * 384) + 256
mm.seek(head_offset)
mm.write(new_head.to_bytes(8, 'little'))
```

## Validation Checklist

When implementing a consumer/producer, verify:

- [ ] GlobalHeader.magic == `0x444D58505F4D454D`
- [ ] GlobalHeader.version == 1
- [ ] ChannelEntry.capacity > 0 (channel exists)
- [ ] Slot.sequence == head + 1 (slot is ready)
- [ ] MessageMeta.payload_len <= 960 (valid payload size)
- [ ] All offsets are correctly calculated
- [ ] Byte order is little-endian
- [ ] Alignment requirements are met

## Example: Reading a Message (Python)

```python
import mmap
import os

# Open shared memory
fd = os.open("/dev/shm/dmxp_alloc", os.O_RDWR)
mm = mmap.mmap(fd, 0, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE)
os.close(fd)

# Read channel 0 metadata
channel_offset = 128 + (0 * 384)
mm.seek(channel_offset)
channel_data = mm.read(384)

capacity = int.from_bytes(channel_data[8:16], 'little')
band_offset = int.from_bytes(channel_data[16:24], 'little')
head = int.from_bytes(channel_data[256:264], 'little')
tail = int.from_bytes(channel_data[128:136], 'little')

# Check if messages available
if head != tail:
    # Calculate slot position
    pos = head % capacity
    slot_offset = band_offset + (pos * 1088)

    # Read slot
    mm.seek(slot_offset)
    slot_data = mm.read(1088)

    # Parse slot
    sequence = int.from_bytes(slot_data[0:8], 'little')

    if sequence == head + 1:
        # Read message metadata
        payload_len = int.from_bytes(slot_data[40:44], 'little')

        # Read payload
        payload = slot_data[64:64+payload_len]

        print(f"Received: {payload.decode('utf-8')}")

        # Increment head
        new_head = head + 1
        mm.seek(channel_offset + 256)
        mm.write(new_head.to_bytes(8, 'little'))

mm.close()
```

## Constants

```rust
const MAX_CHANNELS: usize = 256;
const MSG_INLINE: usize = 960;
const SLOT_SIZE: usize = 1088;
const CHANNEL_ENTRY_SIZE: usize = 384;
const GLOBAL_HEADER_SIZE: usize = 98432;
const MAGIC_NUMBER: u64 = 0x444D58505F4D454D;
```
