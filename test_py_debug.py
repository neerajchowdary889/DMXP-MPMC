#!/usr/bin/env python3
"""Quick test with debug output"""
import sys
sys.path.insert(0, 'tests_py')
from consumer import PythonConsumer

consumer = PythonConsumer()
consumer.attach()

print("\nTrying to receive from channel 0 with debug...")
msg = consumer.receive(0, debug=True)
print(f"Result: {msg}")

consumer.close()
