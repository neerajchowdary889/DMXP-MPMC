import ctypes
from ctypes import c_void_p, c_uint32, c_char_p, c_size_t
import time
import os

# Load Library
lib_path = os.path.abspath("target/debug/libdmxp_kvcache.so")
lib = ctypes.CDLL(lib_path)

# Define Producer New
lib.dmxp_producer_new.argtypes = [c_uint32, c_uint32]
lib.dmxp_producer_new.restype = c_void_p

# Define Producer Send
lib.dmxp_producer_send.argtypes = [c_void_p, c_void_p, c_size_t]
lib.dmxp_producer_send.restype = ctypes.c_int32

def main():
    channel_id = 100
    print(f"Python Producer creating channel {channel_id}...")
    
    # Create producer (Capacity 1024)
    producer = lib.dmxp_producer_new(channel_id, 1024)
    if not producer:
        print("Failed to create producer")
        return

    print("Sending messages to Go...")
    
    messages = [
        "Hello from Python!",
        "Data Packet 1",
        "Data Packet 2",
        "Bye from Python!"
    ]

    for i, msg in enumerate(messages):
        print(f"Python Sending: {msg}")
        msg_bytes = msg.encode('utf-8')
        lib.dmxp_producer_send(producer, msg_bytes, len(msg_bytes))
 
    print("Python Done.")

if __name__ == "__main__":
    main()
