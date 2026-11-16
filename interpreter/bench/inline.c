// The same loop as `inline.sg`, with the step written in the same file.
#include <stdint.h>
#include <stdio.h>

static inline uint64_t mix(uint64_t x) {
    x ^= x >> 33;
    x *= 0xff51afd7ed558ccdULL;
    x ^= x >> 33;
    return x;
}

int main(void) {
    uint64_t acc = 1;
    for (int64_t i = 0; i < 500000000; i++) {
        acc = mix(acc ^ (uint64_t)i);
    }
    printf("%llu\n", (unsigned long long)acc);
    return 0;
}
