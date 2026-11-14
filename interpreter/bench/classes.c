// The same as classes.sg: a test of each range or byte, written by hand.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int kind(uint8_t c) {
    if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_') return 1;
    if (c >= '0' && c <= '9') return 2;
    if (c == ' ' || c == '\t' || c == '\n' || c == '\r') return 3;
    if (c == '(' || c == ')' || c == '{' || c == '}' || c == '[' || c == ']') return 4;
    if (c == '+' || c == '-' || c == '*' || c == '/' || c == '%' || c == '=' || c == '<' || c == '>') return 5;
    return 0;
}

int main(void) {
    int64_t count = 10000000;
    uint8_t* bytes = malloc((size_t)count);
    if (!bytes) return 1;
    int64_t seed = 7;
    for (int64_t i = 0; i < count; i++) {
        seed = (seed * 1103515245 + 12345) % 2147483648;
        bytes[i] = (uint8_t)(seed % 128);
    }
    int64_t total = 0;
    for (int64_t round = 0; round < 20; round++) {
        for (int64_t i = 0; i < count; i++) {
            total += kind(bytes[i]) + round;
        }
    }
    printf("%lld\n", (long long)total);
    free(bytes);
    return 0;
}
