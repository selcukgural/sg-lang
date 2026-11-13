/* The same program written by hand in C, with no checks. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int64_t encode(int64_t value, uint8_t* target) {
    int64_t rest = value;
    int64_t at = 0;
    while (rest >= 0x80) {
        target[at] = (uint8_t)((rest & 0x7f) | 0x80);
        rest >>= 7;
        at += 1;
    }
    target[at] = (uint8_t)rest;
    return at + 1;
}

static int64_t decode(const uint8_t* source, int64_t count) {
    int64_t value = 0;
    int32_t shift = 0;
    for (int64_t i = 0; i < count; i++) {
        value |= (int64_t)(source[i] & 0x7f) << shift;
        shift += 7;
    }
    return value;
}

int main(void) {
    int32_t limit = 64000000;
    uint64_t* words = calloc((size_t)(limit / 64), sizeof(uint64_t));
    int32_t primes = 0;
    for (int32_t n = 2; n < limit; n++) {
        if ((words[n >> 6] & (1ULL << (n & 63))) != 0) {
            continue;
        }
        primes += 1;
        int64_t multiple = (int64_t)n * n;
        while (multiple < limit) {
            words[multiple >> 6] |= 1ULL << (multiple & 63);
            multiple += n;
        }
    }
    printf("%d\n", primes);
    free(words);

    uint8_t buffer[10] = { 0 };
    int64_t sum = 0;
    int64_t value = 1;
    for (int32_t round = 0; round < 30000000; round++) {
        int64_t length = encode(value, buffer);
        sum += (decode(buffer, length) & 0xffff) + length;
        value = (value * 3 + round) & 0x7fffffffffffLL;
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
