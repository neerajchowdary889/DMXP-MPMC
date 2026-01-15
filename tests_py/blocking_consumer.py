#!/usr/bin/env python3
"""
Blocking Python consumer example
"""

from consumer import PythonConsumer
import sys

def main():
    if len(sys.argv) < 2:
        print("Usage: python blocking_consumer.py <channel_id>")
        sys.exit(1)
        
    channel_id = int(sys.argv[1])
    
    print(f"Blocking Consumer: Connecting to channel {channel_id}")
    consumer = PythonConsumer()
    consumer.attach()
    
    print("Blocking Consumer: Waiting for messages...")
    
    try:
        while True:
            msg = consumer.receive_blocking(channel_id)
            payload = msg.payload.decode('utf-8', errors='ignore')
            print(f"Received: {payload}")
    except KeyboardInterrupt:
        print("\nStopping...")
    finally:
        consumer.close()

if __name__ == "__main__":
    main()
