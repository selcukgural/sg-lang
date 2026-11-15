/* The same program written by hand in C: xoshiro256** seeded with SplitMix64, and Lemire's way
   to a range. */
#include <stdint.h>
#include <stdio.h>

static uint64_t s[4];

static uint64_t split_mix(uint64_t* x) {
    uint64_t z = (*x += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
}

static inline uint64_t rotl(uint64_t x, int k) {
    return (x << k) | (x >> (64 - k));
}

static inline uint64_t next(void) {
    uint64_t result = rotl(s[1] * 5, 7) * 9;
    uint64_t t = s[1] << 17;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = rotl(s[3], 45);
    return result;
}

static inline int32_t next_int(int32_t low, int32_t high) {
    uint64_t range = (uint64_t)((int64_t)high - (int64_t)low);
    uint64_t m = (next() >> 32) * range;
    if ((m & 0xffffffffULL) < range) {
        uint64_t floor = (0x100000000ULL - range) % range;
        while ((m & 0xffffffffULL) < floor) m = (next() >> 32) * range;
    }
    return (int32_t)((int64_t)low + (int64_t)(m >> 32));
}

int main(void) {
    uint64_t x = 2026;
    for (int i = 0; i < 4; i++) s[i] = split_mix(&x);
    int64_t total = 0;
    for (int i = 0; i < 100000000; i++) total += next_int(0, 1000);
    printf("%lld\n", (long long)total);
    return 0;
}
