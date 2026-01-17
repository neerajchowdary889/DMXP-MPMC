#!/usr/bin/env python3
import ctypes
import os
import sys
import time
from ctypes import c_void_p, c_int, c_uint32, c_char_p, POINTER, c_ubyte, c_size_t, c_uint64, c_uint16, byref, Structure

# Define constants
DMXP_SUCCESS = 0
DMXP_ERROR_EMPTY = -5
DMXP_ERROR_TIMEOUT = -7

class FFIMessageMeta(Structure):
    _fields_ = [
        ("message_id", c_uint64),
        ("timestamp_ns", c_uint64),
        ("channel_id", c_uint32),
        ("message_type", c_uint32),
        ("sender_pid", c_uint32),
        ("sender_runtime", c_uint16),
        ("flags", c_uint16),
        ("payload_len", c_uint32),
    ]

class DMXP:
    def __init__(self, lib_path=None):
        if lib_path is None:
            paths = [
                "./target/debug/libdmxp_kvcache.so",
                "./target/release/libdmxp_kvcache.so",
                "./libdmxp_kvcache.so"
            ]
            for p in paths:
                if os.path.exists(p):
                    lib_path = p
                    break
        
        if lib_path is None or not os.path.exists(lib_path):
            raise FileNotFoundError("Could not find libdmxp_kvcache.so. Run 'cargo build' first.")

        self.lib = ctypes.CDLL(lib_path)
        
        # Init functions
        self.lib.dmxp_channel_count.argtypes = []
        self.lib.dmxp_channel_count.restype = c_int
        
        self.lib.dmxp_list_channels.argtypes = [POINTER(c_uint32), c_size_t, POINTER(c_size_t)]
        self.lib.dmxp_list_channels.restype = c_int

        self.lib.dmxp_producer_new.argtypes = [c_uint32, c_uint32]
        self.lib.dmxp_producer_new.restype = c_void_p
        
        self.lib.dmxp_producer_send.argtypes = [c_void_p, POINTER(c_ubyte), c_size_t]
        self.lib.dmxp_producer_send.restype = c_int
        
        self.lib.dmxp_producer_free.argtypes = [c_void_p]
        self.lib.dmxp_producer_free.restype = None
        
        self.lib.dmxp_consumer_new.argtypes = [c_uint32]
        self.lib.dmxp_consumer_new.restype = c_void_p
        
        self.lib.dmxp_consumer_receive_ext.argtypes = [c_void_p, c_int, POINTER(c_ubyte), POINTER(c_size_t), POINTER(FFIMessageMeta)]
        self.lib.dmxp_consumer_receive_ext.restype = c_int
        
        self.lib.dmxp_consumer_free.argtypes = [c_void_p]
        self.lib.dmxp_consumer_free.restype = None

    def channel_count(self):
        return self.lib.dmxp_channel_count()

    def list_channels(self):
        max_channels = 100
        buf = (c_uint32 * max_channels)()
        count = c_size_t(0)
        
        if self.lib.dmxp_list_channels(buf, max_channels, byref(count)) == DMXP_SUCCESS:
            return [buf[i] for i in range(count.value)]
        return []

    def create_producer(self, channel_id, capacity=1024):
        return Producer(self.lib, channel_id, capacity)

    def create_consumer(self, channel_id):
        return Consumer(self.lib, channel_id)

class Producer:
    def __init__(self, lib, channel_id, capacity):
        self.lib = lib
        self.handle = self.lib.dmxp_producer_new(channel_id, capacity)
        if not self.handle:
            raise RuntimeError(f"Failed to create producer for channel {channel_id}")

    def send(self, data):
        if isinstance(data, str):
            data = data.encode('utf-8')
        buf = (c_ubyte * len(data)).from_buffer_copy(data)
        res = self.lib.dmxp_producer_send(self.handle, buf, len(data))
        if res != DMXP_SUCCESS:
            raise RuntimeError(f"Failed to send message: error {res}")

    def close(self):
        if self.handle:
            self.lib.dmxp_producer_free(self.handle)
            self.handle = None

    def __del__(self):
        self.close()

class Consumer:
    def __init__(self, lib, channel_id):
        self.lib = lib
        self.handle = self.lib.dmxp_consumer_new(channel_id)
        if not self.handle:
            raise RuntimeError(f"Failed to create consumer for channel {channel_id}")
        self.buffer = (c_ubyte * 65536)() # 64KB buf

    def receive(self, timeout_ms=None, with_meta=True):
        """
        Receive message.
        timeout_ms: None (blocking), 0 (non-blocking), >0 (timeout in ms)
        with_meta: If True, returns (bytes, metadata_dict). If False, returns bytes.
        Returns: Data or None if timeout/empty
        """
        out_len = c_size_t(len(self.buffer))
        
        if with_meta:
            meta = FFIMessageMeta()
            meta_ptr = byref(meta)
        else:
            meta_ptr = None
        
        # Convert timeout arg
        if timeout_ms is None:
            c_timeout = -1  # Blocking
        else:
            c_timeout = int(timeout_ms)
            
        res = self.lib.dmxp_consumer_receive_ext(
            self.handle, 
            c_timeout, 
            self.buffer, 
            byref(out_len),
            meta_ptr
        )
        
        if res == DMXP_SUCCESS:
            data = bytes(self.buffer[:out_len.value])
            if with_meta:
                meta_dict = {
                    'message_id': meta.message_id,
                    'timestamp_ns': meta.timestamp_ns,
                    'channel_id': meta.channel_id,
                    'sender_pid': meta.sender_pid
                }
                return data, meta_dict
            else:
                return data
        elif res == DMXP_ERROR_EMPTY or res == DMXP_ERROR_TIMEOUT:
            return None
        else:
             raise RuntimeError(f"Receive failed: error {res}")

    def close(self):
        if self.handle:
            self.lib.dmxp_consumer_free(self.handle)
            self.handle = None

    def __del__(self):
        self.close()

def main():
    print("Loading DMXP library...")
    try:
        dmxp = DMXP()
    except FileNotFoundError as e:
        print(e)
        return

    channel_id = 99
    
    print(f"Creating producer for channel {channel_id}...")
    producer = dmxp.create_producer(channel_id, 2048)
    
    # Check active channels
    print(f"Active channels: {dmxp.list_channels()}")
    print(f"Total count: {dmxp.channel_count()}")
    
    msg = "Hello with Metadata!"
    print(f"Sending: '{msg}'")
    producer.send(msg)
    
    print(f"Creating consumer for channel {channel_id}...")
    consumer = dmxp.create_consumer(channel_id)
    
    print("Receiving with 1000ms timeout (WITH metadata)...")
    result = consumer.receive(timeout_ms=1000, with_meta=True)
    
    if result:
        data, meta = result
        print(f"Received: '{data.decode('utf-8')}'")
        print(f"Metadata: ID={meta['message_id']}, PID={meta['sender_pid']}, Time={meta['timestamp_ns']}")
    else:
        print("Timed out!")
        
    # Send another message for no-meta test
    producer.send("Message without checking metadata")
    print("\nReceiving (WITHOUT metadata)...")
    data_only = consumer.receive(timeout_ms=1000, with_meta=False)
    if data_only:
         print(f"Received raw data only: '{data_only.decode('utf-8')}'")
    
    print("Success!")

if __name__ == "__main__":
    main()
