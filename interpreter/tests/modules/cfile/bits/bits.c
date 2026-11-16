// The C of the `bits` module: a builtin of the compiler, and zlib's `crc32`.
#include <stdint.h>
#include <string.h>
#include <zlib.h>

static inline int32_t bits_in(uint64_t n) {
    return __builtin_popcountll(n);
}

static uint32_t crc_of(const char *text) {
    return (uint32_t)crc32(0L, (const Bytef *)text, (uInt)strlen(text));
}
