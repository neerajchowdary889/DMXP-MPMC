#!/usr/bin/env python3
"""
Simple example: Using Python Producer
"""

from tests_py.producer import PythonProducer

# Create producer
producer = PythonProducer()
producer.attach()

# Send a single message
producer.send(0, "Hello from Python!")
print("✓ Sent message to channel 0")

# Send multiple messages
messages = [f"Message {i}" for i in range(10)]
sent = producer.send_batch(0, messages)
print(f"✓ Sent {sent} messages to channel 0")

# Send to different channels
for channel_id in range(4):
    producer.send(channel_id, f"Hello to channel {channel_id}")
print("✓ Sent messages to channels 0-3")

producer.close()
print("✓ Done!")
