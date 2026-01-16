#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>
#include <string.h>

// --- FFI Declarations ---
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

void* dmxp_producer_new(uint32_t channel_id, uint32_t capacity);
int32_t dmxp_producer_send_batch(void* handle, const uint8_t** data_ptrs, const size_t* data_lens, size_t count);
void dmxp_producer_free(void* handle);

void* dmxp_consumer_new(uint32_t channel_id);
int32_t dmxp_consumer_receive_ext(void* handle, int32_t timeout_ms, uint8_t* out_buf, size_t* out_len, FFIMessageMeta* out_meta);
void dmxp_consumer_free(void* handle);

#define DMXP_SUCCESS 0

// --- Benchmark ---

int main() {
    uint32_t channel_id = 202; // Use a new channel
    uint32_t capacity = 1024 * 64; 
    int TOTAL_MESSAGES = 1000000;
    int BATCH_SIZE = 32;
    int ITERATIONS = TOTAL_MESSAGES / BATCH_SIZE;

    printf("Starting C Batch Benchmark (Batch Size: %d)...\n", BATCH_SIZE);
    
    // 1. Setup Producer
    void* producer = dmxp_producer_new(channel_id, capacity);
    if (!producer) { fprintf(stderr, "Failed to create producer\n"); return 1; }

    // 2. Setup Consumer
    void* consumer = dmxp_consumer_new(channel_id);
    if (!consumer) { fprintf(stderr, "Failed to create consumer\n"); return 1; }

    // Prepare Batch
    const uint8_t* payloads[BATCH_SIZE];
    size_t lengths[BATCH_SIZE];
    uint8_t buffer_pool[BATCH_SIZE][32];

    for(int i=0; i<BATCH_SIZE; i++) {
        memset(buffer_pool[i], 'B', 32);
        payloads[i] = buffer_pool[i];
        lengths[i] = 32;
    }

    uint8_t rx_buf[128];
    size_t rx_len = 128;
    
    // Heat up
    dmxp_producer_send_batch(producer, payloads, lengths, BATCH_SIZE);
    for(int k=0; k<BATCH_SIZE; k++) {
        rx_len = 128;
        dmxp_consumer_receive_ext(consumer, 0, rx_buf, &rx_len, NULL);
    }

    // Measure
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);

    for (int i = 0; i < ITERATIONS; i++) {
        // Batch Send
        int res_send = dmxp_producer_send_batch(producer, payloads, lengths, BATCH_SIZE);
        if (res_send != DMXP_SUCCESS) { printf("Batch Send failed at %d: %d\n", i, res_send); break; }

        // Individual Receives (Consumer doesn't have batch receive yet)
        for(int k=0; k<BATCH_SIZE; k++) {
            rx_len = 128;
            int res_recv = dmxp_consumer_receive_ext(consumer, 0, rx_buf, &rx_len, NULL); 
            if (res_recv != DMXP_SUCCESS) { printf("Recv failed at %d sub %d, error %d\n", i, k, res_recv); break; }
        }
    }

    clock_gettime(CLOCK_MONOTONIC, &end);

    // Results
    double elapsed = (end.tv_sec - start.tv_sec) + (end.tv_nsec - start.tv_nsec) / 1e9;
    double msgs_per_sec = (double)(ITERATIONS * BATCH_SIZE) / elapsed;

    printf("\n--- Results ---\n");
    printf("Total Messages: %d\n", ITERATIONS * BATCH_SIZE);
    printf("Time: %.4f s\n", elapsed);
    printf("Throughput: %.2f Messages/sec\n", msgs_per_sec);
    
    dmxp_producer_free(producer);
    dmxp_consumer_free(consumer);
    return 0;
}
