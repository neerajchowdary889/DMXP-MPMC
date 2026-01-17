package main

/*
#cgo LDFLAGS: -L../../target/debug -ldmxp_kvcache
#include <stdlib.h>
#include <stdint.h>

typedef struct {
    uint64_t message_id;
    uint64_t timestamp_ns;
    uint32_t channel_id;
    uint32_t message_type;
    uint32_t sender_pid;
    uint16_t sender_runtime;
    uint16_t flags;
    uint32_t payload_len;
} FFIMessageMeta;

// Forward declarations of Rust FFI functions
void* dmxp_consumer_new(uint32_t channel_id);
int32_t dmxp_consumer_receive_ext(void* handle, int32_t timeout_ms, uint8_t* out_buf, size_t* out_len, FFIMessageMeta* out_meta);
void dmxp_consumer_free(void* handle);
*/
import "C"
import (
	"fmt"
	"os"
	"unsafe"
)

func main() {
    channelID := uint32(100)
	fmt.Printf("Go Consumer connecting to channel %d...\n", channelID)

	// Create Consumer
	handle := C.dmxp_consumer_new(C.uint32_t(channelID))
	if handle == nil {
		fmt.Println("Failed to create consumer")
		os.Exit(1)
	}
	defer C.dmxp_consumer_free(handle)

	fmt.Println("Waiting for messages from Python...")

	buffer := make([]byte, 1024)
	var meta C.FFIMessageMeta

	for {
		outLen := C.size_t(len(buffer))
		timeoutMs := C.int32_t(1000) // 1 second timeout

		// Call Rust FFI
		// We pass address of buffer[0], address of outLen, address of meta
		res := C.dmxp_consumer_receive_ext(
			handle,
			timeoutMs,
			(*C.uint8_t)(unsafe.Pointer(&buffer[0])),
			&outLen,
			&meta,
		)

		if res == 0 { // DMXP_SUCCESS
			msg := string(buffer[:outLen])
			fmt.Printf("Go Received: '%s'\n", msg)
			fmt.Printf("   Metadata -> PID: %d, MsgID: %d\n", meta.sender_pid, meta.message_id)
		} else if res == -7 { // DMXP_ERROR_TIMEOUT
			// timeout, just loop
			continue
		} else {
			fmt.Printf("Error receiving: %d\n", res)
		}
	}
}
