// The program of C that uses the library of `library.sg` or of `library.c`, through the header
// `sg build --library` writes for the first.
#include <stdio.h>
#include <stdlib.h>

#include "library.h"

int main(void) {
    Stats *stats = stats_new("samples");
    uint32_t state = 2463534242u;
    for (int i = 0; i < 50000000; i++) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        stats_add(stats, (double)(state % 1000000) / 1000.0);
    }
    printf("%.6f %.3f\n", stats_mean(stats), stats_spread(stats));
    stats_free(stats);
    uint64_t length = (uint64_t)256 << 20;
    uint8_t *bytes = malloc(length);
    if (!bytes) return 1;
    for (uint64_t i = 0; i < length; i++) bytes[i] = (uint8_t)(i * 2654435761u >> 24);
    printf("%llu\n", (unsigned long long)fnv(bytes, length));
    free(bytes);
    return 0;
}
