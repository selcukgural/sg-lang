// The same as keywords.sg, written by hand: each word is tested by its length and its bytes.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { char* ptr; int64_t len; } word;

static int kind(const char* p, int64_t n) {
    if (n == 2 && memcmp(p, "if", 2) == 0) return 1;
    if (n == 4 && memcmp(p, "else", 4) == 0) return 2;
    if (n == 5 && memcmp(p, "while", 5) == 0) return 3;
    if (n == 3 && memcmp(p, "for", 3) == 0) return 4;
    if (n == 6 && memcmp(p, "return", 6) == 0) return 5;
    if (n == 5 && memcmp(p, "match", 5) == 0) return 6;
    if (n == 6 && memcmp(p, "struct", 6) == 0) return 7;
    if (n == 4 && memcmp(p, "enum", 4) == 0) return 8;
    if (n == 3 && memcmp(p, "var", 3) == 0) return 9;
    if (n == 4 && memcmp(p, "true", 4) == 0) return 10;
    if (n == 5 && memcmp(p, "false", 5) == 0) return 11;
    if (n == 4 && memcmp(p, "null", 4) == 0) return 12;
    return 0;
}

int main(void) {
    static const char* table[] = {
        "if", "else", "while", "for", "return", "match", "struct", "enum", "var", "true", "false", "null",
        "value", "count", "index", "name", "total", "result", "left", "right", "item", "next", "size", "word",
    };
    int64_t count = 1000000;
    word* words = malloc(sizeof(word) * (size_t)count);
    if (!words) return 1;
    int64_t seed = 7;
    for (int64_t i = 0; i < count; i++) {
        seed = (seed * 1103515245 + 12345) % 2147483648;
        const char* picked = table[seed % 24];
        int64_t n = (int64_t)strlen(picked);
        words[i].ptr = malloc((size_t)n);
        if (!words[i].ptr) return 1;
        memcpy(words[i].ptr, picked, (size_t)n);
        words[i].len = n;
    }
    int64_t total = 0;
    for (int64_t round = 0; round < 40; round++) {
        for (int64_t i = 0; i < count; i++) {
            total += kind(words[i].ptr, words[i].len) + round;
        }
    }
    printf("%lld\n", (long long)total);
    for (int64_t i = 0; i < count; i++) free(words[i].ptr);
    free(words);
    return 0;
}
