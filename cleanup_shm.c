#include <sys/mman.h>
#include <fcntl.h>
#include <stdio.h>

int main() {
    printf("Cleaning up DMXP shared memory objects...\n");

    // Try to unlink various possible DMXP shared memory objects
    const char* names[] = {
        "/dmxp_dmxp_alloc",
        "/dmxp_alloc",
        "/dmxp_ovf_ctrl",
        "/dmxp_ovf_data_0",
        "/dmxp_ovf_data_1",
        "/dmxp_ovf_data_2",
        NULL
    };

    for (int i = 0; names[i] != NULL; i++) {
        int result = shm_unlink(names[i]);
        if (result == 0) {
            printf("Cleaned up: %s\n", names[i]);
        }
    }

    printf("Done.\n");
    return 0;
}