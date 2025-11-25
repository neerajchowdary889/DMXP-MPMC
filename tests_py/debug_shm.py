#!/usr/bin/env python3
"""
Debug script to print raw bytes from shared memory
"""
import mmap
import os

shm_path = "/dev/shm/dmxp_alloc"

if not os.path.exists(shm_path):
    print(f"Shared memory not found: {shm_path}")
    exit(1)

fd = os.open(shm_path, os.O_RDONLY)
size = os.fstat(fd).st_size
mm = mmap.mmap(fd, size, mmap.MAP_SHARED, mmap.PROT_READ)
os.close(fd)

print(f"Shared memory size: {size} bytes\n")

# Read GlobalHeader
print("=== GlobalHeader ===")
print(f"Magic (0-8):          {int.from_bytes(mm[0:8], 'little'):016x}")
print(f"Version (8-12):       {int.from_bytes(mm[8:12], 'little')}")
print(f"Max channels (12-16): {int.from_bytes(mm[12:16], 'little')}")
print(f"Channel count (16-20):{int.from_bytes(mm[16:20], 'little')}")
print(f"Reserved (20-24):     {int.from_bytes(mm[20:24], 'little')}")

# Channels array starts at offset 128
print(f"\n=== Channels Array (starts at offset 128) ===")

for i in range(4):
    offset = 128 + (i * 384)  # Each ChannelEntry is 384 bytes
    print(f"\nChannel {i} (offset {offset}):")
    print(f"  channel_id (0):   {int.from_bytes(mm[offset+0:offset+4], 'little')}")
    print(f"  flags (4):        {int.from_bytes(mm[offset+4:offset+8], 'little')}")
    print(f"  capacity (8):     {int.from_bytes(mm[offset+8:offset+16], 'little')}")
    print(f"  band_offset (16): {int.from_bytes(mm[offset+16:offset+24], 'little')}")
    print(f"  tail (128):       {int.from_bytes(mm[offset+128:offset+136], 'little')}")
    print(f"  head (256):       {int.from_bytes(mm[offset+256:offset+264], 'little')}")

mm.close()
