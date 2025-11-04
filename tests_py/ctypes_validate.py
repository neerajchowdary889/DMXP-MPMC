import ctypes as c

class MessageMeta(c.Structure):
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

class AtomicU64(c.Structure):
    # Represent AtomicU64 as u64 for ABI layout
    _fields_ = [("value", c.c_uint64)]

class SlotHeader(c.Structure):
    _fields_ = [
        ("sequence", AtomicU64),
        ("length", AtomicU64),
    ]

def main():
    # MessageMeta checks
    # C ABI aligns struct size to the max field alignment (8), so total is 40
    assert c.sizeof(MessageMeta) == 40, c.sizeof(MessageMeta)
    assert c.alignment(MessageMeta) == c.alignment(c.c_uint64)

    assert getattr(MessageMeta, 'message_id').offset == 0
    assert getattr(MessageMeta, 'timestamp_ns').offset == 8
    assert getattr(MessageMeta, 'channel_id').offset == 16
    assert getattr(MessageMeta, 'message_type').offset == 20
    assert getattr(MessageMeta, 'sender_pid').offset == 24
    assert getattr(MessageMeta, 'sender_runtime').offset == 28
    assert getattr(MessageMeta, 'flags').offset == 30
    assert getattr(MessageMeta, 'payload_len').offset == 32

    # SlotHeader checks
    assert c.sizeof(SlotHeader) == 16, c.sizeof(SlotHeader)
    assert c.alignment(SlotHeader) == c.alignment(c.c_uint64)
    assert getattr(SlotHeader, 'sequence').offset == 0
    assert getattr(SlotHeader, 'length').offset == 8

    print("ctypes validation passed")

if __name__ == "__main__":
    main()


