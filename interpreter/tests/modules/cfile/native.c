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

/* C's `long` in a struct and as a value: 8 bytes on Linux and macOS, 4 on Windows; SG's
   `CLong` and `CULong` are laid out the same. */
typedef struct { uint8_t tag; long value; unsigned long mask; } tagged;

static inline long tagged_value(const void* p) {
    const tagged* t = p;
    return t->tag == 7 && sizeof(tagged) == 3 * sizeof(long) ? t->value : -1;
}

static inline unsigned long long_bits(void) {
    return (unsigned long)sizeof(long) * 8;
}
