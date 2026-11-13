/* The same program written by hand in C, with no checks: two arrays on the stack. */
#include <stdint.h>
#include <stdio.h>

static void to_hex(const uint8_t* source, int64_t count, uint8_t* target) {
    static const uint8_t digits[] = "0123456789abcdef";
    for (int64_t i = 0; i < count; i++) {
        target[i * 2] = digits[source[i] / 16];
        target[i * 2 + 1] = digits[source[i] % 16];
    }
}

static int64_t total(const uint8_t* bytes, int64_t count) {
    int64_t sum = 0;
    for (int64_t i = 0; i < count; i++) {
        sum += bytes[i];
    }
    return sum;
}

int main(void) {
    int64_t sum = 0;
    uint8_t id[32] = { 0 };
    uint8_t text[64] = { 0 };
    for (int32_t round = 0; round < 15000000; round++) {
        for (int64_t i = 0; i < 32; i++) {
            id[i] = (uint8_t)((round + i * 7) % 256);
        }
        to_hex(id, 32, text);
        sum += total(text, 64) + total(text + round % 32, 64 - round % 32);
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
