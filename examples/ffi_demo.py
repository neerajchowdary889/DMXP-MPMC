#!/usr/bin/env python3
import ctypes
import os
import sys
import time
from ctypes import c_void_p, c_int, c_uint32, c_char_p, POINTER, c_ubyte, c_size_t, byref

# Define constants
DMXP_SUCCESS = 0
DMXP_ERROR_EMPTY = -5

class DMXP:
    def __init__(self, lib_path=None):
        if lib_path is None:
            # Try to find the library in standard locations or target/debug/release
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
        
        # Define function signatures
        self.lib.dmxp_producer_new.argtypes = [c_uint32, c_uint32]
        self.lib.dmxp_producer_new.restype = c_void_p
        
        self.lib.dmxp_producer_send.argtypes = [c_void_p, POINTER(c_ubyte), c_size_t]
        self.lib.dmxp_producer_send.restype = c_int
        
        self.lib.dmxp_producer_free.argtypes = [c_void_p]
        self.lib.dmxp_producer_free.restype = None
        
        self.lib.dmxp_consumer_new.argtypes = [c_uint32]
        self.lib.dmxp_consumer_new.restype = c_void_p
        
        self.lib.dmxp_consumer_receive.argtypes = [c_void_p, c_int, POINTER(c_ubyte), POINTER(c_size_t)]
        self.lib.dmxp_consumer_receive.restype = c_int
        
        self.lib.dmxp_consumer_free.argtypes = [c_void_p]
        self.lib.dmxp_consumer_free.restype = None

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
        
        # Create ctypes buffer
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
        self.buffer = (c_ubyte * 1024)() # Pre-allocate 1KB buffer

    def receive(self, blocking=True):
        out_len = c_size_t(len(self.buffer))
        block_flag = 1 if blocking else 0
        
        res = self.lib.dmxp_consumer_receive(self.handle, block_flag, self.buffer, byref(out_len))
        
        if res == DMXP_SUCCESS:
            # Convert to bytes
            return bytes(self.buffer[:out_len.value])
        elif res == DMXP_ERROR_EMPTY:
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

    channel_id = 0
    
    print(f"Creating producer for channel {channel_id}...")
    producer = dmxp.create_producer(channel_id)
    
    msg = "Hello from Python via FFI!"
    print(f"Sending: '{msg}'")
    producer.send(msg)
    
    print(f"Creating consumer for channel {channel_id}...")
    consumer = dmxp.create_consumer(channel_id)
    
    print("Receiving...")
    received = consumer.receive()
    print(f"Received: '{received.decode('utf-8')}'")
    
    print("Success!")

if __name__ == "__main__":
    main()
