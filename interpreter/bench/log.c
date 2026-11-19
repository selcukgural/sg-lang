/* The same program written by hand in C: the level in an atomic global, read before each line. */
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>

/* 0 trace, 1 debug, 2 info. */
static _Atomic int log_from = 2;

int main(void) {
    int64_t sum = 0;
    for (int64_t i = 0; i < 1000000000; i++) {
        if (atomic_load_explicit(&log_from, memory_order_relaxed) <= 1) {
            fprintf(stderr, "DEBUG step %lld, sum %lld\n", (long long)i, (long long)sum);
        }
        sum = (int64_t)((uint64_t)sum + (uint64_t)(i ^ (sum >> 3)));
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
