#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include "lightmem.h"

int main(void) {
    printf("=== Testing LightMem Linux C Native API ===\n");

    const char* test_db = "/tmp/lightmem_c_linux_test.db";
    remove(test_db);

    LMemHandle* lm = lmem_open(test_db, 0);
    if (!lm) {
        fprintf(stderr, "Failed to open LightMem database!\n");
        return 1;
    }
    printf("✔ Database opened successfully.\n");

    // 1. Remember
    char* rec_json = lmem_remember(
        lm,
        "Linux kernel uses eBPF for programmable in-kernel telemetry",
        "fact",
        "Linux eBPF Architecture",
        "linux,ebpf,kernel",
        0.98f
    );
    if (!rec_json) {
        fprintf(stderr, "lmem_remember failed: %s\n", lmem_get_last_error());
        lmem_close(lm);
        return 1;
    }
    printf("✔ Stored memory: %s\n", rec_json);
    lmem_free_string(rec_json);

    // 2. Recall
    char* recall_json = lmem_recall(lm, "eBPF telemetry", "fact", NULL, 5, -1.0f);
    if (!recall_json) {
        fprintf(stderr, "lmem_recall failed: %s\n", lmem_get_last_error());
        lmem_close(lm);
        return 1;
    }
    printf("✔ Recalled memories: %s\n", recall_json);
    lmem_free_string(recall_json);

    // 3. Stats
    char* stats_json = lmem_stats(lm);
    if (stats_json) {
        printf("✔ Storage stats: %s\n", stats_json);
        lmem_free_string(stats_json);
    }

    // 4. Close & Cleanup
    lmem_close(lm);
    remove(test_db);
    printf("🎉 Linux C Native API verification PASSED 100%%!\n");
    return 0;
}
