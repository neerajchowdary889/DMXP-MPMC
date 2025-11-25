#!/usr/bin/env python3
"""
Python producer for DMXP-MPMC shared memory
Writes messages to channels in shared memory
"""

import mmap
import os
import time
import sys

# Constants
MAGIC_NUMBER = 0x444D58505F4D454D
SLOT_SIZE = 1088
MAX_CHANNELS = 256

class PythonProducer:
    """Python producer for MPMC shared memory"""
    
    def __init__(self, shm_path="/dev/shm/dmxp_alloc"):
        self.shm_path = shm_path
        self.mm = None
        self.message_counter = 0
    
    def attach(self):
        """Attach to existing shared memory"""
        if not os.path.exists(self.shm_path):
            raise FileNotFoundError(f"Shared memory not found: {self.shm_path}")
        
        # Open and mmap the shared memory file
        fd = os.open(self.shm_path, os.O_RDWR)
        size = os.fstat(fd).st_size
        self.mm = mmap.mmap(fd, size, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE)
        os.close(fd)
        
        # Validate magic number
        magic = int.from_bytes(self.mm[0:8], 'little')
        if magic != MAGIC_NUMBER:
            raise ValueError(f"Invalid magic number: {magic:x}")
        
        print(f"✓ Attached to shared memory")
        
        # Read version and channel count
        version = int.from_bytes(self.mm[8:12], 'little')
        channel_count = int.from_bytes(self.mm[16:20], 'little')
        print(f"  Version: {version}")
        print(f"  Active channels: {channel_count}")
    
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
    
    def send(self, channel_id, payload, debug=False):
        """Send a message to a channel"""
        info = self.get_channel_info(channel_id)
        if not info:
            raise ValueError(f"Channel {channel_id} not found")
        
        tail = info['tail']
        head = info['head']
        capacity = info['capacity']
        band_offset = info['band_offset']
        
        if debug:
            print(f"Channel {channel_id}: head={head}, tail={tail}, capacity={capacity}")
        
        # Check if buffer is full
        if tail - head >= capacity:
            raise IOError(f"Channel {channel_id} is full (head={head}, tail={tail}, capacity={capacity})")
        
        # Calculate slot position
        pos = tail % capacity
        slot_offset = band_offset + (pos * SLOT_SIZE)
        
        if debug:
            print(f"Channel {channel_id}: Writing to slot at offset {slot_offset} (pos={pos})")
        
        # Prepare payload
        if isinstance(payload, str):
            payload_bytes = payload.encode('utf-8')
        else:
            payload_bytes = bytes(payload)
        
        payload_len = len(payload_bytes)
        if payload_len > 960:
            raise ValueError(f"Payload too large: {payload_len} bytes (max 960)")
        
        # Prepare slot data
        slot_data = bytearray(SLOT_SIZE)
        
        # Write sequence number (tail + 1, because sequences start at 1)
        sequence = tail + 1
        slot_data[0:8] = sequence.to_bytes(8, 'little')
        
        # Write MessageMeta (starts at offset 8)
        meta_offset = 8
        
        # message_id
        slot_data[meta_offset:meta_offset+8] = self.message_counter.to_bytes(8, 'little')
        self.message_counter += 1
        
        # timestamp_ns
        timestamp_ns = int(time.time() * 1_000_000_000)
        slot_data[meta_offset+8:meta_offset+16] = timestamp_ns.to_bytes(8, 'little')
        
        # channel_id
        slot_data[meta_offset+16:meta_offset+20] = channel_id.to_bytes(4, 'little')
        
        # message_type (0 for now)
        slot_data[meta_offset+20:meta_offset+24] = (0).to_bytes(4, 'little')
        
        # sender_pid
        slot_data[meta_offset+24:meta_offset+28] = os.getpid().to_bytes(4, 'little')
        
        # sender_runtime (0 for Python)
        slot_data[meta_offset+28:meta_offset+30] = (0).to_bytes(2, 'little')
        
        # flags (0 for now)
        slot_data[meta_offset+30:meta_offset+32] = (0).to_bytes(2, 'little')
        
        # payload_len
        slot_data[meta_offset+32:meta_offset+36] = payload_len.to_bytes(4, 'little')
        
        # Write payload (starts at offset 64)
        payload_offset = 64
        slot_data[payload_offset:payload_offset+payload_len] = payload_bytes
        
        # Write slot to shared memory
        self.mm.seek(slot_offset)
        self.mm.write(bytes(slot_data))
        
        # Increment tail cursor
        new_tail = tail + 1
        tail_offset = 128 + (channel_id * 384) + 128
        self.mm.seek(tail_offset)
        self.mm.write(new_tail.to_bytes(8, 'little'))
        
        if debug:
            print(f"Channel {channel_id}: Sent message (sequence={sequence}, payload_len={payload_len})")
        
        return True
    
    def send_batch(self, channel_id, messages):
        """Send multiple messages to a channel"""
        sent = 0
        for msg in messages:
            try:
                self.send(channel_id, msg)
                sent += 1
            except IOError as e:
                print(f"Warning: {e}")
                break
        return sent
    
    def close(self):
        """Close shared memory"""
        if self.mm:
            try:
                self.mm.close()
            except BufferError:
                pass

def main():
    """Example usage"""
    if len(sys.argv) < 3:
        print("Usage: python producer.py <num_channels> <messages_per_channel>")
        sys.exit(1)
    
    num_channels = int(sys.argv[1])
    messages_per_channel = int(sys.argv[2])
    
    producer = PythonProducer()
    
    try:
        producer.attach()
        
        print(f"\nSending {messages_per_channel} messages to {num_channels} channels...")
        
        start_time = time.time()
        total_sent = 0
        channel_stats = {}
        
        # Send messages to all channels
        for channel_id in range(num_channels):
            print(f"\nSending to channel {channel_id}...")
            sent = 0
            
            for i in range(messages_per_channel):
                message = f"Python message {i} from channel {channel_id}"
                
                try:
                    producer.send(channel_id, message)
                    sent += 1
                    
                    if (i + 1) % 100 == 0:
                        print(f"  Sent {i + 1} messages")
                
                except IOError as e:
                    print(f"  Channel full after {sent} messages: {e}")
                    break
            
            channel_stats[channel_id] = sent
            total_sent += sent
        
        elapsed = time.time() - start_time
        
        # Print statistics
        print("\n" + "=" * 80)
        print("PYTHON PRODUCER STATISTICS")
        print("=" * 80)
        print(f"Channels:              {num_channels}")
        print(f"Messages per channel:  {messages_per_channel}")
        print(f"Total messages sent:   {total_sent}")
        print(f"Time taken:            {elapsed:.3f}s")
        print(f"Throughput (TPS):      {total_sent / elapsed:.2f} messages/sec")
        print(f"Per-channel TPS:       {(total_sent / num_channels) / elapsed:.2f} messages/sec")
        print()
        print("Per-Channel Breakdown:")
        print("-" * 80)
        
        for channel_id, sent in channel_stats.items():
            percentage = (sent / messages_per_channel) * 100 if messages_per_channel > 0 else 0
            print(f"  Channel {channel_id:2}: {sent:6} messages ({percentage:5.1f}%)")
        
        print("=" * 80)
        
        if total_sent == num_channels * messages_per_channel:
            print("✓ All messages sent successfully!")
        else:
            print(f"⚠ Warning: Attempted {num_channels * messages_per_channel} but sent {total_sent}")
    
    finally:
        producer.close()

if __name__ == "__main__":
    main()
