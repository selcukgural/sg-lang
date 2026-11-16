// The C file of the `cfile` program: functions a header would give, and a variable.
#include <stdint.h>

#define SQUARE(x) ((x) * (x))

static int64_t hits = 0;

typedef struct { int32_t x; int32_t y; } point;

static point origin = { 3, 4 };

static inline int64_t square(int64_t n) {
    hits += 1;
    return SQUARE(n);
}

static inline int32_t count_bits(uint64_t n) {
    return __builtin_popcountll(n);
}
