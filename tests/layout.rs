// Layout conformance tests for ABI stability across languages.
// These tests assert sizes, alignments, and field offsets for
// MessageMeta and SlotHeader. They also print the observed values
// to aid debugging when a mismatch occurs on a given platform.
// use dmxp_kvcache::MPMC::Buffer::SlotHeader; // Removed
use dmxp_kvcache::MPMC::Structs::MessageMeta;
use memoffset::offset_of;
use std::mem::{align_of, size_of};

#[test]
fn test_message_meta_layout() {
    // Calculate expected size with 8-byte alignment (due to u64 fields).
    let raw = 8 + 8 + 4 + 4 + 4 + 2 + 2 + 4; // 36 bytes of fields
    let aligned = (raw + 7) & !7; // round up to 8-byte multiple => 40

    let size = size_of::<MessageMeta>(); // get the size of the MessageMeta struct
    let align = align_of::<MessageMeta>(); // get the alignment of the MessageMeta struct
    let off_message_id = offset_of!(MessageMeta, message_id);
    let off_timestamp_ns = offset_of!(MessageMeta, timestamp_ns); // get the offset of the timestamp_ns field
    let off_channel_id = offset_of!(MessageMeta, channel_id);
    let off_message_type = offset_of!(MessageMeta, message_type); // get the offset of the message_type field
    let off_sender_pid = offset_of!(MessageMeta, sender_pid);
    let off_sender_runtime = offset_of!(MessageMeta, sender_runtime);
    let off_flags = offset_of!(MessageMeta, flags);
    let off_payload_len = offset_of!(MessageMeta, payload_len);

    println!(
        "MessageMeta => size: {size}, expected: {aligned}, align: {align} (u64 align: {}), offsets: [message_id:{off_message_id}, timestamp_ns:{off_timestamp_ns}, channel_id:{off_channel_id}, message_type:{off_message_type}, sender_pid:{off_sender_pid}, sender_runtime:{off_sender_runtime}, flags:{off_flags}, payload_len:{off_payload_len}]",
        align_of::<u64>()
    );

    // Check if the layout matches the expected values
    assert_eq!(size, aligned);
    assert_eq!(align, align_of::<u64>());
    assert_eq!(off_message_id, 0);
    assert_eq!(off_timestamp_ns, 8);
    assert_eq!(off_channel_id, 16);
    assert_eq!(off_message_type, 20);
    assert_eq!(off_sender_pid, 24);
    assert_eq!(off_sender_runtime, 28);
    assert_eq!(off_flags, 30);
    assert_eq!(off_payload_len, 32);
}

// SlotHeader test removed as SlotHeader struct no longer exists.
// See Buffer::Slot for the current slot layout.
