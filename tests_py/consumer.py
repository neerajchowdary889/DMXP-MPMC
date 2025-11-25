#!/usr/bin/env python3
"""
Python consumer for DMXP-MPMC shared memory
Reads messages from channels created by the Rust producer
"""

import ctypes as c
import mmap
import os
import time
from dataclasses import dataclass

# Constants
MAX_CHANNELS = 256
MSG_INLINE = 960  # From Rust: 1024 - 64 (MessageMeta size)
SLOT_SIZE = 1088  # From Rust print_layout: Slot size is 1088 bytes (64-byte aligned)
MAGIC_NUMBER = 0x444D58505F4D454D  # "DMXP_MEM" in hex

# Structures matching Rust layout

class AtomicU64(c.Structure):
    """Represents AtomicU64 as u64 for ABI compatibility"""
    _fields_ = [("value", c.c_uint64)]

class CachePadded(c.Structure):
    """CachePadded<AtomicU64> - 64 bytes total"""
    _fields_ = [
        ("value", AtomicU64),
        ("_pad", c.c_uint8 * 56)  # Pad to 64 bytes
    ]

class MessageMeta(c.Structure):
    """Message metadata - 40 bytes"""
    _pack_ = 1
    _fields_ = [
        ("message_id", c.c_uint64),
        ("timestamp_ns", c.c_uint64),
        ("channel_id", c.c_uint32),
        ("message_type", c.c_uint32),
        ("sender_pid", c.c_uint32),
        ("sender_runtime", c.c_uint16),
        ("flags", c.c_uint16),
        ("payload_len", c.c_uint32),
    ]

class Slot(c.Structure):
    """Slot structure - 1024 bytes, 128-byte aligned"""
    _pack_ = 128
    _fields_ = [
        ("sequence", AtomicU64),
        ("meta", MessageMeta),
        ("_pad1", c.c_uint8 * 16),  # Align to 64 bytes
        ("payload", c.c_uint8 * MSG_INLINE),
    ]

class ChannelEntry(c.Structure):
    """Channel metadata - 384 bytes total, 128-byte aligned
    Layout from Rust:
      channel_id: offset 0
      flags: offset 4
      capacity: offset 8
      band_offset: offset 16
      tail: offset 128 (CachePadded<AtomicU64> = 64 bytes)
      head: offset 256 (CachePadded<AtomicU64> = 64 bytes)
    """
    _fields_ = [
        ("channel_id", c.c_uint32),      # offset 0
        ("flags", c.c_uint32),            # offset 4
        ("capacity", c.c_uint64),         # offset 8
        ("band_offset", c.c_uint64),      # offset 16
        ("_pad1", c.c_uint8 * 104),       # pad to offset 128
        ("tail", CachePadded),            # offset 128, 64 bytes
        ("head", CachePadded),            # offset 256, 64 bytes
        ("_pad2", c.c_uint8 * 64),        # pad to 384 bytes total
    ]

class GlobalHeader(c.Structure):
    """Global header - 98432 bytes total, 128-byte aligned
    Layout from Rust:
      magic: offset 0
      version: offset 8
      max_channels: offset 12
      channel_count: offset 16
      reserved: offset 20
      channels: offset 128
    """
    _fields_ = [
        ("magic", c.c_uint64),            # offset 0
        ("version", c.c_uint32),          # offset 8
        ("max_channels", c.c_uint32),     # offset 12
        ("channel_count", c.c_uint32),    # offset 16
        ("reserved", c.c_uint32),         # offset 20
        ("_pad", c.c_uint8 * 104),        # pad to offset 128
        ("channels", ChannelEntry * MAX_CHANNELS),  # offset 128
    ]

@dataclass
class Message:
    """Decoded message"""
    channel_id: int
    message_id: int
    timestamp_ns: int
    payload: bytes

