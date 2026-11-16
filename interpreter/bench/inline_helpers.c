// The C file of `inline.sg`: a step of a hash, as a header would give it.
#include <stdint.h>

static inline uint64_t mix(uint64_t x) {
    x ^= x >> 33;
    x *= 0xff51afd7ed558ccdULL;
    x ^= x >> 33;
    return x;
}
