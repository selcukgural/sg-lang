/* The same program written by hand in C, with no checks. The bytes are read eight at a time
   with `memcpy`, the way xxHash itself reads them. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const uint64_t P1 = 11400714785092573703ULL;
static const uint64_t P2 = 14029467366897019727ULL;
static const uint64_t P3 = 1609587929392839161ULL;
static const uint64_t P4 = 9650029242287828579ULL;
static const uint64_t P5 = 2870177450012600261ULL;

static uint64_t rotl(uint64_t value, int by) {
    return value << by | value >> (64 - by);
}

static uint64_t read64(const uint8_t* data) {
    uint64_t value;
    memcpy(&value, data, 8);
    return value;
}

static uint64_t read32(const uint8_t* data) {
    uint32_t value;
    memcpy(&value, data, 4);
    return value;
}

static uint64_t round64(uint64_t acc, uint64_t input) {
    acc += input * P2;
    return rotl(acc, 31) * P1;
}

static uint64_t merge(uint64_t acc, uint64_t value) {
    acc ^= round64(0, value);
    return acc * P1 + P4;
}

static uint64_t xxh64(const uint8_t* data, int64_t length, uint64_t seed) {
    int64_t at = 0;
    uint64_t hash;
    if (length >= 32) {
        uint64_t v1 = seed + P1 + P2, v2 = seed + P2, v3 = seed, v4 = seed - P1;
        while (at + 32 <= length) {
            v1 = round64(v1, read64(data + at));
            v2 = round64(v2, read64(data + at + 8));
            v3 = round64(v3, read64(data + at + 16));
            v4 = round64(v4, read64(data + at + 24));
            at += 32;
        }
        hash = rotl(v1, 1) + rotl(v2, 7) + rotl(v3, 12) + rotl(v4, 18);
        hash = merge(hash, v1);
        hash = merge(hash, v2);
        hash = merge(hash, v3);
        hash = merge(hash, v4);
    } else {
        hash = seed + P5;
    }
    hash += (uint64_t)length;
    while (at + 8 <= length) {
        hash ^= round64(0, read64(data + at));
        hash = rotl(hash, 27) * P1 + P4;
        at += 8;
    }
    if (at + 4 <= length) {
        hash ^= read32(data + at) * P1;
        hash = rotl(hash, 23) * P2 + P3;
        at += 4;
    }
    while (at < length) {
        hash ^= data[at] * P5;
        hash = rotl(hash, 11) * P1;
        at += 1;
    }
    hash ^= hash >> 33;
    hash *= P2;
    hash ^= hash >> 29;
    hash *= P3;
    hash ^= hash >> 32;
    return hash;
}

int main(void) {
    int64_t size = 1048576;
    uint8_t* bytes = malloc((size_t)size);
    uint64_t state = 0x9e3779b97f4a7c15ULL;
    for (int64_t i = 0; i < size; i++) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes[i] = (uint8_t)(state & 0xff);
    }
    printf("%llu\n", (unsigned long long)xxh64(bytes, size, 0));
    printf("%llu\n", (unsigned long long)xxh64((const uint8_t*)"abc", 3, 0));

    uint64_t total = 0;
    for (int32_t seed = 0; seed < 2000; seed++) {
        int64_t length = size - seed % 61;
        total ^= xxh64(bytes, length, (uint64_t)seed);
    }
    printf("%llu\n", (unsigned long long)total);
    free(bytes);
    return 0;
}