class PythonConsumer:
    """Python consumer for MPMC shared memory"""
    
    def __init__(self, shm_path="/dev/shm/dmxp_alloc"):
        self.shm_path = shm_path
        self.mm = None
        self.header = None
        
    def attach(self):
        """Attach to existing shared memory"""
        if not os.path.exists(self.shm_path):
            raise FileNotFoundError(f"Shared memory not found: {self.shm_path}")
        
        # Open and mmap the shared memory file
        fd = os.open(self.shm_path, os.O_RDWR)
        size = os.fstat(fd).st_size
        self.mm = mmap.mmap(fd, size, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE)
        os.close(fd)
        
        # Read global header
        self.header = GlobalHeader.from_buffer(self.mm)
        
        # Validate magic number
        if self.header.magic != MAGIC_NUMBER:
            raise ValueError(f"Invalid magic number: {self.header.magic:x}")
        
        print(f"✓ Attached to shared memory")
        print(f"  Version: {self.header.version}")
        print(f"  Active channels: {self.header.channel_count}")
        
    def get_channel_info(self, channel_id):
        """Get channel metadata by reading raw bytes"""
        if channel_id >= MAX_CHANNELS:
            return None
        
        # Calculate offset: channels start at 128, each entry is 384 bytes
        offset = 128 + (channel_id * 384)
        
        # Read fields directly from memory
        self.mm.seek(offset)
        data = self.mm.read(384)
        
        ch_id = int.from_bytes(data[0:4], 'little')
        flags = int.from_bytes(data[4:8], 'little')
        capacity = int.from_bytes(data[8:16], 'little')
        band_offset = int.from_bytes(data[16:24], 'little')
        tail = int.from_bytes(data[128:136], 'little')
        head = int.from_bytes(data[256:264], 'little')
        
        if capacity == 0:
            return None
        
        return {
            'channel_id': ch_id,
            'capacity': capacity,
            'band_offset': band_offset,
            'head': head,
            'tail': tail,
        }
    
    def list_channels(self):
        """List all active channels"""
        channels = []
        for i in range(MAX_CHANNELS):
            info = self.get_channel_info(i)
            if info:
                channels.append(info)
        return channels
    
    def receive(self, channel_id, debug=False):
        """Receive one message from a channel"""
        info = self.get_channel_info(channel_id)
        if not info:
            if debug:
                print(f"Channel {channel_id}: No channel info")
            return None
        
        # Get current head and tail positions
        head = info['head']
        tail = info['tail']
        capacity = info['capacity']
        band_offset = info['band_offset']
        
        if debug:
            print(f"Channel {channel_id}: head={head}, tail={tail}, capacity={capacity}")
        
        # Check if buffer is empty
        if head == tail:
            if debug:
                print(f"Channel {channel_id}: Buffer empty (head == tail)")
            return None
        
        # Calculate slot position
        pos = head % capacity
        slot_offset = band_offset + (pos * SLOT_SIZE)
        
        if debug:
            print(f"Channel {channel_id}: Reading slot at offset {slot_offset} (pos={pos})")
        
        # Read slot
        try:
            self.mm.seek(slot_offset)
            slot_data = self.mm.read(SLOT_SIZE)
        except Exception as e:
            if debug:
                print(f"Channel {channel_id}: Error reading slot: {e}")
            return None
        
        # Read sequence number (first 8 bytes)
        sequence = int.from_bytes(slot_data[0:8], 'little')
        
        if debug:
            print(f"Channel {channel_id}: Slot sequence={sequence}, expected={head+1}")
        
        # Check sequence number (sequences start at 1, so first message has seq=1 when head=0)
        if sequence != head + 1:
            if debug:
                print(f"Channel {channel_id}: Sequence mismatch")
            return None
        
        # Read MessageMeta (starts at offset 8 in slot, after sequence)
        meta_offset = 8
        message_id = int.from_bytes(slot_data[meta_offset:meta_offset+8], 'little')
        timestamp_ns = int.from_bytes(slot_data[meta_offset+8:meta_offset+16], 'little')
        ch_id = int.from_bytes(slot_data[meta_offset+16:meta_offset+20], 'little')
        payload_len = int.from_bytes(slot_data[meta_offset+32:meta_offset+36], 'little')
        
        # Payload starts after sequence (8) + MessageMeta (40) + padding (16) = 64 bytes
        payload_offset = 64
        payload = slot_data[payload_offset:payload_offset+payload_len]
        
        message = Message(
            channel_id=ch_id,
            message_id=message_id,
            timestamp_ns=timestamp_ns,
            payload=payload
        )
        
        # Increment head - write back to shared memory
        new_head = head + 1
        head_offset = 128 + (channel_id * 384) + 256  # ChannelEntry.head offset
        self.mm.seek(head_offset)
        self.mm.write(new_head.to_bytes(8, 'little'))
        
        return message
    
    def consume_all(self, channel_id, max_messages=None):
        """Consume all available messages from a channel"""
        messages = []
        count = 0
        
        while True:
            if max_messages and count >= max_messages:
                break
            
            msg = self.receive(channel_id)
            if msg is None:
                break
            
            messages.append(msg)
            count += 1
        
        return messages
    
    def close(self):
        """Close shared memory"""
        if self.mm:
            try:
                self.mm.close()
            except BufferError:
                pass  # Ignore buffer errors on close

def main():
    """Example usage"""
    import sys
    
    if len(sys.argv) < 2:
        print("Usage: python consumer.py <num_channels> [messages_per_channel]")
        sys.exit(1)
    
    num_channels = int(sys.argv[1])
    expected_per_channel = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
    
    consumer = PythonConsumer()
    
    try:
        consumer.attach()
        
        # List available channels
        channels = consumer.list_channels()
        print(f"\nAvailable channels: {[c['channel_id'] for c in channels]}")
        
        # Consume from all requested channels
        start_time = time.time()
        total_received = 0
        channel_stats = {}
        
        for channel_id in range(num_channels):
            print(f"\nConsuming from channel {channel_id}...")
            messages = consumer.consume_all(channel_id, expected_per_channel)
            channel_stats[channel_id] = len(messages)
            total_received += len(messages)
            
            # Print first few messages
            for i, msg in enumerate(messages[:5]):
                payload_str = msg.payload.decode('utf-8', errors='ignore')
                print(f"  [{i}] Channel {msg.channel_id}: {payload_str[:80]}")
            
            if len(messages) > 5:
                print(f"  ... and {len(messages) - 5} more messages")
        
        elapsed = time.time() - start_time
        
        # Print statistics
        print("\n" + "=" * 80)
        print("PYTHON CONSUMER STATISTICS")
        print("=" * 80)
        print(f"Channels:                 {num_channels}")
        print(f"Expected per channel:     {expected_per_channel}")
        print(f"Total expected:           {num_channels * expected_per_channel}")
        print(f"Total received:           {total_received}")
        print(f"Time taken:               {elapsed:.3f}s")
        print(f"Throughput (TPS):         {total_received / elapsed:.2f} messages/sec")
        print(f"Per-channel TPS:          {(total_received / num_channels) / elapsed:.2f} messages/sec")
        print()
        print("Per-Channel Breakdown:")
        print("-" * 80)
        
        for channel_id, count in channel_stats.items():
            percentage = (count / expected_per_channel) * 100 if expected_per_channel > 0 else 0
            print(f"  Channel {channel_id:2}: {count:6} messages ({percentage:5.1f}%)")
        
        print("=" * 80)
        
        if total_received == num_channels * expected_per_channel:
            print("✓ All messages received successfully!")
        else:
            print(f"⚠ Warning: Expected {num_channels * expected_per_channel} but received {total_received}")
        
    finally:
        consumer.close()

if __name__ == "__main__":
    main()
